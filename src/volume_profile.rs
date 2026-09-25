use chrono::{Datelike, Duration, TimeZone, Utc};
use chrono_tz::America::New_York;

use crate::types::{VpCandle, VpLevels};

const BIN_SIZE: f64 = 0.50; // $0.50 bins for gold
const VA_PCT: f64 = 0.70; // 70% value area
const SWING_PIVOT_RADIUS: usize = 8;
const FIFTEEN_MIN_MS: i64 = 15 * 60 * 1000;

/// Hour (America/New_York) at which the FX trading session rolls over.
/// 17:00 NY is the daily close/open used by every retail gold feed.
const SESSION_CLOSE_HOUR: u32 = 17;

/// How many sessions back we are willing to look for a non-empty previous
/// session. Covers the weekend hole (Fri 17:00 → Sun 18:00) plus holidays.
const MAX_SESSION_LOOKBACK: i64 = 5;

/// Keep the complete fixed 2,000-candle Sifting seed (about 21 days of 15m
/// bars) plus room for live closes and session/week boundaries.
const RETAIN_MS: i64 = 35 * 24 * 60 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PivotKind {
    High,
    Low,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwingDirection {
    Bullish,
    Bearish,
}

impl SwingDirection {
    fn label(self) -> &'static str {
        match self {
            Self::Bullish => "SWING_BULL",
            Self::Bearish => "SWING_BEAR",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Bullish => "bullish",
            Self::Bearish => "bearish",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Pivot {
    index: usize,
    kind: PivotKind,
}

#[derive(Clone, Copy, Debug)]
struct SwingLeg {
    start_index: usize,
    end_index: usize,
    direction: SwingDirection,
    high: f64,
    low: f64,
}

pub struct VolumeProfileEngine {
    pub pw_levels: Option<VpLevels>,
    pub ps_levels: Option<VpLevels>,
    pub cw_levels: Option<VpLevels>,
    /// The most recent completed pivot-to-pivot swing profile. Its label is
    /// `SWING_BULL` or `SWING_BEAR`, so consumers can target the correct leg
    /// without guessing from a time-window profile.
    pub swing_levels: Option<VpLevels>,
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
            swing_levels: None,
            ps_session_end: None,
            candles: Vec::new(),
        }
    }

    /// Build a price histogram. For swing profiles `range` is the exact best
    /// low/high of the directional leg; session/week profiles derive their
    /// range from the candles in that window.
    fn compute(
        candles: &[VpCandle],
        label: &str,
        start: i64,
        end: i64,
        range: Option<(f64, f64)>,
        direction: &str,
        swing_high: Option<f64>,
        swing_low: Option<f64>,
    ) -> Option<VpLevels> {
        if candles.is_empty() {
            return None;
        }

        let valid: Vec<&VpCandle> = candles
            .iter()
            .filter(|c| {
                c.time > 0
                    && c.open.is_finite()
                    && c.high.is_finite()
                    && c.low.is_finite()
                    && c.close.is_finite()
                    && c.volume.is_finite()
                    && c.volume > 0.0
                    && c.high >= c.low
            })
            .collect();
        if valid.is_empty() {
            return None;
        }

        let (lo, hi) = range.unwrap_or_else(|| {
            let lo = valid
                .iter()
                .map(|c| c.low)
                .fold(f64::INFINITY, f64::min);
            let hi = valid
                .iter()
                .map(|c| c.high)
                .fold(f64::NEG_INFINITY, f64::max);
            (lo, hi)
        });
        if !lo.is_finite() || !hi.is_finite() || hi <= lo {
            return None;
        }

        let n_bins = (((hi - lo) / BIN_SIZE).ceil() as usize).max(1);
        let mut bins = vec![0.0_f64; n_bins];

        for c in valid {
            let clipped_low = c.low.max(lo);
            let clipped_high = c.high.min(hi);
            if clipped_high < clipped_low {
                continue;
            }

            let low_idx = Self::bin_index(clipped_low, lo, n_bins);
            let high_idx = Self::bin_index(clipped_high, lo, n_bins);
            let candle_range = c.high - c.low;

            if candle_range <= f64::EPSILON {
                bins[low_idx] += c.volume;
                continue;
            }

            // Allocate volume by the actual overlap with each price row. The
            // previous implementation split it equally among every row in a
            // candle's range, which shifts POC/VA when ranges are uneven.
            for i in low_idx..=high_idx {
                let bin_low = lo + i as f64 * BIN_SIZE;
                let bin_high = bin_low + BIN_SIZE;
                let overlap_low = c.low.max(bin_low);
                let overlap_high = c.high.min(bin_high);
                let overlap = (overlap_high - overlap_low).max(0.0);
                if overlap > 0.0 {
                    bins[i] += c.volume * overlap / candle_range;
                }
            }
        }

        let total: f64 = bins.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }

        let poc_idx = bins
            .iter()
            .enumerate()
            .max_by(|a, b| {
                a.1.partial_cmp(b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)?;
        let poc = lo + (poc_idx as f64 + 0.5) * BIN_SIZE;

        // Standard market-profile value area expansion: start at POC and add
        // the larger adjacent row until 70% of total volume is included. On a
        // tie, prefer the upper row for deterministic CME-style expansion.
        let target = total * VA_PCT;
        let mut va_sum = bins[poc_idx];
        let mut lo_idx = poc_idx;
        let mut hi_idx = poc_idx;

        while va_sum < target && (lo_idx > 0 || hi_idx < n_bins - 1) {
            let lo_vol = if lo_idx > 0 {
                bins[lo_idx - 1]
            } else {
                f64::NEG_INFINITY
            };
            let hi_vol = if hi_idx < n_bins - 1 {
                bins[hi_idx + 1]
            } else {
                f64::NEG_INFINITY
            };
            if hi_vol >= lo_vol {
                hi_idx += 1;
                va_sum += bins[hi_idx];
            } else {
                lo_idx -= 1;
                va_sum += bins[lo_idx];
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
            direction: direction.into(),
            swing_high,
            swing_low,
        })
    }

    fn bin_index(price: f64, lo: f64, n_bins: usize) -> usize {
        (((price - lo) / BIN_SIZE).floor() as isize)
            .clamp(0, n_bins as isize - 1) as usize
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
    /// PW  = previous trading week   (Sun 18:00 NY open → Fri 17:00 NY close),
    ///       drawn/served unchanged for the whole of the current week.
    /// PS  = the last **closed** session (17:00 NY → 17:00 NY), so it rolls
    ///       forward at every session close instead of being computed once.
    /// CW  = current week so far.
    /// SWING = the most recent completed pivot-to-pivot directional leg.
    pub fn recompute(&mut self, now_ms: i64) {
        let week_start = Self::most_recent_week_start_utc(now_ms);
        let (pw_start, pw_end) = Self::previous_week_bounds_utc(now_ms);

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
        let cw: Vec<_> = self
            .candles
            .iter()
            .filter(|c| c.time >= week_start && c.time <= now_ms)
            .cloned()
            .collect();

        self.pw_levels = Self::compute(
            &pw,
            "PW",
            pw_start,
            pw_end,
            None,
            "neutral",
            None,
            None,
        );
        self.cw_levels = Self::compute(
            &cw,
            "CW",
            week_start,
            now_ms,
            None,
            "neutral",
            None,
            None,
        );

        // ---- Previous session (rolls at every session close) ----
        let (ps_start, ps_end, ps_candles) = self.previous_session_slice(now_ms);
        self.ps_levels = Self::compute(
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

        self.swing_levels = self.compute_swing();
    }

    fn compute_swing(&self) -> Option<VpLevels> {
        if self.candles.len() < 2 {
            return None;
        }
        let leg = Self::latest_swing(&self.candles)?;
        let start_index = leg.start_index.min(leg.end_index);
        let end_index = leg.start_index.max(leg.end_index);
        let candles = &self.candles[start_index..=end_index];
        let start = self.candles[start_index].time;
        let end = self.candles[end_index]
            .time
            .saturating_add(Self::candle_interval(candles));

        Self::compute(
            candles,
            leg.direction.label(),
            start,
            end,
            Some((leg.low, leg.high)),
            leg.direction.as_str(),
            Some(leg.high),
            Some(leg.low),
        )
    }

    fn candle_interval(candles: &[VpCandle]) -> i64 {
        candles
            .windows(2)
            .map(|pair| pair[1].time.saturating_sub(pair[0].time))
            .find(|delta| *delta > 0)
            .unwrap_or(FIFTEEN_MIN_MS)
    }

    /// Find the latest confirmed alternating pivot pair. A high followed by a
    /// low is a bearish leg (high → low); a low followed by a high is bullish
    /// (low → high). The profile's price bounds are the best high and best low
    /// inside that leg, not the arbitrary bounds of a calendar window.
    fn latest_swing(candles: &[VpCandle]) -> Option<SwingLeg> {
        let mut pivots = Vec::new();
        if candles.len() >= SWING_PIVOT_RADIUS * 2 + 1 {
            for index in SWING_PIVOT_RADIUS..candles.len() - SWING_PIVOT_RADIUS {
                let high = candles[index].high;
                let low = candles[index].low;
                let is_high = high.is_finite()
                    && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
                        .all(|j| j == index || high > candles[j].high);
                let is_low = low.is_finite()
                    && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
                        .all(|j| j == index || low < candles[j].low);

                if is_high {
                    Self::push_pivot(&mut pivots, PivotKind::High, index, candles);
                } else if is_low {
                    Self::push_pivot(&mut pivots, PivotKind::Low, index, candles);
                }
            }
        }

        let mut latest = None;
        for pair in pivots.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            let direction = match (first.kind, second.kind) {
                (PivotKind::Low, PivotKind::High) => SwingDirection::Bullish,
                (PivotKind::High, PivotKind::Low) => SwingDirection::Bearish,
                _ => continue,
            };
            let start_index = first.index;
            let end_index = second.index;
            let slice = &candles[start_index..=end_index];
            let high = slice
                .iter()
                .map(|c| c.high)
                .filter(|v| v.is_finite())
                .fold(f64::NEG_INFINITY, f64::max);
            let low = slice
                .iter()
                .map(|c| c.low)
                .filter(|v| v.is_finite())
                .fold(f64::INFINITY, f64::min);
            if high.is_finite() && low.is_finite() && high > low {
                latest = Some(SwingLeg {
                    start_index,
                    end_index,
                    direction,
                    high,
                    low,
                });
            }
        }

        if latest.is_some() {
            return latest;
        }

        // A short or monotonic history may not contain a confirmed pivot on
        // both sides. Still anchor a deterministic best-extreme leg rather
        // than silently falling back to a mixed source/time profile.
        let high_index = candles
            .iter()
            .enumerate()
            .max_by(|a, b| {
                a.1.high
                    .partial_cmp(&b.1.high)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)?;
        let low_index = candles
            .iter()
            .enumerate()
            .min_by(|a, b| {
                a.1.low
                    .partial_cmp(&b.1.low)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)?;
        if high_index == low_index {
            return None;
        }

        let (start_index, end_index, direction) = if low_index < high_index {
            (low_index, high_index, SwingDirection::Bullish)
        } else {
            (high_index, low_index, SwingDirection::Bearish)
        };
        let slice = &candles[start_index..=end_index];
        let high = slice
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let low = slice.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        if high > low {
            Some(SwingLeg {
                start_index,
                end_index,
                direction,
                high,
                low,
            })
        } else {
            None
        }
    }

    fn push_pivot(pivots: &mut Vec<Pivot>, kind: PivotKind, index: usize, candles: &[VpCandle]) {
        if let Some(previous) = pivots.last_mut() {
            if previous.kind == kind {
                let replace = match kind {
                    PivotKind::High => candles[index].high >= candles[previous.index].high,
                    PivotKind::Low => candles[index].low <= candles[previous.index].low,
                };
                if replace {
                    previous.index = index;
                }
                return;
            }
        }
        pivots.push(Pivot { index, kind });
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
        let now = Utc
            .timestamp_millis_opt(now_ms)
            .single()
            .unwrap_or_else(Utc::now);
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

    pub fn most_recent_week_start_utc(now_ms: i64) -> i64 {
        let now = Utc
            .timestamp_millis_opt(now_ms)
            .single()
            .unwrap_or_else(Utc::now);
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

    /// Bounds of the previous trading week in UTC millis:
    /// `(Sunday 18:00 NY open, Friday 17:00 NY close)` of the week before the
    /// current one. Both anchors are resolved in America/New_York local time so
    /// they stay on the FX open/close across DST changes.
    pub fn previous_week_bounds_utc(now_ms: i64) -> (i64, i64) {
        let week_start = Self::most_recent_week_start_utc(now_ms);
        let this_sunday = Utc
            .timestamp_millis_opt(week_start)
            .single()
            .unwrap_or_else(Utc::now)
            .with_timezone(&New_York);
        let prev_sunday = this_sunday - Duration::days(7);
        let prev_friday = this_sunday - Duration::days(2);
        let start = New_York
            .with_ymd_and_hms(prev_sunday.year(), prev_sunday.month(), prev_sunday.day(), 18, 0, 0)
            .single()
            .expect("valid Sunday 18:00 local time");
        let end = New_York
            .with_ymd_and_hms(prev_friday.year(), prev_friday.month(), prev_friday.day(), SESSION_CLOSE_HOUR, 0, 0)
            .single()
            .expect("valid Friday 17:00 local time");
        (
            start.with_timezone(&Utc).timestamp_millis(),
            end.with_timezone(&Utc).timestamp_millis(),
        )
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

    fn swing_history(prices: &[f64]) -> Vec<VpCandle> {
        let start = Utc::now().timestamp_millis() - prices.len() as i64 * 15 * MIN;
        prices
            .iter()
            .enumerate()
            .map(|(i, price)| {
                candle(
                    start + i as i64 * 15 * MIN,
                    price - 0.5,
                    price + 0.5,
                    100.0,
                )
            })
            .collect()
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
        // PW ends at the previous Friday 17:00 NY close, not at the Sunday open.
        assert_eq!(pw.start, ny(2025, 6, 1, 18, 0));
        assert_eq!(pw.end, ny(2025, 6, 6, 17, 0));
        assert_eq!(cw.start, week_start);
        assert!((3100.0..=3110.0).contains(&pw.poc));
        assert!((3200.0..=3210.0).contains(&cw.poc));
    }

    #[test]
    fn pw_excludes_weekend_candles_and_is_stable_all_week() {
        let mut e = VolumeProfileEngine::new();
        let (start, end) = VolumeProfileEngine::previous_week_bounds_utc(ny(2025, 6, 11, 12, 0));
        assert_eq!(start, ny(2025, 6, 1, 18, 0));
        assert_eq!(end, ny(2025, 6, 6, 17, 0));
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
    fn bearish_swing_is_anchored_high_to_low() {
        let prices = [
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 104.0, 103.0, 102.0, 101.0,
            100.0, 99.0, 98.0, 97.0, 96.0, 95.0, 96.0, 97.0, 98.0, 99.0, 100.0, 101.0,
            102.0, 103.0, 104.0,
        ];
        let mut e = VolumeProfileEngine::new();
        e.ingest_candles(swing_history(&prices));
        let swing = e.swing_levels.expect("bearish swing");
        assert_eq!(swing.window, "SWING_BEAR");
        assert_eq!(swing.direction, "bearish");
        assert_eq!(swing.swing_high, Some(105.5));
        assert_eq!(swing.swing_low, Some(94.5));
        assert!(swing.start < swing.end);
    }

    #[test]
    fn bullish_swing_is_anchored_low_to_high() {
        let prices = [
            105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0,
            106.0, 107.0, 108.0, 109.0, 110.0, 109.0, 108.0, 107.0, 106.0, 105.0, 104.0,
            103.0, 102.0, 101.0,
        ];
        let mut e = VolumeProfileEngine::new();
        e.ingest_candles(swing_history(&prices));
        let swing = e.swing_levels.expect("bullish swing");
        assert_eq!(swing.window, "SWING_BULL");
        assert_eq!(swing.direction, "bullish");
        assert_eq!(swing.swing_low, Some(99.5));
        assert_eq!(swing.swing_high, Some(110.5));
        assert!(swing.start < swing.end);
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
