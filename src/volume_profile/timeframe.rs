//! Profile input resolution, mirroring TradingView's lower-timeframe rule.
//!
//! TradingView's Volume Profile indicators are not computed from the chart's
//! bars. For the Fixed Range Volume Profile it walks a fixed sequence —
//! `1, 5, 15, 30, 60, 240, 1D` — and picks the first resolution whose bar
//! count for the selected range stays under 5,000:
//!
//! > *"When calculating volume profile fixed range, we check a list of
//! > timeframes in a sequence until the number of bars in the time interval
//! > for which VP is calculated will be fewer than 5000."*
//!
//! That matters for gold: a full trading week holds roughly 6,900 1-minute
//! bars, so TradingView builds a **weekly** profile from **5-minute** bars
//! (about 1,380 of them), while a single 18:00→18:00 session stays on **1m**
//! bars (about 1,380 bars). The engine keeps a 1m history and re-buckets it
//! per window, so every window uses the same input resolution TradingView
//! would have used — `LowerTf::Tv` ("tv", the default).
//!
//! `LowerTf::Fixed(ms)` pins one resolution for every window (e.g. `1m` for
//! the old finest-available behaviour).

use crate::types::VpCandle;

/// TradingView's lower-timeframe ladder, coarsest last.
pub const TV_LOWER_TIMEFRAMES: [(i64, &str); 7] = [
    (60_000, "1m"),
    (300_000, "5m"),
    (900_000, "15m"),
    (1_800_000, "30m"),
    (3_600_000, "1h"),
    (14_400_000, "4h"),
    (86_400_000, "1d"),
];

/// TradingView's bar-count cap for the chosen lower timeframe.
pub const TV_LOWER_TF_BAR_CAP: usize = 5_000;

/// Which resolution the profile windows are computed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowerTf {
    /// Replicate TradingView's 5,000-bar ladder per window (default).
    Tv,
    /// One fixed resolution in milliseconds for every window.
    Fixed(i64),
}

impl LowerTf {
    /// `VP_LOWER_TF=tv` (default) or an explicit resolution (`1m`, `5m`, ...).
    pub fn parse(raw: &str) -> Self {
        match parse_label(raw) {
            Some(ms) if ms > 0 => LowerTf::Fixed(ms),
            _ => LowerTf::Tv,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LowerTf::Tv => "tv",
            LowerTf::Fixed(ms) => label_for_ms(ms),
        }
    }
}

/// Parse a resolution label (`"15m"`, `"1h"`, `"4h"`, `"1d"`) into millis.
pub fn parse_label(raw: &str) -> Option<i64> {
    let raw = raw.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }
    let (digits, unit) = raw.split_at(raw.find(|c: char| !c.is_ascii_digit())?);
    let n: i64 = digits.parse().ok()?;
    let ms = match unit {
        "s" => 1_000,
        "m" | "min" => 60_000,
        "h" => 3_600_000,
        "d" | "1d" => 86_400_000,
        _ => return None,
    };
    Some(n * ms)
}

/// Human label for a bucket size, e.g. `300_000 -> "5m"`. Falls back to the
/// nearest known ladder entry for odd sizes.
pub fn label_for_ms(ms: i64) -> &'static str {
    for (known, label) in TV_LOWER_TIMEFRAMES {
        if known == ms {
            return label;
        }
    }
    TV_LOWER_TIMEFRAMES
        .iter()
        .min_by_key(|(known, _)| (known - ms).abs())
        .map(|(_, label)| *label)
        .unwrap_or("1m")
}

/// The bucket size the candles are already in: the smallest positive gap
/// between consecutive bars (a 15m series reports 900_000).
pub fn candle_interval_ms(candles: &[VpCandle]) -> Option<i64> {
    let mut times: Vec<i64> = candles.iter().map(|c| c.time).collect();
    times.sort_unstable();
    times
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|d| *d > 0)
        .min()
}

/// Count distinct buckets the candles would occupy at `interval_ms`. This is
/// the faithful stand-in for TradingView's "number of bars" check, measured on
/// the data the engine actually holds.
pub fn distinct_buckets(candles: &[VpCandle], interval_ms: i64) -> usize {
    if interval_ms <= 0 {
        return 0;
    }
    let mut seen: Vec<i64> = candles
        .iter()
        .map(|c| c.time - c.time.rem_euclid(interval_ms))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    seen.len()
}

/// Pick the resolution TradingView would use for a range containing `candles`:
/// the finest ladder entry whose distinct bar count stays under the cap.
pub fn tv_lower_timeframe(candles: &[VpCandle]) -> (i64, &'static str) {
    for (ms, label) in TV_LOWER_TIMEFRAMES {
        if distinct_buckets(candles, ms) < TV_LOWER_TF_BAR_CAP {
            return (ms, label);
        }
    }
    let (ms, label) = TV_LOWER_TIMEFRAMES[TV_LOWER_TIMEFRAMES.len() - 1];
    (ms, label)
}

/// Resolve the input resolution for one window and return the bars to feed
/// the histogram, plus the label of the resolution actually used.
///
/// Candles are only ever aggregated *up*: if the stored history is already
/// coarser than the requested resolution (the 15m chart seed fallback), the
/// history is used as-is and its own resolution is reported, so a client can
/// see that this profile was built from 15m bars.
pub fn prepare_input(
    candles: &[VpCandle],
    lower_tf: LowerTf,
) -> (Vec<VpCandle>, &'static str) {
    if candles.is_empty() {
        return (Vec::new(), "15m");
    }
    let input_ms = candle_interval_ms(candles).unwrap_or(15 * 60 * 1000);
    let target_ms = match lower_tf {
        LowerTf::Tv => tv_lower_timeframe(candles).0,
        LowerTf::Fixed(ms) => ms,
    };
    if target_ms <= input_ms {
        return (candles.to_vec(), label_for_ms(input_ms));
    }
    (aggregate(candles, target_ms), label_for_ms(target_ms))
}

/// Aggregate candles into buckets of `interval_ms` aligned to the UTC epoch
/// (5m bars start at :00, :05, :10, ...), summing tick volume and taking the
/// OHLC of the bucket. Input order does not matter.
pub fn aggregate(candles: &[VpCandle], interval_ms: i64) -> Vec<VpCandle> {
    if interval_ms <= 0 {
        return candles.to_vec();
    }
    let mut buckets: Vec<(i64, VpCandle)> = Vec::with_capacity(candles.len());
    for candle in candles {
        let start = candle.time - candle.time.rem_euclid(interval_ms);
        match buckets.last_mut() {
            Some((time, agg)) if *time == start => {
                agg.high = agg.high.max(candle.high);
                agg.low = agg.low.min(candle.low);
                agg.close = candle.close;
                agg.volume += candle.volume;
            }
            _ => buckets.push((
                start,
                VpCandle {
                    time: start,
                    open: candle.open,
                    high: candle.high,
                    low: candle.low,
                    close: candle.close,
                    volume: candle.volume,
                    source: candle.source.clone(),
                },
            )),
        }
    }
    buckets.sort_by_key(|(time, _)| *time);
    // Out-of-order input can leave two chunks for the same bucket: merge them.
    let mut merged: Vec<(i64, VpCandle)> = Vec::with_capacity(buckets.len());
    for (time, candle) in buckets {
        match merged.last_mut() {
            Some((last_time, last)) if *last_time == time => {
                last.high = last.high.max(candle.high);
                last.low = last.low.min(candle.low);
                last.close = candle.close;
                last.volume += candle.volume;
            }
            _ => merged.push((time, candle)),
        }
    }
    merged.into_iter().map(|(_, candle)| candle).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const M1: i64 = 60_000;

    fn bar(time: i64, low: f64, high: f64, volume: f64) -> VpCandle {
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

    #[test]
    fn labels_round_trip() {
        assert_eq!(parse_label("15m"), Some(900_000));
        assert_eq!(parse_label("5M"), Some(300_000));
        assert_eq!(parse_label("1h"), Some(3_600_000));
        assert_eq!(parse_label("4h"), Some(14_400_000));
        assert_eq!(parse_label("1d"), Some(86_400_000));
        assert_eq!(parse_label("tv"), None);
        assert_eq!(LowerTf::parse("tv"), LowerTf::Tv);
        assert_eq!(LowerTf::parse("nonsense"), LowerTf::Tv);
        assert_eq!(LowerTf::parse("1m"), LowerTf::Fixed(M1));
    }

    /// A full gold week is ~6,900 1m bars, so TradingView computes a weekly
    /// profile from 5m bars — while a single session stays on 1m.
    #[test]
    fn weekly_windows_use_5m_and_sessions_use_1m_like_tradingview() {
        // 5 days x 23 hours of 1m bars = 6,900 bars.
        let mut week = Vec::new();
        for hour in 0..(5 * 23) {
            for minute in 0..60 {
                week.push(bar((hour * 60 + minute) * M1, 4100.0, 4101.0, 1.0));
            }
        }
        assert!(week.len() > TV_LOWER_TF_BAR_CAP);
        let (ms, label) = tv_lower_timeframe(&week);
        assert_eq!((ms, label), (300_000, "5m"));

        // One session: 23 hours = 1,380 bars -> below the cap -> 1m.
        let session: Vec<VpCandle> = week.iter().take(23 * 60).cloned().collect();
        let (ms, label) = tv_lower_timeframe(&session);
        assert_eq!((ms, label), (M1, "1m"));
    }

    #[test]
    fn aggregation_sums_volume_and_keeps_the_bucket_ohlc() {
        let candles = vec![
            bar(0, 4100.0, 4101.0, 5.0),
            bar(M1, 4100.5, 4102.0, 7.0),
            bar(2 * M1, 4099.0, 4100.5, 3.0),
            bar(3 * M1, 4101.0, 4103.0, 1.0),
            bar(5 * M1, 4102.0, 4104.0, 2.0),
        ];
        let agg = aggregate(&candles, 300_000);
        assert_eq!(agg.len(), 2);
        assert_eq!(agg[0].time, 0);
        assert_eq!(agg[0].open, 4100.0);
        assert_eq!(agg[0].high, 4102.0);
        assert_eq!(agg[0].low, 4099.0);
        assert_eq!(agg[0].close, 4100.5);
        assert_eq!(agg[0].volume, 15.0);
        // The 5-minute bucket starting at 5m holds the last bar only.
        assert_eq!(agg[1].time, 300_000);
        assert_eq!(agg[1].volume, 2.0);
    }

    #[test]
    fn aggregation_merges_out_of_order_chunks() {
        let candles = vec![
            bar(6 * M1, 4100.0, 4101.0, 4.0),
            bar(0, 4100.0, 4101.0, 1.0),
            bar(M1, 4100.0, 4102.0, 2.0),
        ];
        let agg = aggregate(&candles, 300_000);
        assert_eq!(agg.len(), 2);
        assert_eq!(agg[0].time, 0);
        assert_eq!(agg[0].volume, 3.0);
        assert_eq!(agg[0].high, 4102.0);
    }

    #[test]
    fn prepare_input_never_downscales_coarse_history() {
        // 15m history: asking for TV parity cannot invent finer bars.
        let candles = vec![bar(0, 4100.0, 4101.0, 1.0), bar(900_000, 4101.0, 4102.0, 1.0)];
        let (out, label) = prepare_input(&candles, LowerTf::Tv);
        assert_eq!(out.len(), 2);
        assert_eq!(label, "15m");

        // A coarse history with a pinned finer target stays coarse too.
        let (out, label) = prepare_input(&candles, LowerTf::Fixed(M1));
        assert_eq!(out.len(), 2);
        assert_eq!(label, "15m");
    }

    #[test]
    fn prepare_input_reports_one_minute_for_short_windows() {
        let candles: Vec<VpCandle> = (0..120).map(|i| bar(i * M1, 4100.0, 4101.0, 1.0)).collect();
        let (_, label) = prepare_input(&candles, LowerTf::Tv);
        assert_eq!(label, "1m");
    }
}
