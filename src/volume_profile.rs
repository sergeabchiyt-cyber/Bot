use chrono::{Datelike, Duration, TimeZone, Utc};
use chrono_tz::America::New_York;

use crate::types::{VpCandle, VpLevels};

const BIN_SIZE: f64 = 0.50;      // $0.50 bins for gold
const VA_PCT: f64 = 0.70;        // 70% value area

/// Hour (America/New_York) at which the FX trading session rolls over.
/// 17:00 NY is the daily close/open used by every retail gold feed.
const SESSION_CLOSE_HOUR: u32 = 17;

/// How many sessions back we are willing to look for a non-empty previous
/// session. Covers the weekend hole (Fri 17:00 → Sun 18:00) plus holidays.
const MAX_SESSION_LOOKBACK: i64 = 5;

/// How much history we keep in memory: previous week + current week + slack.
const RETAIN_MS: i64 = 16 * 24 * 60 * 60 * 1000;

pub struct VolumeProfileEngine {
    pub pw_levels: Option<VpLevels>,
    pub ps_levels: Option<VpLevels>,
    pub cw_levels: Option<VpLevels>,
    /// End timestamp (ms) of the session PS currently describes. Used to
    /// detect a session rollover so PS is recomputed on every session close
    /// instead of being frozen at whatever it was at boot.
    ps_session_end: Option<i64>,
    candles: Vec<VpCandle>,
}

impl VolumeProfileEngine {
    pub fn new() -> Self {
        Self {
            pw_levels: None,
            ps_levels: None,
            cw_levels: None,
            ps_session_end: None,
            candles: Vec::new(),
        }
    }

    fn compute(candles: &[VpCandle], label: &str, start: i64, end: i64) -> Option<VpLevels> {
        if candles.is_empty() {
            return None;
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for c in candles {
            if c.low < lo { lo = c.low; }
            if c.high > hi { hi = c.high; }
        }
        if !lo.is_finite() || !hi.is_finite() || hi <= lo {
            return None;
        }

        let n_bins = (((hi - lo) / BIN_SIZE).ceil() as usize).max(1);
        let mut bins = vec![0.0_f64; n_bins];

        for c in candles {
            let i_lo = (((c.low - lo) / BIN_SIZE).floor() as isize).max(0) as usize;
            let i_hi = (((c.high - lo) / BIN_SIZE).floor() as isize)
                .min(n_bins as isize - 1)
                .max(0) as usize;
            let span = (i_hi - i_lo + 1) as f64;
            let per_bin = c.volume / span;
            for i in i_lo..=i_hi {
                bins[i] += per_bin;
            }
        }

        let poc_idx = bins
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)?;
        let poc = lo + (poc_idx as f64 + 0.5) * BIN_SIZE;

        let total: f64 = bins.iter().sum();
        let target = total * VA_PCT;
        let mut va_sum = bins[poc_idx];
        let mut lo_idx = poc_idx;
        let mut hi_idx = poc_idx;

        while va_sum < target && (lo_idx > 0 || hi_idx < n_bins - 1) {
            let lo_vol = if lo_idx > 0 { bins[lo_idx - 1] } else { f64::NEG_INFINITY };
            let hi_vol = if hi_idx < n_bins - 1 { bins[hi_idx + 1] } else { f64::NEG_INFINITY };
            if lo_vol >= hi_vol {
                lo_idx -= 1;
                va_sum += bins[lo_idx];
            } else {
                hi_idx += 1;
                va_sum += bins[hi_idx];
            }
        }

        let val = lo + lo_idx as f64 * BIN_SIZE;
        let vah = lo + (hi_idx as f64 + 1.0) * BIN_SIZE;

        Some(VpLevels {
            window: label.into(),
            poc,
            vah,
            val,
            start,
            end,
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    pub fn ingest_candle(&mut self, candle: VpCandle) {
        self.candles.push(candle);
        self.candles.sort_by_key(|c| c.time);
        self.candles.dedup_by_key(|c| c.time);
        self.recompute(Utc::now().timestamp_millis());
    }

    /// Recompute every window against `now_ms`.
    ///
    /// PW  = previous trading week   (Sun 18:00 NY → Sun 18:00 NY)
    /// PS  = the last **closed** session (17:00 NY → 17:00 NY), so it rolls
    ///       forward at every session close instead of being computed once.
    /// CW  = current week so far.
    pub fn recompute(&mut self, now_ms: i64) {
        let week_start = Self::most_recent_week_start_utc(now_ms);
        let pw_start = week_start - 7 * 24 * 60 * 60 * 1000;

        let retain_floor = pw_start.min(now_ms - RETAIN_MS);
        self.candles.retain(|c| c.time >= retain_floor);

        let pw: Vec<_> = self
            .candles
            .iter()
            .filter(|c| c.time >= pw_start && c.time < week_start)
            .cloned()
            .collect();
        let cw: Vec<_> = self.candles.iter().filter(|c| c.time >= week_start).cloned().collect();

        self.pw_levels = Self::compute(&pw, "PW", pw_start, week_start);
        self.cw_levels = Self::compute(&cw, "CW", week_start, now_ms);

        // ---- Previous session (rolls at every session close) ----
        let (ps_start, ps_end, ps_candles) = self.previous_session_slice(now_ms);
        self.ps_levels = Self::compute(&ps_candles, "PS", ps_start, ps_end);
        self.ps_session_end = Some(ps_end);
    }

    /// Recompute only when the session boundary has moved past the session PS
    /// currently represents. Returns true when PS actually rolled over, so the
    /// caller can broadcast fresh levels. Cheap enough to call on a timer.
    pub fn refresh_on_session_close(&mut self, now_ms: i64) -> bool {
        let current_end = Self::last_session_close_utc(now_ms);
        if self.ps_session_end == Some(current_end) {
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

    /// Walks back from the most recent session close until it finds a session
    /// that actually contains candles (skips the weekend / holiday gap).
    fn previous_session_slice(&self, now_ms: i64) -> (i64, i64, Vec<VpCandle>) {
        let mut end = Self::last_session_close_utc(now_ms);
        let first_end = end;
        let mut first_start = Self::session_close_shift(end, -1);

        for _ in 0..MAX_SESSION_LOOKBACK {
            let start = Self::session_close_shift(end, -1);
            let slice: Vec<VpCandle> = self
                .candles
                .iter()
                .filter(|c| c.time >= start && c.time < end)
                .cloned()
                .collect();
            if !slice.is_empty() {
                return (start, end, slice);
            }
            if end == first_end {
                first_start = start;
            }
            end = start;
        }
        (first_start, first_end, Vec::new())
    }

    /// Most recent 17:00 America/New_York boundary at or before `now_ms`.
    /// DST-safe: the wall-clock hour is re-anchored in the local zone.
    pub fn last_session_close_utc(now_ms: i64) -> i64 {
        let now = Utc.timestamp_millis_opt(now_ms).single().unwrap_or_else(Utc::now);
        let local = now.with_timezone(&New_York);
        let mut close = Self::ny_close_on(local.year(), local.month(), local.day());
        if close > now_ms {
            let prev = local - Duration::days(1);
            close = Self::ny_close_on(prev.year(), prev.month(), prev.day());
        }
        close
    }

    /// Shift a session-close timestamp by `days` whole calendar days, keeping
    /// the boundary pinned to 17:00 local time across DST changes.
    fn session_close_shift(close_ms: i64, days: i64) -> i64 {
        let ts = Utc
            .timestamp_millis_opt(close_ms)
            .single()
            .unwrap_or_else(Utc::now)
            .with_timezone(&New_York);
        let shifted = ts + Duration::days(days);
        Self::ny_close_on(shifted.year(), shifted.month(), shifted.day())
    }

    fn ny_close_on(year: i32, month: u32, day: u32) -> i64 {
        New_York
            .with_ymd_and_hms(year, month, day, SESSION_CLOSE_HOUR, 0, 0)
            .single()
            // 17:00 never falls in a DST gap in New York, but stay total.
            .unwrap_or_else(|| {
                New_York
                    .with_ymd_and_hms(year, month, day, SESSION_CLOSE_HOUR + 1, 0, 0)
                    .earliest()
                    .expect("valid NY session close")
            })
            .with_timezone(&Utc)
            .timestamp_millis()
    }

    /// Emitted to the broadcast bus, replayed on WS subscribe, and used by
    /// order flow for bubble detection.
    pub fn all_levels(&self) -> Vec<VpLevels> {
        let mut out = Vec::new();
        if let Some(pw) = &self.pw_levels { out.push(pw.clone()); }
        if let Some(ps) = &self.ps_levels { out.push(ps.clone()); }
        if let Some(cw) = &self.cw_levels { out.push(cw.clone()); }
        out
    }

    pub fn most_recent_week_start_utc(now_ms: i64) -> i64 {
        let now = Utc.timestamp_millis_opt(now_ms).single().unwrap_or_else(Utc::now);
        let local = now.with_timezone(&New_York);
        let days_since_sunday = local.weekday().num_days_from_sunday() as i64;
        let mut sunday = local - Duration::days(days_since_sunday);
        sunday = New_York
            .with_ymd_and_hms(sunday.year(), sunday.month(), sunday.day(), 18, 0, 0)
            .single()
            .expect("valid Sunday 18:00 local time");
        if sunday.with_timezone(&Utc) > now {
            sunday = sunday - Duration::days(7);
        }
        sunday.with_timezone(&Utc).timestamp_millis()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn session_close_is_1700_new_york() {
        // Wed 2025-06-11 20:00 NY -> close is that same day 17:00 NY.
        let now = ny(2025, 6, 11, 20, 0);
        assert_eq!(
            VolumeProfileEngine::last_session_close_utc(now),
            ny(2025, 6, 11, 17, 0)
        );
        // 16:59 NY is still the *previous* day's session.
        let now = ny(2025, 6, 11, 16, 59);
        assert_eq!(
            VolumeProfileEngine::last_session_close_utc(now),
            ny(2025, 6, 10, 17, 0)
        );
    }

    #[test]
    fn session_boundary_survives_dst_change() {
        // US DST ends 2025-11-02. 17:00 local on either side must stay 17:00.
        let before = VolumeProfileEngine::last_session_close_utc(ny(2025, 10, 31, 23, 0));
        let after = VolumeProfileEngine::last_session_close_utc(ny(2025, 11, 3, 23, 0));
        assert_eq!(before, ny(2025, 10, 31, 17, 0));
        assert_eq!(after, ny(2025, 11, 3, 17, 0));
    }

    /// The actual bug report: PS must change when a session closes.
    #[test]
    fn ps_rolls_forward_at_every_session_close() {
        let mut e = VolumeProfileEngine::new();

        // Session A: Mon 17:00 -> Tue 17:00, price band 3300-3310.
        fill(&mut e, ny(2025, 6, 9, 17, 0), ny(2025, 6, 10, 17, 0), 3300.0, 3310.0);
        // Session B: Tue 17:00 -> Wed 17:00, price band 3400-3410.
        fill(&mut e, ny(2025, 6, 10, 17, 0), ny(2025, 6, 11, 17, 0), 3400.0, 3410.0);
        // Session C (in progress): Wed 17:00 -> now, band 3500-3510.
        fill(&mut e, ny(2025, 6, 11, 17, 0), ny(2025, 6, 12, 10, 0), 3500.0, 3510.0);

        // At Tue 20:00 the last closed session is A.
        e.recompute(ny(2025, 6, 10, 20, 0));
        let ps_a = e.ps_levels.clone().expect("PS for session A");
        assert_eq!(ps_a.start, ny(2025, 6, 9, 17, 0));
        assert_eq!(ps_a.end, ny(2025, 6, 10, 17, 0));
        assert!((3300.0..=3310.0).contains(&ps_a.poc), "poc {} not in A", ps_a.poc);

        // Nothing has closed yet at Wed 10:00 -> PS must NOT move.
        assert!(!e.refresh_on_session_close(ny(2025, 6, 11, 10, 0)));
        assert_eq!(e.ps_levels.clone().unwrap().poc, ps_a.poc);

        // Wed 17:00 close passes -> PS rolls to session B.
        assert!(e.refresh_on_session_close(ny(2025, 6, 11, 17, 1)));
        let ps_b = e.ps_levels.clone().expect("PS for session B");
        assert_eq!(ps_b.start, ny(2025, 6, 10, 17, 0));
        assert_eq!(ps_b.end, ny(2025, 6, 11, 17, 0));
        assert!((3400.0..=3410.0).contains(&ps_b.poc), "poc {} not in B", ps_b.poc);
        assert_ne!(ps_a.poc, ps_b.poc, "PS was frozen across a session close");

        // Thu 17:00 close passes -> PS rolls to session C.
        assert!(e.refresh_on_session_close(ny(2025, 6, 12, 17, 1)));
        let ps_c = e.ps_levels.clone().expect("PS for session C");
        assert_eq!(ps_c.start, ny(2025, 6, 11, 17, 0));
        assert!((3500.0..=3510.0).contains(&ps_c.poc), "poc {} not in C", ps_c.poc);
    }

    #[test]
    fn ps_skips_the_weekend_gap() {
        let mut e = VolumeProfileEngine::new();
        // Friday session: Thu 17:00 -> Fri 17:00 (market closes Fri 17:00 NY).
        fill(&mut e, ny(2025, 6, 12, 17, 0), ny(2025, 6, 13, 17, 0), 3350.0, 3360.0);

        // Saturday noon: the "last closed session" window (Fri 17:00 ->
        // Sat 17:00) has no data, so PS must fall back to the Friday session
        // rather than disappear.
        e.recompute(ny(2025, 6, 14, 12, 0));
        let ps = e.ps_levels.clone().expect("PS over the weekend");
        assert_eq!(ps.start, ny(2025, 6, 12, 17, 0));
        assert_eq!(ps.end, ny(2025, 6, 13, 17, 0));
        assert!((3350.0..=3360.0).contains(&ps.poc));
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
        assert_eq!(pw.end, week_start);
        assert_eq!(cw.start, week_start);
        assert!((3100.0..=3110.0).contains(&pw.poc));
        assert!((3200.0..=3210.0).contains(&cw.poc));
    }
}
