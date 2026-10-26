//! Fixed-range volume histogram with TradingView-parity row and value-area rules.
//!
//! Ported from Node 1's `volume_profile` engine. Swing overlays are not part of
//! Node 3's contract and are dropped.
//!
//! Row rules (`RowMode::Rows`): the tick size is one cent. The row height is a
//! whole number of ticks, chosen so the row count sits closest to the target.
//! Rows anchor at the profile low, and the top row is clipped to the raw high.
//! Each bar's volume spreads across rows by price overlap. Up volume means
//! close >= open.
//!
//! Value area: starting at the POC, grow one row at a time toward the larger
//! adjacent row until the target share of volume is covered. Ties go to the
//! row closer to the POC, then the row above.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::types::VpCandle;

/// TradingView "Number Of Rows" default. The reference profiles use 128.
pub const DEFAULT_ROWS: usize = 128;
/// Default fixed-width bin in price units, used by `RowMode::Price`.
pub const DEFAULT_BIN_SIZE: f64 = 0.50;
/// XAUUSD tick: one cent. Row heights are whole multiples of this.
pub const DEFAULT_TICK_SIZE: f64 = 0.01;
/// Share of total volume the value area must contain (TradingView default 70%).
pub const DEFAULT_VA_PCT: f64 = 0.70;
const MAX_ROWS: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowMode {
    Rows,
    Price,
}

impl RowMode {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "price" | "fixed" | "bin" | "bins" | "bin_size" | "ticks_per_row" => RowMode::Price,
            _ => RowMode::Rows,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RowMode::Rows => "rows",
            RowMode::Price => "price",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfileModel {
    pub row_mode: RowMode,
    pub rows: usize,
    pub bin_size: f64,
    pub tick_size: f64,
    pub va_pct: f64,
}

impl Default for ProfileModel {
    fn default() -> Self {
        Self {
            row_mode: RowMode::Rows,
            rows: DEFAULT_ROWS,
            bin_size: DEFAULT_BIN_SIZE,
            tick_size: DEFAULT_TICK_SIZE,
            va_pct: DEFAULT_VA_PCT,
        }
    }
}

impl ProfileModel {
    /// Clamp user-supplied values into safe ranges.
    pub fn sanitized(&self) -> Self {
        Self {
            row_mode: self.row_mode,
            rows: self.rows.clamp(2, 5_000),
            bin_size: if self.bin_size.is_finite() && self.bin_size > 0.0 {
                self.bin_size
            } else {
                DEFAULT_BIN_SIZE
            },
            tick_size: if self.tick_size.is_finite() && self.tick_size > 0.0 {
                self.tick_size
            } else {
                DEFAULT_TICK_SIZE
            },
            va_pct: if self.va_pct.is_finite() {
                self.va_pct.clamp(0.05, 0.99)
            } else {
                DEFAULT_VA_PCT
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileRow {
    pub low: f64,
    pub high: f64,
    pub price: f64,
    pub volume: f64,
    pub up_volume: f64,
    pub down_volume: f64,
}

#[derive(Debug, Clone)]
pub struct Histogram {
    pub rows: Vec<ProfileRow>,
    pub row_height: f64,
    pub range_low: f64,
    pub range_high: f64,
    pub total_volume: f64,
    pub poc_index: usize,
    pub val_index: usize,
    pub vah_index: usize,
    pub model: ProfileModel,
}

impl Histogram {
    pub fn poc(&self) -> f64 {
        self.rows[self.poc_index].price
    }

    /// High edge of the top value-area row.
    pub fn vah(&self) -> f64 {
        self.rows[self.vah_index].high
    }

    /// Low edge of the bottom value-area row.
    pub fn val(&self) -> f64 {
        self.rows[self.val_index].low
    }
}

/// Row grid for a price range: `(grid_low, row_height, row_count)`.
pub fn row_grid(lo: f64, hi: f64, model: &ProfileModel) -> Option<(f64, f64, usize)> {
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return None;
    }
    let model = model.sanitized();
    match model.row_mode {
        RowMode::Price => {
            let grid_lo = (lo / model.bin_size).floor() * model.bin_size;
            let grid_hi = (hi / model.bin_size).ceil() * model.bin_size;
            let count = ((grid_hi - grid_lo) / model.bin_size).ceil() as usize;
            if count > MAX_ROWS {
                return None;
            }
            Some((grid_lo, model.bin_size, count.max(1)))
        }
        RowMode::Rows => {
            let span = hi - lo;
            let exact_ticks = span / model.rows as f64 / model.tick_size;
            if !exact_ticks.is_finite() {
                return None;
            }
            let floor_ticks = exact_ticks.floor().max(1.0);
            let ceil_ticks = exact_ticks.ceil().max(1.0);
            let count_for = |ticks: f64| -> usize {
                let height = ticks * model.tick_size;
                ((span / height).ceil() as usize).max(1)
            };
            let floor_rows = count_for(floor_ticks);
            let ceil_rows = count_for(ceil_ticks);
            let (ticks, count) = if ceil_ticks > floor_ticks
                && ceil_rows.abs_diff(model.rows) < floor_rows.abs_diff(model.rows)
            {
                (ceil_ticks, ceil_rows)
            } else {
                (floor_ticks, floor_rows)
            };
            if count > MAX_ROWS {
                return None;
            }
            Some((lo, ticks * model.tick_size, count))
        }
    }
}

/// Value area over raw row volumes: `(poc_index, val_index, vah_index)`.
pub fn value_area(bins: &[f64], va_pct: f64) -> Option<(usize, usize, usize)> {
    let total: f64 = bins.iter().sum();
    if bins.is_empty() || !total.is_finite() || total <= 0.0 {
        return None;
    }
    let poc = bins
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(Ordering::Equal))
        .map(|(index, _)| index)?;
    let count = bins.len();
    let target = total * va_pct;
    let mut va_sum = bins[poc];
    let mut lo = poc;
    let mut hi = poc;
    while va_sum < target && (lo > 0 || hi + 1 < count) {
        let lo_vol = if lo > 0 {
            bins[lo - 1]
        } else {
            f64::NEG_INFINITY
        };
        let hi_vol = if hi + 1 < count {
            bins[hi + 1]
        } else {
            f64::NEG_INFINITY
        };
        let take_upper = if (hi_vol - lo_vol).abs() <= f64::EPSILON {
            let dist_up = (hi + 1) - poc;
            let dist_down = poc - lo.saturating_sub(1);
            dist_up <= dist_down
        } else {
            hi_vol > lo_vol
        };
        if take_upper {
            hi += 1;
            va_sum += bins[hi];
        } else {
            lo -= 1;
            va_sum += bins[lo];
        }
    }
    Some((poc, lo, hi))
}

struct Buckets {
    total: Vec<f64>,
    up: Vec<f64>,
    down: Vec<f64>,
}

impl Buckets {
    fn new(count: usize) -> Self {
        Self {
            total: vec![0.0; count],
            up: vec![0.0; count],
            down: vec![0.0; count],
        }
    }

    fn add(&mut self, index: usize, volume: f64, is_up: bool) {
        self.total[index] += volume;
        if is_up {
            self.up[index] += volume;
        } else {
            self.down[index] += volume;
        }
    }
}

fn bin_index(price: f64, lo: f64, count: usize, row_height: f64) -> usize {
    let raw = ((price - lo) / row_height).floor() as isize;
    raw.clamp(0, count as isize - 1) as usize
}

/// Build the profile over `candles`. Bars without positive finite volume and a
/// valid range are ignored. Returns `None` when no volume remains.
pub fn histogram(candles: &[VpCandle], model: &ProfileModel) -> Option<Histogram> {
    let model = model.sanitized();
    let valid: Vec<&VpCandle> = candles
        .iter()
        .filter(|candle| {
            candle.time > 0
                && candle.open.is_finite()
                && candle.high.is_finite()
                && candle.low.is_finite()
                && candle.close.is_finite()
                && candle.volume.is_finite()
                && candle.volume > 0.0
                && candle.high >= candle.low
        })
        .collect();
    if valid.is_empty() {
        return None;
    }
    let raw_lo = valid.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    let raw_hi = valid
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let (lo, row_height, count) = row_grid(raw_lo, raw_hi, &model)?;
    let grid_hi = lo + count as f64 * row_height;
    let top = match model.row_mode {
        RowMode::Rows => raw_hi.min(grid_hi),
        RowMode::Price => grid_hi,
    };

    let mut buckets = Buckets::new(count);
    for candle in valid {
        let low_index = bin_index(candle.low, lo, count, row_height);
        let high_index = bin_index(candle.high, lo, count, row_height);
        let is_up = candle.close >= candle.open;
        let span = candle.high - candle.low;
        if span <= f64::EPSILON {
            buckets.add(low_index, candle.volume, is_up);
            continue;
        }
        for index in low_index..=high_index {
            let bin_low = lo + index as f64 * row_height;
            let bin_high = bin_low + row_height;
            let overlap = (candle.high.min(bin_high) - candle.low.max(bin_low)).max(0.0);
            if overlap > 0.0 {
                buckets.add(index, candle.volume * overlap / span, is_up);
            }
        }
    }

    let total: f64 = buckets.total.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let (poc_index, val_index, vah_index) = value_area(&buckets.total, model.va_pct)?;
    let rows = (0..count)
        .map(|index| {
            let low = lo + index as f64 * row_height;
            let high = if index + 1 == count {
                top
            } else {
                low + row_height
            };
            ProfileRow {
                low,
                high,
                price: (low + high) / 2.0,
                volume: buckets.total[index],
                up_volume: buckets.up[index],
                down_volume: buckets.down[index],
            }
        })
        .collect();

    Some(Histogram {
        rows,
        row_height,
        range_low: raw_lo,
        range_high: raw_hi,
        total_volume: total,
        poc_index,
        val_index,
        vah_index,
        model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(time: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> VpCandle {
        VpCandle {
            time,
            open,
            high,
            low,
            close,
            volume,
            source: "test".into(),
        }
    }

    fn model(rows: usize) -> ProfileModel {
        ProfileModel {
            rows,
            tick_size: 0.5,
            ..ProfileModel::default()
        }
    }

    #[test]
    fn row_grid_uses_whole_ticks_and_hits_the_target_count() {
        let (grid_lo, height, count) = row_grid(0.0, 3.5, &model(7)).expect("grid");
        assert_eq!(grid_lo, 0.0);
        assert!((height - 0.5).abs() < 1e-9);
        assert_eq!(count, 7);
    }

    #[test]
    fn value_area_grows_toward_the_larger_adjacent_row() {
        // Total 17, target 11.9. From POC (index 2) the lower neighbour (3)
        // beats the upper (2), so the area grows down first and stops at 1..=2.
        let bins = [1.0, 3.0, 10.0, 2.0, 1.0];
        let (poc, val, vah) = value_area(&bins, 0.70).expect("va");
        assert_eq!(poc, 2);
        assert_eq!((val, vah), (1, 2));
    }

    #[test]
    fn value_area_ties_grow_upward_when_equidistant() {
        // Neighbours equal and equidistant from POC: the tie goes to the row above.
        // Total 16, target 8. POC 10 alone already exceeds the target; use 0.50.
        let bins = [1.0, 2.0, 10.0, 2.0, 1.0];
        let (poc, val, vah) = value_area(&bins, 0.50).expect("va");
        assert_eq!(poc, 2);
        assert_eq!((val, vah), (2, 2));
        // Need one more row: 70% target 11.2 forces growth; the tie picks upward.
        let (_, val, vah) = value_area(&bins, 0.70).expect("va");
        assert_eq!((val, vah), (2, 3));
    }

    #[test]
    fn up_and_down_volume_split_by_close_against_open() {
        let candles = [
            candle(60_000, 1.0, 2.0, 1.0, 2.0, 10.0),
            candle(120_000, 2.0, 2.0, 1.0, 1.0, 4.0),
        ];
        let hist = histogram(&candles, &model(2)).expect("hist");
        let up: f64 = hist.rows.iter().map(|r| r.up_volume).sum();
        let down: f64 = hist.rows.iter().map(|r| r.down_volume).sum();
        assert!((up - 10.0).abs() < 1e-9);
        assert!((down - 4.0).abs() < 1e-9);
        assert!((hist.total_volume - 14.0).abs() < 1e-9);
    }

    #[test]
    fn top_row_is_clipped_to_raw_high_in_rows_mode() {
        let candles = [candle(60_000, 10.0, 10.9, 10.0, 10.5, 5.0)];
        let hist = histogram(&candles, &model(3)).expect("hist");
        let last = hist.rows.last().expect("row");
        assert!(last.high <= 10.9 + 1e-9);
    }

    #[test]
    fn empty_or_zero_volume_input_yields_no_profile() {
        assert!(histogram(&[], &ProfileModel::default()).is_none());
        let flat = [candle(60_000, 1.0, 1.0, 1.0, 1.0, 0.0)];
        assert!(histogram(&flat, &ProfileModel::default()).is_none());
    }

    #[test]
    fn price_mode_uses_fixed_bins_aligned_to_the_grid() {
        let model = ProfileModel {
            row_mode: RowMode::Price,
            bin_size: 0.5,
            ..ProfileModel::default()
        };
        let (grid_lo, height, count) = row_grid(10.1, 11.2, &model).expect("grid");
        assert!((grid_lo - 10.0).abs() < 1e-9);
        assert!((height - 0.5).abs() < 1e-9);
        assert_eq!(count, 3);
    }
}
