//! Window policy for the three published profiles, all on the New York calendar.
//!
//! * PW: previous week, Sunday 18:00 -> Friday 18:00 NY, fixed at the Sunday open.
//! * PS: previous closed session. Frozen at the week open by default, so at a
//!   Sunday 18:00 open it is Thursday 18:00 -> Friday 18:00. PoC only.
//!   `PS_MODE=rolling` re-anchors it to the latest close instead.
//!   Weekend buckets (Fri 18:00 -> Sun 18:00 filler) are walked over.
//! * CW: current week, week open -> last completed weekday 18:00 close. It first
//!   publishes after the `CW_FIRST_PUBLISH_WEEKDAY` close (Monday by default)
//!   and refreshes on every later weekday close. It resets at the Sunday open.

use chrono::NaiveDate;

use super::histogram::{histogram as build_histogram, Histogram, ProfileModel};
use super::timeframe::{prepare_input, LowerTf};
use crate::ny::{
    close_on, is_weekend_close, last_trading_close_date, latest_close_date, shift_days,
    week_open_date, weekday_name, weekday_rank,
};
use crate::store::CandleStore;
use crate::types::{PendingWindow, VpCandle};

/// How many trading sessions the previous-session walk may inspect.
pub const MAX_SESSION_LOOKBACK: usize = 5;
/// A bar counts as having a body when open and close differ by more than this.
const BODY_EPSILON: f64 = f64::EPSILON;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsMode {
    /// Freeze the previous session at the week open (default).
    WeekOpen,
    /// Re-anchor to the latest closed session as time advances.
    Rolling,
}

impl PsMode {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "rolling" | "live" => PsMode::Rolling,
            _ => PsMode::WeekOpen,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PsMode::WeekOpen => "week_open",
            PsMode::Rolling => "rolling",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowSettings {
    pub model: ProfileModel,
    pub lower_tf: LowerTf,
    /// Trading weekday rank (1 = Mon ... 5 = Fri) on which CW first publishes.
    pub cw_first_weekday: u32,
    pub ps_mode: PsMode,
}

/// One computed profile and the bounds it covers.
#[derive(Debug, Clone)]
pub struct BuiltProfile {
    pub start: i64,
    pub end: i64,
    pub input_interval: &'static str,
    pub input_bars: usize,
    pub histogram: Histogram,
}

/// Everything computed for one refresh.
#[derive(Debug, Clone)]
pub struct WindowSet {
    pub pw: Option<BuiltProfile>,
    pub ps: Option<BuiltProfile>,
    pub cw: Option<BuiltProfile>,
    pub pending: Vec<PendingWindow>,
}

/// Compute PW, PS, and CW for `now_ms` from the stored closed bars.
pub fn compute_windows(store: &CandleStore, now_ms: i64, settings: &WindowSettings) -> WindowSet {
    let latest = latest_close_date(now_ms);
    let week_open = week_open_date(now_ms);
    let mut pending = Vec::new();

    // PW: previous Sunday 18:00 -> Friday 18:00.
    let pw_start = close_on(shift_days(week_open, -7));
    let pw_end = close_on(shift_days(week_open, -2));
    let pw = build_window(store, pw_start, pw_end, settings);
    if pw.is_none() {
        pending.push(PendingWindow {
            window: "PW".into(),
            reason: "no complete 1m/15m/1h history for the previous week".into(),
            publish_at_ms: None,
        });
    }

    // PS: previous closed session, anchored to the week open unless rolling.
    let ps_anchor = match settings.ps_mode {
        PsMode::WeekOpen => week_open,
        PsMode::Rolling => latest,
    };
    let ps = find_previous_session(store, ps_anchor, settings);
    if ps.is_none() {
        pending.push(PendingWindow {
            window: "PS".into(),
            reason: "no complete closed session with traded bars in the lookback".into(),
            publish_at_ms: None,
        });
    }

    // CW: week open -> last completed weekday close, gated on the first publish day.
    let last_close = last_trading_close_date(latest);
    let cw_first = i64::from(settings.cw_first_weekday);
    let cw_visible =
        last_close > week_open && weekday_rank(last_close) >= settings.cw_first_weekday;
    let cw = if cw_visible {
        build_window(store, close_on(week_open), close_on(last_close), settings)
    } else {
        None
    };
    if !cw_visible {
        pending.push(PendingWindow {
            window: "CW".into(),
            reason: format!(
                "CW publishes after the {} 18:00 NY close",
                weekday_name(settings.cw_first_weekday)
            ),
            publish_at_ms: Some(close_on(shift_days(week_open, cw_first))),
        });
    } else if cw.is_none() {
        pending.push(PendingWindow {
            window: "CW".into(),
            reason: "no complete 1m/15m/1h history for the current week".into(),
            publish_at_ms: None,
        });
    }

    WindowSet {
        pw,
        ps,
        cw,
        pending,
    }
}

/// Walk back from the `anchor` close over whole sessions. Weekend closes are
/// skipped, so filler from Fri 18:00 to Sun 18:00 never becomes the previous
/// session. A weekday session with no traded bars is also skipped, because
/// Node 1 emits closed-market buckets as bodyless filler. Missing or partial
/// history on a weekday is not skipped: the walk returns `None`, failing closed.
fn find_previous_session(
    store: &CandleStore,
    anchor: NaiveDate,
    settings: &WindowSettings,
) -> Option<BuiltProfile> {
    let mut close = anchor;
    for _ in 0..MAX_SESSION_LOOKBACK {
        if !is_weekend_close(close) {
            let start = close_on(shift_days(close, -1));
            let end = close_on(close);
            let (_, bars) = store.window(start, end)?;
            let has_body = bars
                .iter()
                .any(|bar| (bar.close - bar.open).abs() > BODY_EPSILON);
            if has_body {
                return profile_from_bars(start, end, &bars, settings);
            }
        }
        close = shift_days(close, -1);
    }
    None
}

/// Build a window from stored bars when the store covers it.
fn build_window(
    store: &CandleStore,
    start: i64,
    end: i64,
    settings: &WindowSettings,
) -> Option<BuiltProfile> {
    let (_, bars) = store.window(start, end)?;
    profile_from_bars(start, end, &bars, settings)
}

fn profile_from_bars(
    start: i64,
    end: i64,
    bars: &[VpCandle],
    settings: &WindowSettings,
) -> Option<BuiltProfile> {
    let (input, input_interval) = prepare_input(bars, settings.lower_tf);
    let histogram = build_histogram(&input, &settings.model)?;
    Some(BuiltProfile {
        start,
        end,
        input_interval,
        input_bars: input.len(),
        histogram,
    })
}

/// Profile for an explicit `[start, end)` range. Accepts any range with at least
/// two stored bars, including partial coverage.
pub fn custom_profile(
    store: &CandleStore,
    start: i64,
    end: i64,
    settings: &WindowSettings,
) -> Result<BuiltProfile, String> {
    if end <= start {
        return Err("end must be after start".to_string());
    }
    let Some((_, bars)) = store.finest_bars(start, end) else {
        return Err("no stored bars in the requested range".to_string());
    };
    profile_from_bars(start, end, &bars, settings)
        .ok_or_else(|| "requested range holds no traded volume".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ny::utc_ms;
    use crate::vp::histogram::{RowMode, DEFAULT_ROWS};

    const MINUTE: i64 = 60_000;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("date")
    }

    fn settings(cw_first_weekday: u32, ps_mode: PsMode) -> WindowSettings {
        WindowSettings {
            model: ProfileModel {
                row_mode: RowMode::Rows,
                rows: DEFAULT_ROWS,
                ..ProfileModel::default()
            },
            lower_tf: LowerTf::Tv,
            cw_first_weekday,
            ps_mode,
        }
    }

    /// Fill every minute in `[start, end)` with a 1m bar at `price`.
    /// `body` gives open != close. Volume is 10 per bar.
    fn fill(store: &mut CandleStore, start: i64, end: i64, price: f64, body: bool) {
        let mut time = start;
        while time < end {
            let close = if body { price + 0.2 } else { price };
            store.upsert(
                "1m",
                VpCandle {
                    time,
                    open: price,
                    high: price.max(close) + 0.1,
                    low: price.min(close) - 0.1,
                    close,
                    volume: 10.0,
                    source: "test".into(),
                },
            );
            time += MINUTE;
        }
    }

    /// A full trading week (Sun 18:00 -> Fri 18:00) of traded 1m bars.
    fn fill_trading_week(store: &mut CandleStore, sunday: NaiveDate, price: f64) {
        let start = close_on(sunday);
        let end = close_on(shift_days(sunday, 5));
        fill(store, start, end, price, true);
    }

    #[test]
    fn ps_walk_back_at_sunday_open_is_thursday_to_friday() {
        // Sunday 2026-10-25 18:00 NY is the week open.
        let sunday = date(2026, 10, 25);
        let now = close_on(sunday);
        let mut store = CandleStore::default();
        // Thu 22 Oct 18:00 -> Fri 23 Oct 18:00: the real session, traded at 4100.
        fill(
            &mut store,
            close_on(date(2026, 10, 22)),
            close_on(date(2026, 10, 23)),
            4100.0,
            true,
        );
        // Fri 23 Oct 18:00 -> Sun 25 Oct 18:00: weekend filler at 4200 with no body.
        fill(
            &mut store,
            close_on(date(2026, 10, 23)),
            close_on(sunday),
            4200.0,
            false,
        );

        let set = compute_windows(&store, now, &settings(1, PsMode::WeekOpen));
        let ps = set.ps.expect("PS at week open");
        assert_eq!(ps.start, close_on(date(2026, 10, 22)));
        assert_eq!(ps.end, close_on(date(2026, 10, 23)));
        assert_eq!(ps.input_interval, "1m");
        // The weekend filler at 4200 never leaks into the profile.
        assert!(
            (ps.histogram.poc() - 4100.1).abs() < 0.6,
            "PoC {}",
            ps.histogram.poc()
        );
    }

    #[test]
    fn ps_is_frozen_for_the_week_by_default_and_rolls_only_when_asked() {
        let mut store = CandleStore::default();
        // Thu 22 Oct -> Fri 23 Oct session.
        fill(
            &mut store,
            close_on(date(2026, 10, 22)),
            close_on(date(2026, 10, 23)),
            4100.0,
            true,
        );
        // Sun 25 Oct 18:00 -> Mon 26 Oct 18:00 session.
        fill(
            &mut store,
            close_on(date(2026, 10, 25)),
            close_on(date(2026, 10, 26)),
            4110.0,
            true,
        );
        // Monday 18:30 NY: the Monday close has just printed.
        let monday_evening = utc_ms(2026, 10, 26, 22, 30);
        let frozen = compute_windows(&store, monday_evening, &settings(1, PsMode::WeekOpen));
        assert_eq!(
            frozen.ps.expect("frozen PS").start,
            close_on(date(2026, 10, 22)),
            "week-open freeze keeps the Thu->Fri session"
        );

        let rolling = compute_windows(&store, monday_evening, &settings(1, PsMode::Rolling));
        assert_eq!(
            rolling.ps.expect("rolling PS").start,
            close_on(date(2026, 10, 25)),
            "rolling re-anchors to the latest closed session (Sun->Mon)"
        );
    }

    #[test]
    fn pw_and_cw_use_the_recorded_week_boundaries() {
        let mut store = CandleStore::default();
        // Previous week: Sun 11 Oct 18:00 -> Fri 16 Oct 18:00.
        fill_trading_week(&mut store, date(2026, 10, 11), 4150.0);
        // Current week: Sun 18 Oct 18:00 -> Thu 22 Oct 18:00 (plus Friday close later).
        fill(
            &mut store,
            close_on(date(2026, 10, 18)),
            close_on(date(2026, 10, 22)),
            4120.0,
            true,
        );
        // Thursday 22 Oct 2026 18:30 NY, after the Thursday close.
        let now = utc_ms(2026, 10, 22, 22, 30);
        let set = compute_windows(&store, now, &settings(1, PsMode::WeekOpen));

        let pw = set.pw.expect("PW");
        assert_eq!(pw.start, close_on(date(2026, 10, 11)));
        assert_eq!(pw.end, close_on(date(2026, 10, 16)));
        assert_eq!(pw.input_interval, "5m");

        let cw = set.cw.expect("CW visible after Thursday close");
        assert_eq!(cw.start, close_on(date(2026, 10, 18)));
        assert_eq!(cw.end, close_on(date(2026, 10, 22)));
    }

    #[test]
    fn cw_first_publish_is_monday_close_by_default_and_wednesday_when_set() {
        let mut store = CandleStore::default();
        // Week open Sun 25 Oct 2026 with bars through Mon 26 Oct 18:00.
        fill(
            &mut store,
            close_on(date(2026, 10, 25)),
            close_on(date(2026, 10, 26)),
            4110.0,
            true,
        );

        // Monday 10:00 NY: before the first publish, so CW is pending for Mon 18:00.
        let monday_morning = utc_ms(2026, 10, 26, 14, 0);
        let mon_pending = compute_windows(&store, monday_morning, &settings(1, PsMode::WeekOpen));
        assert!(mon_pending.cw.is_none());
        let cw_note = mon_pending
            .pending
            .iter()
            .find(|p| p.window == "CW")
            .expect("CW pending");
        assert_eq!(cw_note.publish_at_ms, Some(close_on(date(2026, 10, 26))));

        // Monday 18:30 NY: first publish has happened.
        let monday_evening = utc_ms(2026, 10, 26, 22, 30);
        let mon_live = compute_windows(&store, monday_evening, &settings(1, PsMode::WeekOpen));
        let cw = mon_live.cw.expect("CW after Monday close");
        assert_eq!(cw.end, close_on(date(2026, 10, 26)));

        // With Wednesday first publish, Monday evening is still pending.
        let wed_cfg = compute_windows(&store, monday_evening, &settings(3, PsMode::WeekOpen));
        assert!(wed_cfg.cw.is_none());
        let wed_note = wed_cfg
            .pending
            .iter()
            .find(|p| p.window == "CW")
            .expect("CW pending for Wed");
        assert_eq!(wed_note.publish_at_ms, Some(close_on(date(2026, 10, 28))));
    }

    #[test]
    fn custom_profile_rejects_inverted_and_empty_ranges() {
        let store = CandleStore::default();
        let cfg = settings(1, PsMode::WeekOpen);
        assert!(custom_profile(&store, 10, 5, &cfg).is_err());
        assert!(custom_profile(&store, 0, MINUTE * 10, &cfg).is_err());
    }
}
