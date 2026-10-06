//! The single most-recent swing profile.
//!
//! Exactly one swing profile exists at a time: the most recent confirmed
//! swing high followed by a swing low is a bearish leg (high → low); the
//! most recent swing low followed by a swing high is a bullish leg
//! (low → high). The profile's price bounds are the best high and best low
//! inside that leg, not the arbitrary bounds of a calendar window.

use crate::types::{VpCandle, VpLevels};

use super::StoredProfile;
use super::histogram;
use super::timeframe::LowerTf;

/// Bars on each side of a fractal pivot. 3 × 15m = 45 minutes of
/// confirmation lag: responsive enough to track the current swing without
/// flipping on every minor wiggle.
pub const SWING_PIVOT_RADIUS: usize = 3;

/// How many recent bars the no-pivot fallback may span: one full 18:00 →
/// 18:00 session of 15m bars. A short or straight-line history still gets a
/// deterministic directional leg, but it never stretches across weeks of
/// stale data the way an unbounded fallback would.
pub const SWING_FALLBACK_BARS: usize = 96;

const FIFTEEN_MIN_MS: i64 = 15 * 60 * 1000;

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

/// Build the single current swing profile over `candles`.
///
/// Returns the wire levels plus the histogram behind them, so the engine can
/// serve the same audit (`GET /vp`) for the swing window as for PW/PS/CW.
pub fn compute_swing(
    candles: &[VpCandle],
    model: &histogram::ProfileModel,
    lower_tf: LowerTf,
) -> Option<(VpLevels, StoredProfile)> {
    if candles.len() < 2 {
        return None;
    }
    let leg = latest_swing(candles)?;
    let start_index = leg.start_index.min(leg.end_index);
    let end_index = leg.start_index.max(leg.end_index);
    let window = &candles[start_index..=end_index];
    let start = candles[start_index].time;
    let end = candles[end_index]
        .time
        .saturating_add(candle_interval(window));

    let (input, interval) = super::timeframe::prepare_input(window, lower_tf);
    let input_bars = input.len();
    let range = Some((leg.low, leg.high));
    let histogram = histogram::histogram(&input, range, model)?;
    let levels = histogram.levels(
        leg.direction.label(),
        start,
        end,
        leg.direction.as_str(),
        Some(leg.high),
        Some(leg.low),
        interval,
        input_bars,
    );
    let stored = StoredProfile {
        histogram,
        start,
        end,
        range,
        from_swing_history: true,
        input_interval: interval,
        input_bars,
    };
    Some((levels, stored))
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
            // Window extremes with ties allowed: an equal high/low still
            // counts, and `push_pivot` keeps the most recent of equals, so
            // double tops/bottoms anchor at their second touch.
            let is_high = high.is_finite()
                && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
                    .all(|j| j == index || !candles[j].high.is_finite() || high >= candles[j].high);
            let is_low = low.is_finite()
                && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
                    .all(|j| j == index || !candles[j].low.is_finite() || low <= candles[j].low);

            if is_high {
                push_pivot(&mut pivots, PivotKind::High, index, candles);
            } else if is_low {
                push_pivot(&mut pivots, PivotKind::Low, index, candles);
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

    fallback_leg(candles)
}

/// No confirmed pivot pair (short history or a straight-line trend): anchor
/// a deterministic leg between the best high and best low of the most recent
/// bars instead of spanning the whole retained history. Ties resolve to the
/// most recent bar.
fn fallback_leg(candles: &[VpCandle]) -> Option<SwingLeg> {
    let offset = candles.len().saturating_sub(SWING_FALLBACK_BARS);
    let window = &candles[offset..];
    if window.len() < 2 {
        return None;
    }

    let mut high_rel = 0;
    let mut low_rel = 0;
    for (i, c) in window.iter().enumerate() {
        if c.high.is_finite()
            && (!window[high_rel].high.is_finite() || c.high >= window[high_rel].high)
        {
            high_rel = i;
        }
        if c.low.is_finite() && (!window[low_rel].low.is_finite() || c.low <= window[low_rel].low) {
            low_rel = i;
        }
    }
    if high_rel == low_rel {
        return None;
    }

    let (start_rel, end_rel, direction) = if low_rel < high_rel {
        (low_rel, high_rel, SwingDirection::Bullish)
    } else {
        (high_rel, low_rel, SwingDirection::Bearish)
    };
    let slice = &window[start_rel..=end_rel];
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
        Some(SwingLeg {
            start_index: offset + start_rel,
            end_index: offset + end_rel,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    const MIN: i64 = 60_000;

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

    fn swing_history(prices: &[f64]) -> Vec<VpCandle> {
        let start = Utc::now().timestamp_millis() - prices.len() as i64 * 15 * MIN;
        prices
            .iter()
            .enumerate()
            .map(|(i, price)| candle(start + i as i64 * 15 * MIN, price - 0.5, price + 0.5, 100.0))
            .collect()
    }

    /// The 128-row TradingView model every window uses by default.
    fn model() -> histogram::ProfileModel {
        histogram::ProfileModel::default()
    }

    fn levels_of(computed: Option<(VpLevels, StoredProfile)>) -> VpLevels {
        computed.expect("swing profile").0
    }

    #[test]
    fn bearish_swing_is_anchored_high_to_low() {
        let prices = [
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 99.0,
            98.0, 97.0, 96.0, 95.0, 96.0, 97.0, 98.0, 99.0, 100.0, 101.0, 102.0, 103.0, 104.0,
        ];
        let swing = levels_of(compute_swing(
            &swing_history(&prices),
            &model(),
            LowerTf::Tv,
        ));
        assert_eq!(swing.window, "SWING_BEAR");
        assert_eq!(swing.direction, "bearish");
        assert_eq!(swing.swing_high, Some(105.5));
        assert_eq!(swing.swing_low, Some(94.5));
        assert!(swing.start < swing.end);
    }

    #[test]
    fn bullish_swing_is_anchored_low_to_high() {
        let prices = [
            105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0,
            107.0, 108.0, 109.0, 110.0, 109.0, 108.0, 107.0, 106.0, 105.0, 104.0, 103.0, 102.0,
            101.0,
        ];
        let swing = levels_of(compute_swing(
            &swing_history(&prices),
            &model(),
            LowerTf::Tv,
        ));
        assert_eq!(swing.window, "SWING_BULL");
        assert_eq!(swing.direction, "bullish");
        assert_eq!(swing.swing_low, Some(99.5));
        assert_eq!(swing.swing_high, Some(110.5));
        assert!(swing.start < swing.end);
    }

    #[test]
    fn plateau_highs_anchor_at_the_most_recent_touch() {
        // Double top with two equal 103 touches: the leg must start at the
        // *second* touch (index 4), not the first.
        let prices = [
            100.0, 101.0, 102.0, 103.0, 103.0, 102.0, 101.0, 100.0, 99.0, 98.0, 99.0, 100.0, 101.0,
            102.0, 103.0,
        ];
        let candles = swing_history(&prices);
        let swing = levels_of(compute_swing(&candles, &model(), LowerTf::Tv));
        assert_eq!(swing.window, "SWING_BEAR");
        assert_eq!(swing.direction, "bearish");
        assert_eq!(swing.swing_high, Some(103.5));
        assert_eq!(swing.swing_low, Some(97.5));
        assert_eq!(swing.start, candles[4].time);
        assert_eq!(swing.end, candles[9].time + 15 * MIN);
        assert_eq!(swing.end - swing.start, 6 * 15 * MIN);
    }

    #[test]
    fn fallback_leg_is_bounded_to_recent_bars() {
        // Straight-line uptrend: no confirmed pivots at all, so the leg must
        // come from the most recent SWING_FALLBACK_BARS bars — never the
        // whole retained history.
        let prices: Vec<f64> = (0..150).map(|i| 100.0 + i as f64 * 0.1).collect();
        let candles = swing_history(&prices);
        let swing = levels_of(compute_swing(&candles, &model(), LowerTf::Tv));
        assert_eq!(swing.window, "SWING_BULL");
        assert_eq!(swing.direction, "bullish");
        assert!(swing.swing_low.unwrap() < swing.swing_high.unwrap());
        assert_eq!(
            swing.end - swing.start,
            SWING_FALLBACK_BARS as i64 * 15 * MIN
        );
        assert_eq!(swing.start, candles[150 - SWING_FALLBACK_BARS].time);
    }
}
