//! Deterministic synthetic history for air-gapped CI.
//!
//! `SEED_SYNTHETIC_CANDLES=1` seeds a fixed price series when no upstream
//! history is reachable (hosted runners, offline development). It is never
//! enabled by default, and every bar is labelled `source: "synthetic"` so a
//! consumer can tell it apart from the real feed at a glance.
//!
//! The minutes are generated first and the coarser resolutions are *folded*
//! from them, so 15m and 1h bars are exactly the aggregate of the 1m bars
//! underneath. That makes the synthetic set usable for recomputing whole
//! windows downstream instead of three unrelated series.

use crate::types::{Candle, Interval, SYMBOL};

/// Bars kept in the 1m series (matches `candles::CAPACITY_M1`).
const M1_BARS: usize = 43_200;
/// Bars kept in the 15m series (the chart seed contract).
const M15_BARS: usize = 2_000;
/// Bars kept in the 1h series.
const H1_BARS: usize = 2_000;
/// 1h coverage drives how many minutes have to be generated.
const MINUTES: usize = H1_BARS * 60;

const SOURCE: &str = "synthetic";

/// The synthetic price at minute `index` since the series start.
///
/// A slow drift plus an intraday wave: it looks like a gold chart, it is
/// perfectly reproducible, and it never depends on `rand`.
fn price_at(index: usize) -> f64 {
    let day = (index / 1_440) as f64;
    let phase = (index % 1_440) as f64 / 1_440.0 * std::f64::consts::TAU;
    3300.0 + day * 1.5 + phase.sin() * 12.0
}

fn minute_bar(time: i64, index: usize) -> Candle {
    let open = price_at(index);
    let close = price_at(index + 1);
    Candle {
        symbol: SYMBOL.to_string(),
        interval: Interval::M1.as_str().to_string(),
        time,
        open,
        high: open.max(close) + 0.25,
        low: open.min(close) - 0.25,
        close,
        volume: 100.0 + (index % 17) as f64 * 3.0,
        source: SOURCE.to_string(),
        closed: true,
    }
}

/// Fold finer bars into `interval`. Buckets with no input produce nothing.
fn fold(bars: &[Candle], interval: Interval) -> Vec<Candle> {
    let mut out: Vec<Candle> = Vec::new();
    for bar in bars {
        let bucket = interval.align(bar.time);
        match out.last_mut() {
            Some(last) if last.time == bucket => {
                last.high = last.high.max(bar.high);
                last.low = last.low.min(bar.low);
                last.close = bar.close;
                last.volume += bar.volume;
            }
            _ => out.push(Candle {
                symbol: SYMBOL.to_string(),
                interval: interval.as_str().to_string(),
                time: bucket,
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
                source: SOURCE.to_string(),
                closed: true,
            }),
        }
    }
    out
}

/// The full synthetic set: 1m, 15m and 1h, all ending at the newest bucket
/// that is completely in the past relative to `now_ms`.
pub fn history(now_ms: i64) -> Vec<(Interval, Vec<Candle>)> {
    let last_bucket = Interval::M1.align(now_ms) - Interval::M1.millis();
    let start = last_bucket - (MINUTES as i64 - 1) * Interval::M1.millis();

    let minutes: Vec<Candle> = (0..MINUTES)
        .map(|index| {
            let time = start + index as i64 * Interval::M1.millis();
            minute_bar(time, index)
        })
        .collect();

    let m15 = fold(&minutes, Interval::M15);
    let h1 = fold(&minutes, Interval::H1);

    let keep = |bars: Vec<Candle>, count: usize| -> Vec<Candle> {
        let skip = bars.len().saturating_sub(count);
        bars.into_iter().skip(skip).collect()
    };

    vec![
        (Interval::M1, keep(minutes, M1_BARS)),
        (Interval::M15, keep(m15, M15_BARS)),
        (Interval::H1, keep(h1, H1_BARS)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_781_867_153;

    #[test]
    fn the_set_has_all_three_resolutions_and_the_documented_spans() {
        let set = history(NOW);
        assert_eq!(set.len(), 3);
        for (interval, bars) in &set {
            assert!(
                bars.len() >= 2_000,
                "{} has {} bars",
                interval.as_str(),
                bars.len()
            );
            assert!(bars.iter().all(|bar| bar.interval == interval.as_str()));
            assert!(bars.iter().all(|bar| bar.closed));
            assert!(bars.iter().all(Candle::is_valid));
            assert!(
                bars.iter().all(|bar| bar.time % interval.millis() == 0),
                "{} bars are off the bucket grid",
                interval.as_str()
            );
            assert!(bars.windows(2).all(|pair| pair[0].time < pair[1].time));
        }
        assert_eq!(set[0].1.len(), M1_BARS.min(MINUTES));
    }

    #[test]
    fn history_ends_before_the_forming_bucket() {
        let set = history(NOW);
        for (interval, bars) in &set {
            let last = bars.last().expect("non-empty");
            assert!(last.time < NOW, "{} ends in the future", interval.as_str());
            assert_eq!(last.time, interval.align(last.time));
        }
    }

    #[test]
    fn coarse_bars_are_the_fold_of_the_minute_bars() {
        let set = history(NOW);
        let minutes = &set[0].1;
        let quarters = &set[1].1;
        // Compare the overlapping tail.
        let first = quarters.first().expect("15m bars").time;
        let overlapping: Vec<Candle> = minutes
            .iter()
            .filter(|b| b.time >= first)
            .cloned()
            .collect();
        let refolded = fold(&overlapping, Interval::M15);
        assert_eq!(refolded.len(), quarters.len());
        for (folded, actual) in refolded.iter().zip(quarters.iter()) {
            assert_eq!(folded.time, actual.time);
            assert!((folded.open - actual.open).abs() < 1e-9);
            assert!((folded.high - actual.high).abs() < 1e-9);
            assert!((folded.low - actual.low).abs() < 1e-9);
            assert!((folded.close - actual.close).abs() < 1e-9);
            assert!((folded.volume - actual.volume).abs() < 1e-9);
        }
    }

    #[test]
    fn the_series_is_reproducible() {
        let a = history(NOW);
        let b = history(NOW);
        assert_eq!(a[0].1.len(), b[0].1.len());
        assert_eq!(a[2].1.last().unwrap().close, b[2].1.last().unwrap().close);
        // A different clock moves the whole set, it does not rewrite it.
        let c = history(NOW + 3_600_000);
        assert_eq!(a[2].1.len(), c[2].1.len());
        assert_ne!(a[2].1.last().unwrap().time, c[2].1.last().unwrap().time);
    }
}
