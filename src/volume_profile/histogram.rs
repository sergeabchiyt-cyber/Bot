//! Volume histogram shared by every VP window, modelled on TradingView's
//! Volume Profile math so the levels can be compared with (and match) the
//! chart.
//!
//! # Row model
//!
//! TradingView's "Rows Layout" has two modes, and the engine mirrors both:
//!
//! * **Number Of Rows** (`RowMode::Rows`, the default, `VP_ROWS=128`):
//!   the row height is derived from the profile range and rounded to a whole
//!   number of symbol ticks, exactly as documented for the FRVP tool —
//!   `Ticks Per Row = round((Histogram Top - Histogram Bottom) / Rows / Tick Size)`.
//!   Extra rows are created when the range is not an exact multiple, so the
//!   produced row count can exceed the requested one. Rows are anchored at
//!   the **profile low** (TV's histogram bottom), never on a fixed $ grid.
//! * **Ticks Per Row / fixed price bins** (`RowMode::Price`): a constant row
//!   height (`VP_BIN_SIZE`, the legacy $0.50 grid), snapped so bins land on
//!   multiples of the size.
//!
//! # Volume
//!
//! Each bar's volume is spread across the rows its high/low range covers, in
//! proportion to the overlap with each row (a uniform distribution inside the
//! bar). Up/down volume follows TradingView: `close >= open` is up volume,
//! everything else is down volume. Rows are totalled for the POC and value
//! area, which is what TradingView does even when the chart shows Up/Down.
//!
//! # POC / value area
//!
//! POC = the row with the highest total volume (reported at the row centre).
//! The value area grows from the POC by repeatedly adding the larger of the
//! two rows adjacent to the current area until `VA_PCT` of the total is
//! included; on an exact volume tie the row closer to the POC wins, and on an
//! equal distance the row above wins — TradingView's documented tie-break.
//! VAH is the top edge of the highest included row, VAL the bottom edge of the
//! lowest one.

use chrono::Utc;

use crate::types::{ProfileMeta, VpCandle, VpLevels};

/// Default number of rows when `VP_ROW_MODE=rows` (TradingView "Number Of
/// Rows" layout). The user's FRVP uses 128.
pub const DEFAULT_ROWS: usize = 128;
/// Default row height (price units) when `VP_ROW_MODE=price`.
pub const DEFAULT_BIN_SIZE: f64 = 0.50;
/// XAUUSD tick size: one cent. TradingView rounds row heights to whole ticks.
pub const DEFAULT_TICK_SIZE: f64 = 0.01;
/// Share of total window volume the value area must contain (TV default 70).
pub const VA_PCT: f64 = 0.70;
/// Hard ceiling on histogram rows: a mis-set `VP_BIN_SIZE` must not allocate
/// gigabytes.
const MAX_ROWS: usize = 50_000;

/// Which of TradingView's two "Rows Layout" modes the histogram uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowMode {
    /// "Number Of Rows": row height is derived and rounded to whole ticks.
    Rows,
    /// "Ticks Per Row" with a fixed price height (the legacy $ grid).
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

/// Everything needed to lay out the histogram rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileModel {
    pub row_mode: RowMode,
    /// TV "Row Size" when `row_mode == Rows`.
    pub rows: usize,
    /// Row height when `row_mode == Price`.
    pub bin_size: f64,
    /// Symbol tick: row heights are rounded to whole ticks.
    pub tick_size: f64,
    /// TV "Value Area Volume", as a fraction (0.70).
    pub va_pct: f64,
}

impl Default for ProfileModel {
    fn default() -> Self {
        Self {
            row_mode: RowMode::Rows,
            rows: DEFAULT_ROWS,
            bin_size: DEFAULT_BIN_SIZE,
            tick_size: DEFAULT_TICK_SIZE,
            va_pct: VA_PCT,
        }
    }
}

impl ProfileModel {
    /// Clamp a configuration into a range the math can survive.
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
                VA_PCT
            },
        }
    }

}

/// One price row of the histogram, with the up/down split TradingView's
/// "Volume: Up/Down" scheme displays.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProfileRow {
    /// Bottom edge of the row.
    pub low: f64,
    /// Top edge of the row (the last row may be shorter than `row_height`).
    pub high: f64,
    /// Row mid-point: where the POC/VA lines are drawn.
    pub price: f64,
    pub volume: f64,
    pub up_volume: f64,
    pub down_volume: f64,
}

/// The full computed histogram plus the POC/value-area indices into `rows`.
#[derive(Debug, Clone, PartialEq)]
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

    pub fn vah(&self) -> f64 {
        self.rows[self.vah_index].high
    }

    pub fn val(&self) -> f64 {
        self.rows[self.val_index].low
    }

    /// Build the `VpLevels` wire payload for this histogram.
    pub fn levels(
        &self,
        window: &str,
        start: i64,
        end: i64,
        direction: &str,
        swing_high: Option<f64>,
        swing_low: Option<f64>,
        input_interval: &str,
        input_bars: usize,
    ) -> VpLevels {
        VpLevels {
            window: window.into(),
            poc: self.poc(),
            vah: self.vah(),
            val: self.val(),
            start,
            end,
            timestamp: Utc::now().timestamp_millis(),
            direction: direction.into(),
            swing_high,
            swing_low,
            sunday_open: None,
            meta: Some(ProfileMeta {
                row_mode: self.model.row_mode.as_str().into(),
                row_height: self.row_height,
                rows: self.rows.len(),
                range_high: self.range_high,
                range_low: self.range_low,
                input_interval: input_interval.into(),
                input_bars,
                total_volume: self.total_volume,
                va_pct: self.model.va_pct,
            }),
        }
    }
}

/// Lay out the row grid for `[lo, hi]` under `model`.
///
/// Returns `(grid_low, row_height, row_count)`. In `Rows` mode the row height
/// is `ticks * tick_size` with the tick count chosen — per TradingView — as
/// whichever of `floor`/`ceil` of the exact division lands the produced row
/// count closer to the requested one.
pub fn row_grid(lo: f64, hi: f64, model: &ProfileModel) -> Option<(f64, f64, usize)> {
    if !lo.is_finite() || !hi.is_finite() || hi <= lo {
        return None;
    }
    let model = model.sanitized();
    match model.row_mode {
        RowMode::Price => {
            let grid_lo = (lo / model.bin_size).floor() * model.bin_size;
            let grid_hi = (hi / model.bin_size).ceil() * model.bin_size;
            let n = ((grid_hi - grid_lo) / model.bin_size).ceil() as usize;
            // A too-small bin would allocate an unbounded histogram: refuse
            // it rather than OOM the engine.
            if n > MAX_ROWS {
                return None;
            }
            Some((grid_lo, model.bin_size, n.max(1)))
        }
        RowMode::Rows => {
            let span = hi - lo;
            let exact_ticks = span / model.rows as f64 / model.tick_size;
            if !exact_ticks.is_finite() {
                return None;
            }
            // Never let a row collapse below one tick.
            let floor_ticks = exact_ticks.floor().max(1.0);
            let ceil_ticks = exact_ticks.ceil().max(1.0);

            let count_for = |ticks: f64| -> usize {
                let height = ticks * model.tick_size;
                ((span / height).ceil() as usize).max(1)
            };
            // TV picks the candidate whose *produced* row count is closest to
            // the requested one ("may also create additional rows to ensure
            // all of the data is represented"). Ties keep the finer grid.
            let floor_rows = count_for(floor_ticks);
            let ceil_rows = count_for(ceil_ticks);
            let floor_err = floor_rows.abs_diff(model.rows);
            let ceil_err = ceil_rows.abs_diff(model.rows);
            let (ticks, n) = if ceil_ticks > floor_ticks && ceil_err < floor_err {
                (ceil_ticks, ceil_rows)
            } else {
                (floor_ticks, floor_rows)
            };
            if n > MAX_ROWS {
                return None;
            }
            let height = ticks * model.tick_size;
            Some((lo, height, n))
        }
    }
}

/// Build the price histogram over `candles`.
///
/// For swing profiles `range` is the exact best low/high of the directional
/// leg; session/week profiles pass `None` and derive their range from the
/// candles in that window (TradingView's histogram top/bottom).
pub fn histogram(
    candles: &[VpCandle],
    range: Option<(f64, f64)>,
    model: &ProfileModel,
) -> Option<Histogram> {
    if candles.is_empty() {
        return None;
    }
    let model = model.sanitized();

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
        let lo = valid.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        let hi = valid
            .iter()
            .map(|c| c.high)
            .fold(f64::NEG_INFINITY, f64::max);
        (lo, hi)
    });
    if !raw_lo.is_finite() || !raw_hi.is_finite() || raw_hi <= raw_lo {
        return None;
    }

    let (lo, row_height, n_bins) = row_grid(raw_lo, raw_hi, &model)?;
    let grid_hi = lo + n_bins as f64 * row_height;
    // In Rows mode the top row is *partial* when the range is not an exact
    // multiple of the row height (TradingView's extra row), so its top edge is
    // the profile high, not the grid edge. The fixed-price grid keeps whole
    // rows, as it always has.
    let top = match model.row_mode {
        RowMode::Rows => raw_hi.min(grid_hi),
        RowMode::Price => grid_hi,
    };

    let mut bins = vec![0.0_f64; n_bins];
    let mut up_bins = vec![0.0_f64; n_bins];
    let mut down_bins = vec![0.0_f64; n_bins];

    for c in valid {
        let low_idx = bin_index(c.low, lo, n_bins, row_height);
        let high_idx = bin_index(c.high, lo, n_bins, row_height);
        let candle_range = c.high - c.low;
        let is_up = c.close >= c.open;
        let mut add = |idx: usize, volume: f64| {
            bins[idx] += volume;
            if is_up {
                up_bins[idx] += volume;
            } else {
                down_bins[idx] += volume;
            }
        };

        if candle_range <= f64::EPSILON {
            // A doji-less bar (or a venue filler bar): everything on one row.
            add(low_idx, c.volume);
            continue;
        }

        // Allocate by the actual overlap with each price row. Splitting it
        // equally among every row in a candle's range shifts POC/VA when
        // ranges are uneven.
        for i in low_idx..=high_idx {
            let bin_low = lo + i as f64 * row_height;
            let bin_high = bin_low + row_height;
            let overlap_low = c.low.max(bin_low);
            let overlap_high = c.high.min(bin_high);
            let overlap = (overlap_high - overlap_low).max(0.0);
            if overlap > 0.0 {
                add(i, c.volume * overlap / candle_range);
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

    // Value area: start at the POC and repeatedly add the larger adjacent
    // row until `va_pct` of the volume is inside. TradingView breaks an exact
    // volume tie in favour of the row closer to the POC, and an exact
    // distance tie in favour of the row above.
    let target = total * model.va_pct;
    let mut va_sum = bins[poc_idx];
    let mut lo_idx = poc_idx;
    let mut hi_idx = poc_idx;

    while va_sum < target && (lo_idx > 0 || hi_idx + 1 < n_bins) {
        let lo_vol = if lo_idx > 0 {
            bins[lo_idx - 1]
        } else {
            f64::NEG_INFINITY
        };
        let hi_vol = if hi_idx + 1 < n_bins {
            bins[hi_idx + 1]
        } else {
            f64::NEG_INFINITY
        };
        let take_upper = if (hi_vol - lo_vol).abs() <= f64::EPSILON {
            let dist_up = (hi_idx + 1) - poc_idx;
            let dist_down = poc_idx - (lo_idx.saturating_sub(1));
            dist_up <= dist_down
        } else {
            hi_vol > lo_vol
        };
        if take_upper {
            hi_idx += 1;
            va_sum += bins[hi_idx];
        } else {
            lo_idx -= 1;
            va_sum += bins[lo_idx];
        }
    }

    let rows = (0..n_bins)
        .map(|i| {
            let low = lo + i as f64 * row_height;
            let high = if i + 1 == n_bins {
                top
            } else {
                low + row_height
            };
            ProfileRow {
                low,
                high,
                price: (low + high) / 2.0,
                volume: bins[i],
                up_volume: up_bins[i],
                down_volume: down_bins[i],
            }
        })
        .collect();

    Some(Histogram {
        rows,
        row_height,
        range_low: raw_lo,
        range_high: raw_hi,
        total_volume: total,
        poc_index: poc_idx,
        val_index: lo_idx,
        vah_index: hi_idx,
        model,
    })
}

fn bin_index(price: f64, lo: f64, n_bins: usize, row_height: f64) -> usize {
    (((price - lo) / row_height).floor() as isize).clamp(0, n_bins as isize - 1) as usize
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

    fn rows_model(rows: usize) -> ProfileModel {
        ProfileModel {
            row_mode: RowMode::Rows,
            rows,
            ..ProfileModel::default()
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
        let hist = histogram(&candles, None, &rows_model(128)).expect("profile");
        assert!(
            (3400.0..=3402.0).contains(&hist.poc()),
            "poc {} not in heavy band",
            hist.poc()
        );
        assert!(hist.val() <= hist.poc() && hist.poc() <= hist.vah());
    }

    #[test]
    fn price_rows_are_aligned_to_the_fixed_half_dollar_grid() {
        let candles = vec![
            candle(1_700_000_000_000, 100.10, 100.30, 10.0),
            candle(1_700_000_000_001, 100.10, 100.30, 10.0),
        ];
        let model = ProfileModel {
            row_mode: RowMode::Price,
            ..ProfileModel::default()
        };
        let hist = histogram(&candles, None, &model).expect("aligned profile");
        assert_eq!(hist.poc(), 100.25);
        assert_eq!(hist.val(), 100.0);
        assert_eq!(hist.vah(), 100.5);
    }

    #[test]
    fn empty_or_flat_windows_produce_no_levels() {
        assert!(histogram(&[], None, &rows_model(128)).is_none());
        let flat = vec![candle(1_700_000_000_000, 3400.0, 3400.0, 50.0)];
        assert!(histogram(&flat, None, &rows_model(128)).is_none());
    }

    /// TradingView's own worked example: a range spanning 100 ticks with
    /// "Number Of Rows" = 25 gives 4-tick rows (25 rows), while 30 rows first
    /// suggests 3.33 ticks -> rounded to 3, producing 34 rows; 34 is closer to
    /// 30 than 25 is, so 3-tick rows win.
    #[test]
    fn rows_layout_rounds_row_height_to_whole_ticks_like_tradingview() {
        let lo = 10.0;
        let hi = 11.0;

        let (grid_lo, height, n) = row_grid(lo, hi, &rows_model(25)).expect("grid");
        assert_eq!(grid_lo, lo, "rows are anchored at the profile low");
        assert!((height - 0.04).abs() < 1e-12, "height {height}");
        assert_eq!(n, 25);

        let (_, height, n) = row_grid(lo, hi, &rows_model(30)).expect("grid");
        assert!(
            (height - 0.03).abs() < 1e-12,
            "expected 3-tick rows, got {height}"
        );
        // Ceil(100/3) = 34 rows: the last row is a partial top row.
        assert_eq!(n, 34);
    }

    #[test]
    fn rows_layout_creates_a_short_top_row_for_a_partial_range() {
        // 0.10 range over a 0.01 tick: 12 requested rows -> 1 tick rows
        // (10 rows) vs 2 tick rows (5 rows); 10 is closer to 12.
        let (lo, height, n) = row_grid(4150.0, 4150.10, &rows_model(12)).expect("grid");
        assert_eq!(lo, 4150.0);
        assert!((height - 0.01).abs() < 1e-12);
        assert_eq!(n, 10);
    }

    #[test]
    fn rows_are_not_snapped_to_the_half_dollar_grid_in_rows_mode() {
        // A range that is not a multiple of $0.50 must still start at its low.
        let candles = vec![
            candle(1_700_000_000_000, 4155.13, 4155.63, 10.0),
            candle(1_700_000_000_001, 4155.13, 4155.63, 10.0),
        ];
        let hist = histogram(&candles, None, &rows_model(10)).expect("grid");
        assert_eq!(hist.rows[0].low, 4155.13);
        assert!(hist.row_height > 0.0);
        assert!(hist.rows.last().unwrap().high >= 4155.63);
    }

    /// TradingView breaks an exact volume tie in favour of the row closer to
    /// the POC (and, at equal distance, the row above). With a symmetric
    /// ladder the rule is the only thing that pulls the value area lower.
    #[test]
    fn value_area_tie_break_prefers_the_row_closer_to_the_poc() {
        // Seven $0.01 rows, volumes 4,4,10,4,4,4,4 (total 34, target 23.8).
        // POC on row 2. Steps: up (tie at distance 1), up (tie at distance 2),
        // then the tie at distances 3 vs 2 must go DOWN, and the final
        // expansion back up lands on rows 0..=5.
        let volumes = [4.0, 4.0, 10.0, 4.0, 4.0, 4.0, 4.0];
        let candles: Vec<VpCandle> = volumes
            .iter()
            .enumerate()
            .map(|(i, volume)| {
                let low = i as f64 * 0.01;
                candle(i as i64 + 1, low, low + 0.01, *volume)
            })
            .collect();
        let hist = histogram(&candles, Some((0.0, 0.07)), &rows_model(7)).expect("profile");
        assert_eq!(hist.rows.len(), 7);
        assert_eq!(hist.poc_index, 2);
        assert_eq!(hist.val_index, 0, "the nearer row below must win the tie");
        assert_eq!(hist.vah_index, 5);
        assert!(hist.val() <= hist.poc() && hist.poc() <= hist.vah());
    }

    #[test]
    fn up_and_down_volume_follow_the_bar_direction() {
        let candles = vec![
            // up bar (close > open)
            VpCandle {
                time: 1,
                open: 4155.0,
                high: 4156.0,
                low: 4155.0,
                close: 4156.0,
                volume: 100.0,
                source: "test".into(),
            },
            // down bar
            VpCandle {
                time: 2,
                open: 4155.0,
                high: 4155.0,
                low: 4154.0,
                close: 4154.0,
                volume: 30.0,
                source: "test".into(),
            },
        ];
        let hist = histogram(&candles, None, &rows_model(16)).expect("profile");
        let up: f64 = hist.rows.iter().map(|r| r.up_volume).sum();
        let down: f64 = hist.rows.iter().map(|r| r.down_volume).sum();
        assert!((up - 100.0).abs() < 1e-9, "up {up}");
        assert!((down - 30.0).abs() < 1e-9, "down {down}");
        assert!((hist.total_volume - 130.0).abs() < 1e-9);
    }

    #[test]
    fn levels_payload_carries_the_audit_metadata() {
        let candles = vec![candle(1, 4155.0, 4156.0, 10.0)];
        let hist = histogram(&candles, None, &rows_model(64)).expect("profile");
        let levels = hist.levels("PW", 0, 1, "neutral", None, None, "5m", 7);
        let meta = levels.meta.expect("meta");
        assert_eq!(meta.row_mode, "rows");
        assert_eq!(meta.input_interval, "5m");
        assert_eq!(meta.input_bars, 7);
        assert_eq!(meta.rows, hist.rows.len());
        assert_eq!(meta.range_low, 4155.0);
        assert_eq!(meta.range_high, 4156.0);
        assert!(meta.row_height > 0.0);
    }

    #[test]
    fn absurd_bin_sizes_are_rejected_instead_of_allocating() {
        let model = ProfileModel {
            row_mode: RowMode::Price,
            bin_size: 0.00001,
            ..ProfileModel::default()
        };
        let candles = vec![candle(1, 1000.0, 4200.0, 10.0)];
        assert!(histogram(&candles, None, &model).is_none());
    }
}
