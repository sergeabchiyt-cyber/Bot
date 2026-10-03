//! Session, week, and swing volume profiles.
//!
//! This module is the orchestration layer: it owns the retained candle
//! history and refreshes the profile windows. The math lives in focused
//! submodules so each window's definition can be read (and tested) alone:
//!
//! * `session` — 18:00 → 18:00 America/New_York sessions (PS).
//! * `weekly` — trading-week anchors: PW and the CW accumulation window.
//! * `swing` — the single most-recent directional leg.
//! * `histogram` — the fixed-bin POC / value-area calculation.
//!
//! Window semantics:
//!
//! * `PW` = previous trading week (Sun 18:00 NY open → Fri 18:00 NY close),
//!   drawn/served unchanged for the whole of the current week.
//! * `PS` = the last **closed** session (18:00 NY → 18:00 NY), so it rolls
//!   forward at every session close instead of being frozen at boot.
//! * `CW` = current week from the week open (the first session after
//!   Friday's 18:00 close) through the last completed 18:00 daily close; it
//!   remains frozen during the in-progress day, appears once Monday's
//!   session closes, and resets each week.
//! * `SWING_BULL` / `SWING_BEAR` = the most recent confirmed
//!   pivot-to-pivot directional leg (low → high / high → low); only one
//!   exists at a time.

mod histogram;
mod session;
mod swing;
mod weekly;

use chrono::Utc;

use crate::types::{VpCandle, VpLevels};

/// Keep the complete fixed 2,000-candle Sifting seed (about 21 days of 15m
/// bars) plus room for live closes and session/week boundaries.
const RETAIN_MS: i64 = 35 * 24 * 60 * 60 * 1000;

pub struct VolumeProfileEngine {
    pub pw_levels: Option<VpLevels>,
    pub ps_levels: Option<VpLevels>,
    pub cw_levels: Option<VpLevels>,
    /// The most recent completed pivot-to-pivot swing profile. Its label is
    /// `SWING_BULL` or `SWING_BEAR`, so consumers can target the correct leg
    /// without guessing from a time-window profile.
    pub swing_levels: Option<VpLevels>,
    /// End timestamp (ms) of the session PS currently describes.
    ps_session_end: Option<i64>,
    /// The last completed daily session included in CW. CW is deliberately
    /// frozen between daily closes rather than moving on every 15m candle.
    cw_session_end: Option<i64>,
    /// Week anchor used by the frozen CW snapshot. This lets CW reset at the
    /// Sunday 18:00 NY week boundary before the first new daily close.
    cw_week_start: Option<i64>,
    /// Most recent 18:00 NY close seen by `VolumeProfileEngine::recompute`.
    /// The refresh timer compares against this — not against PS's own end,
    /// which skips over empty weekend/holiday sessions — so each close is
    /// detected exactly once and empty sessions don't retrigger every tick.
    last_close_seen: Option<i64>,
    candles: Vec<VpCandle>,
}

impl VolumeProfileEngine {
    pub fn new() -> Self {
        Self {
            pw_levels: None,
            ps_levels: None,
            cw_levels: None,
            swing_levels: None,
            ps_session_end: None,
            cw_session_end: None,
            cw_week_start: None,
            last_close_seen: None,
            candles: Vec::new(),
        }
    }

    fn upsert_candle(&mut self, candle: VpCandle) {
        if let Some(existing) = self.candles.iter_mut().find(|c| c.time == candle.time) {
            *existing = candle;
        } else {
            self.candles.push(candle);
        }
        self.candles.sort_by_key(|c| c.time);
    }

    /// Add or replace one candle and recompute all profiles.
    pub fn ingest_candle(&mut self, candle: VpCandle) {
        self.upsert_candle(candle);
        self.recompute(Utc::now().timestamp_millis());
    }

    /// Seed the engine in one pass. This avoids recomputing a 2,000-candle
    /// history 2,000 times during cold start.
    pub fn ingest_candles(&mut self, candles: Vec<VpCandle>) {
        for candle in candles {
            self.upsert_candle(candle);
        }
        self.recompute(Utc::now().timestamp_millis());
    }

    /// Recompute every window against `now_ms`.
    ///
    /// PW  = previous trading week   (Sun 18:00 NY open → Fri 18:00 NY close),
    ///       drawn/served unchanged for the whole of the current week.
    /// PS  = the last **closed** session (18:00 NY → 18:00 NY), so it rolls
    ///       forward at every session close instead of being computed once.
    /// CW  = current week through the last completed 18:00 NY daily close; it
    ///       remains frozen during the in-progress day and resets each week.
    /// SWING = the most recent completed pivot-to-pivot directional leg.
    pub fn recompute(&mut self, now_ms: i64) {
        let week_start = weekly::most_recent_week_start_utc(now_ms);
        let (pw_start, pw_end) = weekly::previous_week_bounds_utc(now_ms);

        let retain_floor = pw_start.min(now_ms - RETAIN_MS);
        // Keep out-of-order candles in storage; the individual windows apply
        // their own time bounds. This also lets a historical backfill arrive
        // before a caller advances its synthetic/test clock.
        self.candles.retain(|c| c.time >= retain_floor);

        let pw: Vec<_> = self
            .candles
            .iter()
            .filter(|c| c.time >= pw_start && c.time < pw_end)
            .cloned()
            .collect();
        // CW is a completed-daily-session snapshot. A live/in-progress day
        // must never move its levels. The 18:00 NY boundary is also the
        // existing session close, so it is DST-safe and consistent with
        // PS. Before the first close of a new week, explicitly clear the old
        // week's CW profile.
        let completed_day_end = session::last_session_close_utc(now_ms);
        let cw_needs_refresh = self.cw_session_end != Some(completed_day_end)
            || self.cw_week_start != Some(week_start);
        if cw_needs_refresh {
            if completed_day_end > week_start {
                let cw: Vec<_> = self
                    .candles
                    .iter()
                    .filter(|c| c.time >= week_start && c.time < completed_day_end)
                    .cloned()
                    .collect();
                self.cw_levels = histogram::compute(
                    &cw,
                    "CW",
                    week_start,
                    completed_day_end,
                    None,
                    "neutral",
                    None,
                    None,
                );
            } else {
                self.cw_levels = None;
            }
            self.cw_session_end = Some(completed_day_end);
            self.cw_week_start = Some(week_start);
        }

        self.pw_levels = histogram::compute(
            &pw,
            "PW",
            pw_start,
            pw_end,
            None,
            "neutral",
            None,
            None,
        );
        if let Some(levels) = self.pw_levels.as_mut() {
            // Preserve the Sunday 18:00 NY start timestamp in `start`, and
            // expose the boundary candle's open separately as a price level.
            levels.sunday_open = self
                .candles
                .iter()
                .find(|c| c.time == pw_start && c.open.is_finite())
                .map(|c| c.open);
        }
        // ---- Previous session (rolls at every session close) ----
        let (ps_start, ps_end, ps_candles) =
            session::previous_session_slice(&self.candles, now_ms);
        self.ps_levels = histogram::compute(
            &ps_candles,
            "PS",
            ps_start,
            ps_end,
            None,
            "neutral",
            None,
            None,
        );
        self.ps_session_end = Some(ps_end);
        self.last_close_seen = Some(completed_day_end);

        self.swing_levels = swing::compute_swing(&self.candles);
    }

    /// Recompute only when the session boundary has moved past the close
    /// already seen. Returns true when a close actually rolled over, so the
    /// caller can broadcast fresh levels. Cheap enough to call on a timer.
    pub fn refresh_on_session_close(&mut self, now_ms: i64) -> bool {
        let current_end = session::last_session_close_utc(now_ms);
        if self.last_close_seen == Some(current_end) {
            return false;
        }
        self.recompute(now_ms);
        true
    }

    /// The session PS is describing right now (end timestamp, ms).
    pub fn ps_session_end(&self) -> Option<i64> {
        self.ps_session_end
    }

    pub fn candle_count(&self) -> usize {
        self.candles.len()
    }

    /// Most recent 18:00 America/New_York boundary at or before `now_ms`.
    pub fn last_session_close_utc(now_ms: i64) -> i64 {
        session::last_session_close_utc(now_ms)
    }

    pub fn most_recent_week_start_utc(now_ms: i64) -> i64 {
        weekly::most_recent_week_start_utc(now_ms)
    }

    /// Bounds of the previous trading week in UTC millis:
    /// `(Sunday 18:00 NY open, Friday 18:00 NY close)` of the week before the
    /// current one.
    pub fn previous_week_bounds_utc(now_ms: i64) -> (i64, i64) {
        weekly::previous_week_bounds_utc(now_ms)
    }

    /// Emitted to the broadcast bus, replayed on WS subscribe, and used by
    /// order flow for bubble detection.
    pub fn all_levels(&self) -> Vec<VpLevels> {
        let mut out = Vec::new();
        if let Some(pw) = &self.pw_levels {
            out.push(pw.clone());
        }
        if let Some(ps) = &self.ps_levels {
            out.push(ps.clone());
        }
        if let Some(cw) = &self.cw_levels {
            out.push(cw.clone());
        }
        if let Some(swing) = &self.swing_levels {
            out.push(swing.clone());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::America::New_York;

    const MIN: i64 = 60_000;
    const H: i64 = 60 * MIN;

    fn ny(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> i64 {
        New_York
            .with_ymd_and_hms(y, m, d, hh, mm, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
            .timestamp_millis()
    }

    fn candle(time: i64, low: f64, high: f64, volume: f64) -> VpCandle {
        VpCandle {
            time,
            open: low,
            high,
            low,
            close: high,
            volume,
            source: "test".into(),
        }
    }

    /// Fill a window with 15m candles in a given price band.
    fn fill(engine: &mut VolumeProfileEngine, from: i64, to: i64, low: f64, high: f64) {
        let mut t = from;
        while t < to {
            engine.candles.push(candle(t, low, high, 100.0));
            t += 15 * MIN;
        }
        engine.candles.sort_by_key(|c| c.time);
        engine.candles.dedup_by_key(|c| c.time);
    }

    /// The actual bug report: PS must change when a session closes.
    #[test]
    fn ps_rolls_forward_at_every_session_close() {
        let mut e = VolumeProfileEngine::new();

        // Session A: Mon 18:00 -> Tue 18:00, price band 3300-3310.
        fill(&mut e, ny(2025, 6, 9, 18, 0), ny(2025, 6, 10, 18, 0), 3300.0, 3310.0);
        // Session B: Tue 18:00 -> Wed 18:00, price band 3400-3410.
        fill(&mut e, ny(2025, 6, 10, 18, 0), ny(2025, 6, 11, 18, 0), 3400.0, 3410.0);
        // Session C (in progress): Wed 18:00 -> now, band 3500-3510.
        fill(&mut e, ny(2025, 6, 11, 18, 0), ny(2025, 6, 12, 10, 0), 3500.0, 3510.0);

        // At Tue 20:00 the last closed session is A.
        e.recompute(ny(2025, 6, 10, 20, 0));
        let ps_a = e.ps_levels.clone().expect("PS for session A");
        assert_eq!(ps_a.start, ny(2025, 6, 9, 18, 0));
        assert_eq!(ps_a.end, ny(2025, 6, 10, 18, 0));
        assert!((3300.0..=3310.0).contains(&ps_a.poc), "poc {} not in A", ps_a.poc);

        // Nothing has closed yet at Wed 10:00 -> PS must NOT move.
        assert!(!e.refresh_on_session_close(ny(2025, 6, 11, 10, 0)));
        assert_eq!(e.ps_levels.clone().unwrap().poc, ps_a.poc);

        // Wed 18:00 close passes -> PS rolls to session B.
        assert!(e.refresh_on_session_close(ny(2025, 6, 11, 18, 1)));
        let ps_b = e.ps_levels.clone().expect("PS for session B");
        assert_eq!(ps_b.start, ny(2025, 6, 10, 18, 0));
        assert_eq!(ps_b.end, ny(2025, 6, 11, 18, 0));
        assert!((3400.0..=3410.0).contains(&ps_b.poc), "poc {} not in B", ps_b.poc);
        assert_ne!(ps_a.poc, ps_b.poc, "PS was frozen across a session close");

        // Thu 18:00 close passes -> PS rolls to session C.
        assert!(e.refresh_on_session_close(ny(2025, 6, 12, 18, 1)));
        let ps_c = e.ps_levels.clone().expect("PS for session C");
        assert_eq!(ps_c.start, ny(2025, 6, 11, 18, 0));
        assert!((3500.0..=3510.0).contains(&ps_c.poc), "poc {} not in C", ps_c.poc);
    }

    #[test]
    fn ps_skips_the_weekend_gap() {
        let mut e = VolumeProfileEngine::new();
        // Friday session: Thu 18:00 -> Fri 18:00 (market closes Fri 18:00 NY).
        fill(&mut e, ny(2025, 6, 12, 18, 0), ny(2025, 6, 13, 18, 0), 3350.0, 3360.0);

        // Saturday noon: the "last closed session" window (Fri 18:00 ->
        // Sat 18:00) has no data, so PS must fall back to the Friday session
        // rather than disappear.
        e.recompute(ny(2025, 6, 14, 12, 0));
        let ps = e.ps_levels.clone().expect("PS over the weekend");
        assert_eq!(ps.start, ny(2025, 6, 12, 18, 0));
        assert_eq!(ps.end, ny(2025, 6, 13, 18, 0));
        assert!((3350.0..=3360.0).contains(&ps.poc));
    }

    #[test]
    fn refresh_fires_once_per_close_and_ignores_empty_weekend_sessions() {
        let mut e = VolumeProfileEngine::new();
        fill(&mut e, ny(2025, 6, 12, 18, 0), ny(2025, 6, 13, 18, 0), 3350.0, 3360.0);
        // Saturday noon: PS is the Friday session.
        e.recompute(ny(2025, 6, 14, 12, 0));
        assert_eq!(
            e.ps_levels.clone().expect("PS over the weekend").end,
            ny(2025, 6, 13, 18, 0)
        );

        // Saturday 18:00 closes an empty session: exactly one refresh, and PS
        // still describes Friday rather than vanishing.
        assert!(e.refresh_on_session_close(ny(2025, 6, 14, 18, 1)));
        assert_eq!(
            e.ps_levels.clone().expect("PS after empty close").end,
            ny(2025, 6, 13, 18, 0)
        );
        // ...and it must not fire again until the next boundary moves.
        assert!(!e.refresh_on_session_close(ny(2025, 6, 14, 18, 31)));
        assert!(!e.refresh_on_session_close(ny(2025, 6, 15, 12, 0)));
    }

    #[test]
    fn cw_appears_once_monday_closes_and_freezes_intraday() {
        let mut e = VolumeProfileEngine::new();
        // Monday session: Sun 18:00 -> Mon 18:00.
        fill(&mut e, ny(2025, 6, 8, 18, 0), ny(2025, 6, 9, 18, 0), 3300.0, 3310.0);
        // Tuesday session, still in progress.
        fill(&mut e, ny(2025, 6, 9, 18, 0), ny(2025, 6, 10, 12, 0), 3400.0, 3410.0);

        // Monday 12:00: nothing has closed this week yet -> no CW.
        e.recompute(ny(2025, 6, 9, 12, 0));
        assert!(
            e.cw_levels.is_none(),
            "CW must not exist before Monday's close: {:?}",
            e.cw_levels
        );

        // Monday 18:01: Monday closed -> CW covers exactly Monday's session.
        e.recompute(ny(2025, 6, 9, 18, 1));
        let cw = e.cw_levels.clone().expect("CW after Monday close");
        assert_eq!(cw.start, ny(2025, 6, 8, 18, 0));
        assert_eq!(cw.end, ny(2025, 6, 9, 18, 0));
        assert!(
            (3300.0..=3310.0).contains(&cw.poc),
            "poc {} not in Monday band",
            cw.poc
        );

        // Tuesday 12:00: Tuesday still open -> CW frozen on Monday's snapshot.
        e.recompute(ny(2025, 6, 10, 12, 0));
        let frozen = e.cw_levels.clone().expect("CW stays through Tuesday");
        assert_eq!((frozen.start, frozen.end, frozen.poc), (cw.start, cw.end, cw.poc));
    }

    #[test]
    fn ingesting_a_candle_after_a_close_refreshes_ps() {
        let mut e = VolumeProfileEngine::new();
        let now = Utc::now().timestamp_millis();
        let close = VolumeProfileEngine::last_session_close_utc(now);
        // One candle inside the last closed session.
        e.ingest_candle(candle(close - 2 * H, 3200.0, 3205.0, 50.0));
        let ps = e.ps_levels.clone().expect("PS after ingest");
        assert_eq!(ps.end, close);
        assert_eq!(e.ps_session_end(), Some(close));
    }

    #[test]
    fn week_windows_do_not_overlap_the_session_window() {
        let mut e = VolumeProfileEngine::new();
        let now = ny(2025, 6, 11, 12, 0);
        let week_start = VolumeProfileEngine::most_recent_week_start_utc(now);
        fill(&mut e, week_start - 6 * 24 * H, week_start - 24 * H, 3100.0, 3110.0);
        fill(&mut e, week_start + H, now, 3200.0, 3210.0);
        e.recompute(now);
        let pw = e.pw_levels.clone().expect("PW");
        let cw = e.cw_levels.clone().expect("CW");
        // PW ends at the previous Friday 18:00 NY close, not at the Sunday open.
        assert_eq!(pw.start, ny(2025, 6, 1, 18, 0));
        assert_eq!(pw.end, ny(2025, 6, 6, 18, 0));
        assert_eq!(cw.start, week_start);
        assert!((3100.0..=3110.0).contains(&pw.poc));
        assert!((3200.0..=3210.0).contains(&cw.poc));
    }

    #[test]
    fn pw_excludes_weekend_candles_and_is_stable_all_week() {
        let mut e = VolumeProfileEngine::new();
        let (start, end) = VolumeProfileEngine::previous_week_bounds_utc(ny(2025, 6, 11, 12, 0));
        assert_eq!(start, ny(2025, 6, 1, 18, 0));
        assert_eq!(end, ny(2025, 6, 6, 18, 0));
        fill(&mut e, start, end - H, 3100.0, 3110.0);
        // Anything after the Friday close (weekend / Sunday pre-open) must not leak in.
        fill(&mut e, end, end + 40 * H, 3500.0, 3510.0);
        for day in 9..=13 {
            e.recompute(ny(2025, 6, day, 12, 0));
            let pw = e.pw_levels.clone().expect("PW");
            assert_eq!((pw.start, pw.end), (start, end));
            assert!((3100.0..=3110.0).contains(&pw.poc), "{pw:?}");
        }
    }

    #[test]
    fn pw_marks_the_sunday_start_candle_open_price() {
        let mut e = VolumeProfileEngine::new();
        let now = ny(2025, 6, 11, 12, 0);
        let (start, end) = VolumeProfileEngine::previous_week_bounds_utc(now);
        fill(&mut e, start, end, 3100.0, 3110.0);
        let sunday_open = 3107.25;
        e.candles
            .iter_mut()
            .find(|c| c.time == start)
            .expect("exact Sunday 18:00 candle")
            .open = sunday_open;

        e.recompute(now);
        let pw = e.pw_levels.as_ref().expect("PW profile");
        assert_eq!(pw.start, start, "start remains the Sunday-open timestamp");
        assert_eq!(pw.sunday_open, Some(sunday_open));

        let wire = serde_json::to_value(pw).expect("serialize PW levels");
        assert_eq!(wire["window"], "PW");
        assert_eq!(wire["sunday_open"], sunday_open);
    }

    #[test]
    fn duplicate_candle_timestamp_is_replaced() {
        let mut e = VolumeProfileEngine::new();
        let time = Utc::now().timestamp_millis() - H;
        e.ingest_candle(candle(time, 100.0, 101.0, 10.0));
        e.ingest_candle(candle(time, 200.0, 201.0, 10.0));
        assert_eq!(e.candle_count(), 1);
        assert!(e.swing_levels.is_none());
    }
}
