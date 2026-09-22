use anyhow::Result;
use chrono::{Datelike, Duration, TimeZone, Utc};
use chrono_tz::America::New_York;

use crate::types::{VpCandle, VpLevels};

const BIN_SIZE: f64 = 0.50;      // $0.50 bins for gold
const VA_PCT: f64 = 0.70;        // 70% value area

pub struct VolumeProfileEngine {
    pub pw_levels: Option<VpLevels>,
    pub ps_levels: Option<VpLevels>,
    pub cw_levels: Option<VpLevels>,
    pub pw_candles: Vec<VpCandle>,
    pub ps_candles: Vec<VpCandle>,
    pub cw_candles: Vec<VpCandle>,
}

impl VolumeProfileEngine {
    pub fn new() -> Self {
        Self {
            pw_levels: None,
            ps_levels: None,
            cw_levels: None,
            pw_candles: Vec::new(),
            ps_candles: Vec::new(),
            cw_candles: Vec::new(),
        }
    }

    fn compute(candles: &[VpCandle], label: &str) -> Option<VpLevels> {
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
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    pub fn recompute_pw(&mut self) {
        self.pw_levels = Self::compute(&self.pw_candles, "PW");
    }
    pub fn recompute_ps(&mut self) {
        self.ps_levels = Self::compute(&self.ps_candles, "PS");
    }
    pub fn recompute_cw(&mut self) {
        self.cw_levels = Self::compute(&self.cw_candles, "CW");
    }

    pub fn ingest_candle(&mut self, candle: VpCandle) {
        let now_ms = Utc::now().timestamp_millis();
        let week_start = Self::most_recent_week_start_utc(now_ms);
        let ps_start = week_start - 49 * 60 * 60 * 1000;
        let pw_floor = week_start - 7 * 24 * 60 * 60 * 1000;

        if candle.time >= week_start {
            self.cw_candles.push(candle);
            self.cw_candles.sort_by_key(|c| c.time);
            self.cw_candles.dedup_by_key(|c| c.time);
            self.recompute_cw();
        } else if candle.time >= ps_start {
            self.ps_candles.push(candle);
            self.ps_candles.sort_by_key(|c| c.time);
            self.ps_candles.dedup_by_key(|c| c.time);
            self.recompute_ps();
        } else if candle.time >= pw_floor {
            self.pw_candles.push(candle);
            self.pw_candles.sort_by_key(|c| c.time);
            self.pw_candles.dedup_by_key(|c| c.time);
            self.recompute_pw();
        }
    }

    /// Emitted to the broadcast bus, replayed on WS subscribe, and used by
    /// order flow for bubble detection. Includes PW (PoC/VaH/VaL), PS (PoC),
    /// and CW (PoC, plus VaH/VaL populated for consumers that want them).
    pub fn all_levels(&self) -> Vec<VpLevels> {
        let mut out = Vec::new();
        if let Some(pw) = &self.pw_levels { out.push(pw.clone()); }
        if let Some(ps) = &self.ps_levels { out.push(ps.clone()); }
        if let Some(cw) = &self.cw_levels { out.push(cw.clone()); }
        out
    }

    /// Alias kept for the REST /levels endpoint in ws_server.rs.
    pub fn all_levels_full(&self) -> Vec<VpLevels> {
        self.all_levels()
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

    pub fn previous_friday_close_utc(now_ms: i64) -> i64 {
        let week_start = Self::most_recent_week_start_utc(now_ms);
        week_start - 49 * 60 * 60 * 1000
    }
}