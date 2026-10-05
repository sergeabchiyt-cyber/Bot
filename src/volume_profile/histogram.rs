//! Fixed-bin volume histogram shared by every VP window.
//!
//! Each profile bins candle volume into $0.50 price rows (allocated by the
//! candle's actual high/low overlap with each row), takes the Point of
//! Control (POC) as the highest-volume row, then expands a 70% value area
//! (VAH/VAL) outward from the POC.

use chrono::Utc;

use crate::types::{VpCandle, VpLevels};

/// Width of one price row. Gold moves in cents; $0.50 rows keep the POC
/// stable while still resolving intraday value.
pub const BIN_SIZE: f64 = 0.50;
/// Share of total window volume the value area must contain.
pub const VA_PCT: f64 = 0.70;

/// Build a price histogram over `candles`.
///
/// For swing profiles `range` is the exact best low/high of the directional
/// leg; session/week profiles pass `None` and derive their range from the
/// candles in that window.
pub fn compute(
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

    let (raw_lo, raw_hi) = range.unwrap_or_else(|| {
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
    if !raw_lo.is_finite() || !raw_hi.is_finite() || raw_hi <= raw_lo {
        return None;
    }
    // Keep row boundaries on a stable price grid. Anchoring each profile's
    // first row to its exact low shifts all rows by a different fractional
    // amount each week, making otherwise identical profiles incomparable.
    let lo = (raw_lo / BIN_SIZE).floor() * BIN_SIZE;
    let hi = (raw_hi / BIN_SIZE).ceil() * BIN_SIZE;
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

        let low_idx = bin_index(clipped_low, lo, n_bins);
        let high_idx = bin_index(clipped_high, lo, n_bins);
        let candle_range = c.high - c.low;

        if candle_range <= f64::EPSILON {
            bins[low_idx] += c.volume;
            continue;
        }

        // Allocate volume by the actual overlap with each price row. Splitting
        // it equally among every row in a candle's range shifts POC/VA when
        // ranges are uneven.
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
        sunday_open: None,
    })
}

fn bin_index(price: f64, lo: f64, n_bins: usize) -> usize {
    (((price - lo) / BIN_SIZE).floor() as isize)
        .clamp(0, n_bins as isize - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn poc_sits_in_the_heaviest_band_and_value_area_contains_it() {
        let mut candles = Vec::new();
        let mut t = 1_700_000_000_000;
        for _ in 0..20 {
            candles.push(candle(t, 3390.0, 3392.0, 10.0));
            t += 900_000;
        }
        for _ in 0..20 {
            candles.push(candle(t, 3400.0, 3402.0, 100.0));
            t += 900_000;
        }
        let lv = compute(
            &candles,
            "PS",
            candles[0].time,
            t,
            None,
            "neutral",
            None,
            None,
        )
        .expect("profile");
        assert_eq!(lv.window, "PS");
        assert!(
            (3400.0..=3402.0).contains(&lv.poc),
            "poc {} not in heavy band",
            lv.poc
        );
        assert!(lv.val <= lv.poc && lv.poc <= lv.vah);
    }

    #[test]
    fn price_rows_are_aligned_to_the_fixed_half_dollar_grid() {
        let candles = vec![
            candle(1_700_000_000_000, 100.10, 100.30, 10.0),
            candle(1_700_000_000_001, 100.10, 100.30, 10.0),
        ];
        let levels = compute(&candles, "PW", 0, 1, None, "neutral", None, None)
            .expect("aligned profile");
        assert_eq!(levels.poc, 100.25);
        assert_eq!(levels.val, 100.0);
        assert_eq!(levels.vah, 100.5);
    }

    #[test]
    fn empty_or_flat_windows_produce_no_levels() {
        assert!(compute(&[], "PS", 0, 1, None, "neutral", None, None).is_none());
        let flat = vec![candle(1_700_000_000_000, 3400.0, 3400.0, 50.0)];
        assert!(compute(&flat, "PS", 0, 1, None, "neutral", None, None).is_none());
    }
}
