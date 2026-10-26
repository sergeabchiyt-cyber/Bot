//! In-memory store of closed Node 1 bars, one ordered series per resolution.
//!
//! Only closed bars are stored. Open bars update freshness state but never
//! reach the profile maths. Series are keyed by bar open time, so replays and
//! re-sent bars overwrite instead of duplicating.

use std::collections::BTreeMap;

use crate::types::{SeriesStat, VpCandle};

/// Resolutions accepted from Node 1, finest first.
pub const INTERVALS: [&str; 3] = ["1m", "15m", "1h"];
const INTERVAL_MS: [i64; 3] = [60_000, 900_000, 3_600_000];
/// A window is profiled from a resolution only when the bars present cover at
/// least this share of the expected count. Weekend and closed-market buckets
/// count as present because Node 1 emits them as bodyless filler bars.
const COVERAGE_MIN_PERCENT: i64 = 90;

pub fn interval_index(label: &str) -> Option<usize> {
    INTERVALS.iter().position(|known| *known == label)
}

pub fn interval_ms(label: &str) -> Option<i64> {
    interval_index(label).map(|index| INTERVAL_MS[index])
}

#[derive(Debug, Default)]
pub struct CandleStore {
    series: [BTreeMap<i64, VpCandle>; 3],
}

impl CandleStore {
    /// Insert or replace one closed bar. Returns false for an unknown resolution.
    pub fn upsert(&mut self, label: &str, candle: VpCandle) -> bool {
        match interval_index(label) {
            Some(index) => {
                self.series[index].insert(candle.time, candle);
                true
            }
            None => false,
        }
    }

    /// Drop bars whose open time is before `cutoff_ms` from every series.
    pub fn prune_before(&mut self, cutoff_ms: i64) {
        for series in &mut self.series {
            let newer = series.split_off(&cutoff_ms);
            *series = newer;
        }
    }

    /// The bar opening at `time` for a resolution, if stored.
    pub fn get(&self, label: &str, time: i64) -> Option<&VpCandle> {
        self.series[interval_index(label)?].get(&time)
    }

    /// Open of the bar at `time`, preferring the 1H bar and falling back to 1m.
    pub fn open_at(&self, time: i64) -> Option<f64> {
        self.get("1h", time)
            .or_else(|| self.get("1m", time))
            .map(|bar| bar.open)
    }

    /// Bars in `[start, end)` from the finest resolution that covers the
    /// window. Returns `None` when no resolution reaches the coverage floor.
    pub fn window(&self, start: i64, end: i64) -> Option<(&'static str, Vec<VpCandle>)> {
        if end <= start {
            return None;
        }
        for (index, label) in INTERVALS.iter().copied().enumerate() {
            let expected = (end - start) / INTERVAL_MS[index];
            let bars = self.collect(index, start, end);
            if bars.len() >= 2 && bars.len() as i64 * 100 >= expected * COVERAGE_MIN_PERCENT {
                return Some((label, bars));
            }
        }
        None
    }

    /// Bars in `[start, end)` from the finest resolution with at least two
    /// bars, without a coverage floor. Used for explicit custom ranges.
    pub fn finest_bars(&self, start: i64, end: i64) -> Option<(&'static str, Vec<VpCandle>)> {
        if end <= start {
            return None;
        }
        for (index, label) in INTERVALS.iter().copied().enumerate() {
            let bars = self.collect(index, start, end);
            if bars.len() >= 2 {
                return Some((label, bars));
            }
        }
        None
    }

    /// Per-series counts and time bounds for diagnostics.
    pub fn stats(&self) -> Vec<SeriesStat> {
        INTERVALS
            .iter()
            .copied()
            .enumerate()
            .map(|(index, label)| SeriesStat {
                interval: label.to_string(),
                bars: self.series[index].len(),
                oldest_ms: self.series[index].keys().next().copied(),
                newest_ms: self.series[index].keys().next_back().copied(),
            })
            .collect()
    }

    fn collect(&self, index: usize, start: i64, end: i64) -> Vec<VpCandle> {
        self.series[index]
            .range(start..end)
            .map(|(_, bar)| bar.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time: i64, open: f64) -> VpCandle {
        VpCandle {
            time,
            open,
            high: open + 1.0,
            low: open - 1.0,
            close: open,
            volume: 10.0,
            source: "test".into(),
        }
    }

    #[test]
    fn upsert_replaces_by_open_time_and_rejects_unknown_intervals() {
        let mut store = CandleStore::default();
        assert!(store.upsert("1m", bar(60_000, 1.0)));
        assert!(store.upsert("1m", bar(60_000, 2.0)));
        assert!(!store.upsert("5m", bar(60_000, 1.0)));
        assert_eq!(store.stats()[0].bars, 1);
        assert_eq!(store.get("1m", 60_000).map(|b| b.open), Some(2.0));
    }

    #[test]
    fn window_prefers_finest_resolution_with_coverage() {
        let mut store = CandleStore::default();
        // Ten 1m bars: full coverage of a 10-minute window.
        for index in 0..10 {
            store.upsert("1m", bar(index * 60_000, 1.0));
        }
        let (label, bars) = store.window(0, 600_000).expect("covered");
        assert_eq!(label, "1m");
        assert_eq!(bars.len(), 10);
        // Only two of ten 1m bars exist in the window: coverage fails.
        let mut sparse = CandleStore::default();
        sparse.upsert("1m", bar(0, 1.0));
        sparse.upsert("1m", bar(60_000, 1.0));
        assert!(sparse.window(0, 600_000).is_none());
        // ...but custom ranges accept it.
        assert_eq!(sparse.finest_bars(0, 600_000).map(|(_, b)| b.len()), Some(2));
    }

    #[test]
    fn prune_drops_bars_before_cutoff() {
        let mut store = CandleStore::default();
        store.upsert("1h", bar(0, 1.0));
        store.upsert("1h", bar(3_600_000, 1.0));
        store.prune_before(3_600_000);
        assert_eq!(store.stats()[2].bars, 1);
        assert_eq!(store.stats()[2].oldest_ms, Some(3_600_000));
    }
}
