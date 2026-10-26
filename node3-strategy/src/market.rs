//! Market book: the closed-bar store plus every profile, level, and drawing
//! Node 3 publishes. Node 1's `/levels` is never consulted.
//!
//! `recompute` returns whether anything visible changed. Timestamps move only
//! when their payload changes, so unchanged data is not re-broadcast.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::drawings;
use crate::ny::DAY_MS;
use crate::store::CandleStore;
use crate::types::{
    DrawingsPayload, ProfileMeta, SeriesStat, VpCandle, VpLevels, WindowStatus,
};
use crate::vp::windows::{compute_windows, custom_profile, BuiltProfile, WindowSettings, WindowSet};
use crate::vp::{audit_from_profile, VpAudit};

pub struct MarketBook {
    store: CandleStore,
    settings: WindowSettings,
    retention_ms: i64,
    levels: Vec<VpLevels>,
    audits: BTreeMap<String, VpAudit>,
    drawings: DrawingsPayload,
    windows: Vec<WindowStatus>,
}

impl MarketBook {
    pub fn new(config: &Config) -> Self {
        Self {
            store: CandleStore::default(),
            settings: config.window_settings(),
            retention_ms: config.retention_days * DAY_MS,
            levels: Vec::new(),
            audits: BTreeMap::new(),
            drawings: DrawingsPayload {
                timestamp: 0,
                items: Vec::new(),
                pending: Vec::new(),
            },
            windows: Vec::new(),
        }
    }

    /// Store one closed bar. Returns false for an unknown resolution.
    pub fn upsert_closed(&mut self, interval: &str, candle: VpCandle) -> bool {
        self.store.upsert(interval, candle)
    }

    pub fn series_stats(&self) -> Vec<SeriesStat> {
        self.store.stats()
    }

    /// Rebuild every window from the store. Returns `true` when the levels,
    /// audits, or drawings changed.
    pub fn recompute(&mut self, now_ms: i64) -> bool {
        self.store.prune_before(now_ms - self.retention_ms);
        let set = compute_windows(&self.store, now_ms, &self.settings);
        let (levels, audits, statuses) = assemble(&self.store, &set, now_ms);
        let drawings = drawings::build(&levels, &audits, &set.pending, now_ms);

        self.windows = statuses;
        let levels_changed = !same_levels(&self.levels, &levels);
        let audits_changed = audits != self.audits;
        let drawings_changed =
            drawings.items != self.drawings.items || drawings.pending != self.drawings.pending;

        if levels_changed {
            self.levels = levels;
        }
        if audits_changed {
            self.audits = audits;
        }
        if drawings_changed {
            self.drawings = drawings;
        }
        levels_changed || audits_changed || drawings_changed
    }

    pub fn levels(&self) -> Vec<VpLevels> {
        self.levels.clone()
    }

    pub fn drawings(&self) -> DrawingsPayload {
        self.drawings.clone()
    }

    pub fn windows(&self) -> Vec<WindowStatus> {
        self.windows.clone()
    }

    /// Published audit for `PW`, `PS`, or `CW`.
    pub fn audit(&self, window: &str) -> Option<VpAudit> {
        self.audits.get(window).cloned()
    }

    pub fn vp_ready(&self) -> bool {
        !self.levels.is_empty()
    }

    /// Recompute one window with a different row count. Rows are not part of
    /// the published profile, so this never changes stored state.
    pub fn audit_with_rows(&self, window: &str, rows: usize, now_ms: i64) -> Option<VpAudit> {
        let mut settings = self.settings;
        settings.model.rows = rows;
        let set = compute_windows(&self.store, now_ms, &settings);
        let profile = match window {
            "PW" => set.pw.as_ref(),
            "PS" => set.ps.as_ref(),
            "CW" => set.cw.as_ref(),
            _ => None,
        }?;
        Some(audit_from_profile(window, profile))
    }

    /// Profile for an explicit `[start, end)` range.
    pub fn custom_audit(
        &self,
        start: i64,
        end: i64,
        rows: Option<usize>,
    ) -> Result<VpAudit, String> {
        let mut settings = self.settings;
        if let Some(rows) = rows {
            settings.model.rows = rows;
        }
        let profile: BuiltProfile = custom_profile(&self.store, start, end, &settings)?;
        Ok(audit_from_profile("CUSTOM", &profile))
    }
}

/// Levels, audits, and window status for one computed set.
fn assemble(
    store: &CandleStore,
    set: &WindowSet,
    now_ms: i64,
) -> (Vec<VpLevels>, BTreeMap<String, VpAudit>, Vec<WindowStatus>) {
    let mut levels = Vec::new();
    let mut audits = BTreeMap::new();
    let mut statuses = Vec::new();
    let windows: [(&str, &Option<BuiltProfile>); 3] =
        [("PW", &set.pw), ("PS", &set.ps), ("CW", &set.cw)];
    for (window, profile) in windows {
        match profile {
            Some(profile) => {
                // Sunday open: the open of the bar that starts the PW window.
                let sunday_open = if window == "PW" {
                    store.open_at(profile.start)
                } else {
                    None
                };
                levels.push(level_from(window, profile, sunday_open, now_ms));
                audits.insert(window.to_string(), audit_from_profile(window, profile));
                statuses.push(WindowStatus {
                    window: window.into(),
                    published: true,
                    start: Some(profile.start),
                    end: Some(profile.end),
                    input_interval: Some(profile.input_interval.into()),
                    note: None,
                });
            }
            None => statuses.push(WindowStatus {
                window: window.into(),
                published: false,
                start: None,
                end: None,
                input_interval: None,
                note: set
                    .pending
                    .iter()
                    .find(|pending| pending.window == window)
                    .map(|pending| pending.reason.clone()),
            }),
        }
    }
    (levels, audits, statuses)
}

/// PS publishes PoC only. PW and CW publish PoC, VaH, and VaL.
fn level_from(
    window: &str,
    profile: &BuiltProfile,
    sunday_open: Option<f64>,
    now_ms: i64,
) -> VpLevels {
    let hist = &profile.histogram;
    let (vah, val) = if window == "PS" {
        (None, None)
    } else {
        (Some(hist.vah()), Some(hist.val()))
    };
    VpLevels {
        window: window.into(),
        poc: hist.poc(),
        vah,
        val,
        start: profile.start,
        end: profile.end,
        timestamp: now_ms,
        sunday_open,
        meta: ProfileMeta {
            row_mode: hist.model.row_mode.as_str().into(),
            row_height: hist.row_height,
            rows: hist.rows.len(),
            range_low: hist.range_low,
            range_high: hist.range_high,
            va_pct: hist.model.va_pct,
            total_volume: hist.total_volume,
            input_interval: profile.input_interval.into(),
            input_bars: profile.input_bars,
        },
    }
}

/// Equal content, ignoring the publish timestamp.
fn same_levels(old: &[VpLevels], new: &[VpLevels]) -> bool {
    old.len() == new.len()
        && old.iter().zip(new).all(|(a, b)| {
            let mut a = a.clone();
            let mut b = b.clone();
            a.timestamp = 0;
            b.timestamp = 0;
            a == b
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ny::{close_on, utc_ms};
    use crate::vp::windows::PsMode;
    use chrono::NaiveDate;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("date")
    }

    fn fill(book: &mut MarketBook, start: i64, end: i64, price: f64) {
        let mut time = start;
        while time < end {
            book.upsert_closed(
                "1m",
                VpCandle {
                    time,
                    open: price,
                    high: price + 0.6,
                    low: price - 0.4,
                    close: price + 0.2,
                    volume: 10.0,
                    source: "test".into(),
                },
            );
            time += 60_000;
        }
    }

    #[test]
    fn publishes_pw_with_sunday_open_and_ps_as_poc_only() {
        let config = Config::default();
        let mut book = MarketBook::new(&config);
        // Previous trading week, Sun 11 Oct 18:00 -> Fri 16 Oct 18:00.
        fill(
            &mut book,
            close_on(date(2026, 10, 11)),
            close_on(date(2026, 10, 16)),
            4150.0,
        );
        // Sunday 18 Oct 2026 18:30 NY: the new week has just opened.
        let now = utc_ms(2026, 10, 18, 22, 30);
        assert!(book.recompute(now));

        let levels = book.levels();
        let pw = levels.iter().find(|l| l.window == "PW").expect("PW level");
        assert_eq!(pw.start, close_on(date(2026, 10, 11)));
        assert!(pw.vah.is_some() && pw.val.is_some());
        assert_eq!(pw.sunday_open, Some(4150.0), "open of the first PW bar");

        let ps = levels.iter().find(|l| l.window == "PS").expect("PS level");
        assert!(ps.vah.is_none() && ps.val.is_none(), "PS publishes PoC only");
        assert!(ps.sunday_open.is_none());
        assert!(levels.iter().all(|l| l.window != "CW"), "CW not published before Monday close");

        let drawings = book.drawings();
        assert!(drawings.items.iter().any(|item| item.kind == "frvp" && item.window == "PW"));
        assert!(drawings.items.iter().any(|item| item.kind == "sunday_open"));
        assert!(drawings.pending.iter().any(|p| p.window == "CW"));
    }

    #[test]
    fn unchanged_data_is_not_reported_as_a_change() {
        let config = Config::default();
        let mut book = MarketBook::new(&config);
        fill(
            &mut book,
            close_on(date(2026, 10, 11)),
            close_on(date(2026, 10, 16)),
            4150.0,
        );
        let now = utc_ms(2026, 10, 18, 22, 30);
        assert!(book.recompute(now));
        let first_timestamp = book.levels()[0].timestamp;
        assert!(!book.recompute(now + 5_000), "same content must not re-publish");
        assert_eq!(book.levels()[0].timestamp, first_timestamp);
    }

    #[test]
    fn custom_range_and_row_override_do_not_mutate_published_state() {
        let config = Config {
            ps_mode: PsMode::WeekOpen,
            ..Config::default()
        };
        let mut book = MarketBook::new(&config);
        fill(
            &mut book,
            close_on(date(2026, 10, 11)),
            close_on(date(2026, 10, 16)),
            4150.0,
        );
        let now = utc_ms(2026, 10, 18, 22, 30);
        book.recompute(now);
        let before = book.levels();
        let custom = book
            .custom_audit(close_on(date(2026, 10, 12)), close_on(date(2026, 10, 13)), Some(64))
            .expect("custom");
        assert_eq!(custom.rows, 64);
        assert_eq!(custom.window, "CUSTOM");
        assert_eq!(book.levels(), before);
        assert!(book.audit_with_rows("PW", 32, now).is_some());
    }
}
