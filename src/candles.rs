//! Multi-timeframe candle book.
//!
//! One tick source feeds three aligned resolutions (1m, 15m, 1h). A bucket is
//! written to the cache exactly once — when the next bucket's first tick
//! arrives — and the bucket still being filled is published as `closed: false`
//! and never served by `/candles`.
//!
//! The book lives outside the WebSocket connection loop, so a reconnect keeps
//! the in-progress buckets and their tick counts instead of restarting them.
//!
//! A `std::sync::Mutex` is used (not tokio's `RwLock`): the writer is the
//! synchronous tick handler, every critical section is a few arithmetic
//! operations, and the lock is never held across an `.await`.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::types::{Candle, Interval, SYMBOL, TickVolumeBar};

/// 1m history retained: a full trading week (~7,200 bars) plus the current
/// week, so a consumer can rebuild the weekly profiles from Node 1 alone.
pub const CAPACITY_M1: usize = 43_200;
/// 15m history retained — the chart contract's 2,000-bar seed.
pub const CAPACITY_M15: usize = 2_000;
/// 1h history retained — the drawing-boundary resolution.
pub const CAPACITY_H1: usize = 2_000;

/// Source label for live and historical bars from the market feed.
pub const SOURCE: &str = "sifting";

/// Rolling window for `ticks_per_sec`.
const RATE_WINDOW_MS: i64 = 10_000;

/// Why a tick was not counted. Every drop increments a counter in `/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// Non-finite or non-positive price.
    Price,
    /// Non-positive timestamp.
    Timestamp,
    /// A 10-digit epoch-seconds value where the contract requires milliseconds.
    SecondsTimestamp,
    /// More than five seconds ahead of the local clock.
    Future,
    /// Byte-identical repeat of the previous tick (the re-subscribe snapshot).
    Repeat,
    /// A tick for a bucket that has already closed.
    Late,
}

impl DropReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DropReason::Price => "price",
            DropReason::Timestamp => "timestamp",
            DropReason::SecondsTimestamp => "seconds_timestamp",
            DropReason::Future => "future",
            DropReason::Repeat => "repeat",
            DropReason::Late => "late",
        }
    }
}

/// Accepted / dropped tick tallies, reported in `/status`.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TickCounters {
    pub accepted: u64,
    pub price: u64,
    pub timestamp: u64,
    pub seconds_timestamp: u64,
    pub future: u64,
    pub repeat: u64,
    pub late: u64,
}

impl TickCounters {
    pub fn dropped(&self) -> u64 {
        self.price + self.timestamp + self.seconds_timestamp + self.future + self.repeat + self.late
    }

    fn record(&mut self, reason: DropReason) {
        match reason {
            DropReason::Price => self.price += 1,
            DropReason::Timestamp => self.timestamp += 1,
            DropReason::SecondsTimestamp => self.seconds_timestamp += 1,
            DropReason::Future => self.future += 1,
            DropReason::Repeat => self.repeat += 1,
            DropReason::Late => self.late += 1,
        }
    }
}

/// Outcome of one accepted tick.
///
/// `closed` holds the bars that finalised on this tick (1m, 15m, 1h order) and
/// `live` the buckets now being filled. Publishers must send every `closed`
/// frame before the first `live` frame so `closed == true` always precedes the
/// next bucket.
#[derive(Debug, Clone, Default)]
pub struct TickEvent {
    pub dropped: Option<DropReason>,
    pub closed: Vec<Candle>,
    pub live: Vec<Candle>,
    pub tick_volume_closed: Option<TickVolumeBar>,
    pub tick_volume: Option<TickVolumeBar>,
    /// Feed timestamp of the accepted tick (freshness input).
    pub last_tick_ts: Option<i64>,
}

impl TickEvent {
    pub fn accepted(&self) -> bool {
        self.dropped.is_none()
    }
}

/// What a seed merge did, so boot can log and `/status` can be honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedReport {
    pub interval: Interval,
    pub accepted: usize,
    /// Rows that replaced a bar the series already had (same `time`).
    pub merged: usize,
    pub rejected_unaligned: usize,
    pub rejected_invalid: usize,
    /// Bars in the series after the merge.
    pub bars: usize,
}

impl SeedReport {
    pub fn rejected(&self) -> usize {
        self.rejected_unaligned + self.rejected_invalid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickDir {
    Up,
    Down,
    Flat,
}

impl TickDir {
    fn classify(previous: Option<f64>, price: f64) -> Self {
        match previous {
            Some(prev) if price > prev => TickDir::Up,
            Some(prev) if price < prev => TickDir::Down,
            _ => TickDir::Flat,
        }
    }
}

/// One bucket being built from ticks.
#[derive(Debug, Clone, Copy)]
struct Aggregator {
    start: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    ticks: u64,
    up: u64,
    down: u64,
    last_tick: i64,
}

impl Aggregator {
    fn new(start: i64, price: f64, ts: i64, dir: TickDir) -> Self {
        let mut agg = Self {
            start,
            open: price,
            high: price,
            low: price,
            close: price,
            ticks: 0,
            up: 0,
            down: 0,
            last_tick: ts,
        };
        agg.count(ts, dir);
        agg
    }

    fn update(&mut self, price: f64, ts: i64, dir: TickDir) {
        if price > self.high {
            self.high = price;
        }
        if price < self.low {
            self.low = price;
        }
        self.close = price;
        self.count(ts, dir);
    }

    fn count(&mut self, ts: i64, dir: TickDir) {
        self.ticks += 1;
        match dir {
            TickDir::Up => self.up += 1,
            TickDir::Down => self.down += 1,
            TickDir::Flat => {}
        }
        self.last_tick = self.last_tick.max(ts);
    }

    fn candle(&self, interval: Interval, closed: bool) -> Candle {
        Candle {
            symbol: SYMBOL.to_string(),
            interval: interval.as_str().to_string(),
            time: self.start,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.ticks as f64,
            source: SOURCE.to_string(),
            closed,
        }
    }

    fn tick_volume(&self, ticks_per_sec: f64, closed: bool) -> TickVolumeBar {
        TickVolumeBar {
            symbol: SYMBOL.to_string(),
            interval: Interval::M15.as_str().to_string(),
            time: self.start,
            ticks: self.ticks,
            up_ticks: self.up,
            down_ticks: self.down,
            flat_ticks: self.ticks - self.up - self.down,
            close: self.close,
            last_tick: self.last_tick,
            ticks_per_sec,
            closed,
            source: SOURCE.to_string(),
        }
    }
}

/// Closed bars for one resolution, oldest first, capped at `capacity`.
#[derive(Debug)]
struct Series {
    interval: Interval,
    capacity: usize,
    bars: VecDeque<Candle>,
}

impl Series {
    fn new(interval: Interval, capacity: usize) -> Self {
        Self {
            interval,
            capacity: capacity.max(1),
            bars: VecDeque::new(),
        }
    }

    /// Append (or replace) one closed bar, keeping the series ascending and
    /// bounded. The newest bar is never the one evicted.
    fn push(&mut self, bar: Candle) {
        let last = self.bars.back().map(|last| last.time);
        match last {
            Some(time) if time == bar.time => {
                if let Some(slot) = self.bars.back_mut() {
                    *slot = bar;
                }
            }
            Some(time) if time > bar.time => {
                match self.bars.binary_search_by_key(&bar.time, |b| b.time) {
                    Ok(index) => self.bars[index] = bar,
                    Err(index) => self.bars.insert(index, bar),
                }
            }
            _ => {
                self.bars.push_back(bar);
                self.trim();
            }
        }
    }

    fn trim(&mut self) {
        while self.bars.len() > self.capacity {
            self.bars.pop_front();
        }
    }

    /// Merge a history page. Rows that are off the bucket grid, invalid or for
    /// another resolution are rejected; duplicates merge by `time` and the
    /// result is ascending and unique.
    fn seed(&mut self, bars: &[Candle]) -> SeedReport {
        let mut report = SeedReport {
            interval: self.interval,
            accepted: 0,
            merged: 0,
            rejected_unaligned: 0,
            rejected_invalid: 0,
            bars: 0,
        };
        let mut by_time: BTreeMap<i64, Candle> = self
            .bars
            .iter()
            .map(|bar| (bar.time, bar.clone()))
            .collect();
        let before = by_time.len();
        for bar in bars {
            let aligned = bar.time > 0 && self.interval.align(bar.time) == bar.time;
            if !aligned || Interval::parse(&bar.interval) != Some(self.interval) {
                report.rejected_unaligned += 1;
                continue;
            }
            let mut bar = bar.clone();
            bar.closed = true;
            if !bar.is_valid() {
                report.rejected_invalid += 1;
                continue;
            }
            by_time.insert(bar.time, bar);
            report.accepted += 1;
        }
        let grown = by_time.len().saturating_sub(before);
        report.merged = report.accepted.saturating_sub(grown);
        self.bars = by_time.into_values().collect();
        self.trim();
        report.bars = self.bars.len();
        report
    }

    fn recent(&self, limit: usize) -> Vec<Candle> {
        let skip = self.bars.len().saturating_sub(limit);
        self.bars.iter().skip(skip).cloned().collect()
    }
}

#[derive(Debug)]
struct Book {
    series: [Series; 3],
    aggs: [Option<Aggregator>; 3],
    last_price: Option<f64>,
    last_key: Option<(i64, u64, u64)>,
    recent: VecDeque<i64>,
    newest_ts: i64,
    counters: TickCounters,
}

impl Book {
    fn ticks_per_sec(&self) -> f64 {
        let rate = self.recent.len() as f64 / (RATE_WINDOW_MS as f64 / 1000.0);
        (rate * 100.0).round() / 100.0
    }

    fn reject(
        &mut self,
        price: f64,
        ts: i64,
        size: Option<f64>,
        now_ms: i64,
    ) -> Option<DropReason> {
        if !price.is_finite() || price <= 0.0 {
            return Some(DropReason::Price);
        }
        if ts <= 0 {
            return Some(DropReason::Timestamp);
        }
        // A seconds value is three orders of magnitude too small; accepting it
        // would file the tick in 1970 and close every live bucket.
        if !crate::types::is_epoch_ms(ts) {
            return Some(DropReason::SecondsTimestamp);
        }
        if ts > now_ms + crate::types::MAX_FUTURE_MS {
            return Some(DropReason::Future);
        }
        let key = (ts, price.to_bits(), size.unwrap_or(0.0).to_bits());
        if self.last_key == Some(key) {
            return Some(DropReason::Repeat);
        }
        // A tick for a bucket that has already rolled would rewrite history:
        // it used to truncate a "closed" bar and re-emit it.
        let bucket = Interval::M1.align(ts);
        if self.aggs[Interval::M1.index()].is_some_and(|agg| bucket < agg.start) {
            return Some(DropReason::Late);
        }
        self.last_key = Some(key);
        None
    }

    fn advance(&mut self, price: f64, ts: i64) -> TickEvent {
        let rate = self.ticks_per_sec();
        let dir = TickDir::classify(self.last_price, price);
        self.last_price = Some(price);

        let mut event = TickEvent::default();
        for interval in Interval::ALL {
            let index = interval.index();
            let bucket = interval.align(ts);
            match self.aggs[index] {
                Some(agg) if agg.start == bucket => {
                    let mut agg = agg;
                    agg.update(price, ts, dir);
                    self.aggs[index] = Some(agg);
                    event.live.push(agg.candle(interval, false));
                }
                Some(agg) if bucket < agg.start => {
                    // Later resolution, already rolled by a coarser bucket.
                }
                Some(agg) => {
                    let closed = agg.candle(interval, true);
                    self.series[index].push(closed.clone());
                    event.closed.push(closed);
                    if interval == Interval::M15 {
                        event.tick_volume_closed = Some(agg.tick_volume(rate, true));
                    }
                    let fresh = Aggregator::new(bucket, price, ts, dir);
                    self.aggs[index] = Some(fresh);
                    event.live.push(fresh.candle(interval, false));
                }
                None => {
                    let fresh = Aggregator::new(bucket, price, ts, dir);
                    self.aggs[index] = Some(fresh);
                    event.live.push(fresh.candle(interval, false));
                }
            }
        }

        self.recent.push_back(ts);
        self.newest_ts = self.newest_ts.max(ts);
        let cutoff = self.newest_ts - RATE_WINDOW_MS;
        while self.recent.front().is_some_and(|front| *front <= cutoff) {
            self.recent.pop_front();
        }
        let ticks_per_sec = self.ticks_per_sec();
        let index = Interval::M15.index();
        event.tick_volume = self.aggs[index].map(|agg| agg.tick_volume(ticks_per_sec, false));
        event.last_tick_ts = Some(ts);
        self.counters.accepted += 1;
        event
    }
}

/// Shared candle cache for every published resolution.
#[derive(Debug, Clone)]
pub struct CandleBook {
    inner: Arc<Mutex<Book>>,
}

impl CandleBook {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Book {
                series: [
                    Series::new(Interval::M1, CAPACITY_M1),
                    Series::new(Interval::M15, CAPACITY_M15),
                    Series::new(Interval::H1, CAPACITY_H1),
                ],
                aggs: [None; 3],
                last_price: None,
                last_key: None,
                recent: VecDeque::new(),
                newest_ts: 0,
                counters: TickCounters::default(),
            })),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Book> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Closed bars, oldest first. The forming bucket is never included.
    pub fn closed(&self, interval: Interval) -> Vec<Candle> {
        self.lock().series[interval.index()]
            .bars
            .iter()
            .cloned()
            .collect()
    }

    /// The last `limit` closed bars.
    pub fn recent(&self, interval: Interval, limit: usize) -> Vec<Candle> {
        self.lock().series[interval.index()].recent(limit)
    }

    /// The bucket currently being filled, as `closed: false`.
    pub fn forming(&self, interval: Interval) -> Option<Candle> {
        self.lock().aggs[interval.index()].map(|agg| agg.candle(interval, false))
    }

    pub fn bars(&self, interval: Interval) -> usize {
        self.lock().series[interval.index()].bars.len()
    }

    /// Newest closed bar's bucket-open time.
    pub fn edge(&self, interval: Interval) -> Option<i64> {
        self.lock().series[interval.index()]
            .bars
            .back()
            .map(|bar| bar.time)
    }

    /// Oldest retained bar's bucket-open time.
    pub fn origin(&self, interval: Interval) -> Option<i64> {
        self.lock().series[interval.index()]
            .bars
            .front()
            .map(|bar| bar.time)
    }

    pub fn counters(&self) -> TickCounters {
        self.lock().counters.clone()
    }

    /// Merge a history page for one resolution.
    pub fn seed(&self, interval: Interval, bars: &[Candle]) -> SeedReport {
        self.lock().series[interval.index()].seed(bars)
    }

    /// Fold one tick in.
    ///
    /// Validation happens here rather than at the socket so the rules are
    /// unit-testable without a feed: bad prices, second-based timestamps,
    /// future ticks, exact repeats and late ticks are all refused and counted.
    pub fn on_tick(&self, price: f64, ts: i64, size: Option<f64>, now_ms: i64) -> TickEvent {
        let mut book = self.lock();
        if let Some(reason) = book.reject(price, ts, size, now_ms) {
            book.counters.record(reason);
            return TickEvent {
                dropped: Some(reason),
                ..TickEvent::default()
            };
        }
        book.advance(price, ts)
    }
}

impl Default for CandleBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M1: i64 = 60_000;
    const M15: i64 = 15 * M1;
    const H1: i64 = 60 * M1;
    const T0: i64 = 1_700_000_100_000 - 1_700_000_100_000 % H1;
    const NOW: i64 = T0 + 6 * H1;

    fn book() -> CandleBook {
        CandleBook::new()
    }

    fn bar(interval: Interval, time: i64) -> Candle {
        Candle {
            symbol: SYMBOL.into(),
            interval: interval.as_str().into(),
            time,
            open: 3380.0,
            high: 3382.0,
            low: 3379.0,
            close: 3381.0,
            volume: 10.0,
            source: SOURCE.into(),
            closed: true,
        }
    }

    #[test]
    fn the_three_resolutions_roll_together() {
        let book = book();
        let event = book.on_tick(3380.0, T0, None, NOW);
        assert!(event.accepted());
        assert_eq!(event.live.len(), 3);
        for (index, interval) in Interval::ALL.iter().enumerate() {
            assert_eq!(event.live[index].interval, interval.as_str());
            assert_eq!(event.live[index].time, interval.align(T0));
            assert!(!event.live[index].closed);
        }

        let event = book.on_tick(3381.0, T0 + M1, None, NOW);
        assert_eq!(event.closed.len(), 1, "only the 1m bucket closes here");
        assert_eq!(event.closed[0].interval, "1m");
        assert!(event.closed[0].closed);
        assert_eq!(event.closed[0].volume, 1.0);
        assert_eq!(book.bars(Interval::M1), 1);
        assert_eq!(book.bars(Interval::M15), 0);

        let event = book.on_tick(3382.0, T0 + M15, None, NOW);
        let closed: Vec<&str> = event.closed.iter().map(|c| c.interval.as_str()).collect();
        assert_eq!(closed, vec!["1m", "15m"]);
        // The 15m bar is the fold of its two 1m bars.
        assert_eq!(event.closed[1].open, 3380.0);
        assert_eq!(event.closed[1].high, 3381.0);
        assert_eq!(event.closed[1].close, 3381.0);
        assert_eq!(event.closed[1].volume, 2.0);
        assert_eq!(book.bars(Interval::M15), 1);
    }

    #[test]
    fn a_closed_bar_is_published_exactly_once_and_before_the_next_bucket() {
        let book = book();
        book.on_tick(3380.0, T0, None, NOW);
        book.on_tick(3380.5, T0 + 1_000, None, NOW);

        let roll = book.on_tick(3381.0, T0 + M1, None, NOW);
        assert_eq!(roll.closed.len(), 1);
        assert!(roll.closed[0].closed);
        assert!(roll.live.iter().all(|candle| !candle.closed));
        assert_eq!(roll.live[0].time, T0 + M1, "the new bucket is already live");

        // Ticks inside the same bucket never re-close it.
        for step in 1..5 {
            let event = book.on_tick(3380.0 + step as f64 * 0.1, T0 + M1 + step, None, NOW);
            assert!(event.closed.is_empty());
        }
        assert_eq!(book.closed(Interval::M1).len(), 1);
    }

    #[test]
    fn the_forming_bucket_is_not_in_the_candle_cache() {
        let book = book();
        book.on_tick(3380.0, T0, None, NOW);
        assert!(book.closed(Interval::M1).is_empty());
        let forming = book
            .forming(Interval::M1)
            .expect("a bucket is being filled");
        assert!(!forming.closed);
        assert_eq!(forming.time, T0);

        // Roll it, then the cache has exactly that bar.
        book.on_tick(3381.0, T0 + M1, None, NOW);
        let closed = book.closed(Interval::M1);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].time, T0);
        assert!(closed[0].closed);
    }

    #[test]
    fn non_finite_and_non_positive_prices_are_dropped() {
        let book = book();
        for price in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            let event = book.on_tick(price, T0, None, NOW);
            assert_eq!(event.dropped, Some(DropReason::Price), "{price}");
            assert!(event.live.is_empty());
        }
        assert_eq!(book.counters().price, 4);
        assert_eq!(book.counters().accepted, 0);
        assert!(book.closed(Interval::M1).is_empty());
    }

    #[test]
    fn second_based_timestamps_are_dropped() {
        let book = book();
        let event = book.on_tick(3380.0, T0 / 1000, None, NOW);
        assert_eq!(event.dropped, Some(DropReason::SecondsTimestamp));
        assert_eq!(book.counters().seconds_timestamp, 1);

        let event = book.on_tick(3380.0, 0, None, NOW);
        assert_eq!(event.dropped, Some(DropReason::Timestamp));
    }

    #[test]
    fn ticks_more_than_five_seconds_in_the_future_are_dropped() {
        let book = book();
        let event = book.on_tick(3380.0, NOW + 5_001, None, NOW);
        assert_eq!(event.dropped, Some(DropReason::Future));
        // Exactly five seconds ahead is still accepted.
        let event = book.on_tick(3380.0, NOW + 5_000, None, NOW);
        assert!(event.accepted());
    }

    #[test]
    fn late_ticks_do_not_rewrite_a_closed_bucket() {
        let book = book();
        book.on_tick(3380.0, T0, None, NOW);
        book.on_tick(3381.0, T0 + M1, None, NOW);
        assert_eq!(book.closed(Interval::M1).len(), 1);

        let event = book.on_tick(3390.0, T0 + 30_000, None, NOW);
        assert_eq!(event.dropped, Some(DropReason::Late));
        assert_eq!(book.closed(Interval::M1).len(), 1);
        assert_eq!(book.closed(Interval::M1)[0].high, 3380.0);
        assert_eq!(book.counters().late, 1);
    }

    #[test]
    fn a_resubscribe_snapshot_tick_is_not_counted_twice() {
        let book = book();
        book.on_tick(3380.0, T0, Some(1.0), NOW);
        let repeat = book.on_tick(3380.0, T0, Some(1.0), NOW);
        assert_eq!(repeat.dropped, Some(DropReason::Repeat));
        assert_eq!(book.forming(Interval::M1).unwrap().volume, 1.0);
    }

    #[test]
    fn tick_volume_ticks_equal_the_candle_volume() {
        let book = book();
        for step in 0..4 {
            book.on_tick(3380.0 + step as f64, T0 + step * 1_000, None, NOW);
        }
        let live = book.forming(Interval::M15).expect("forming 15m bar");
        let event = book.on_tick(3384.0, T0 + M15, None, NOW);
        let closed_bar = event.tick_volume_closed.expect("closed tick-volume bar");
        assert!(closed_bar.closed);
        assert_eq!(closed_bar.ticks as f64, live.volume);
        assert_eq!(closed_bar.interval, "15m");
        assert_eq!(closed_bar.symbol, SYMBOL);
        assert_eq!(
            closed_bar.up_ticks + closed_bar.down_ticks + closed_bar.flat_ticks,
            closed_bar.ticks
        );
        let live_bar = event.tick_volume.expect("live tick-volume bar");
        assert!(!live_bar.closed);
        assert_eq!(live_bar.time, T0 + M15);
    }

    #[test]
    fn unaligned_seed_rows_are_rejected_and_duplicates_merge_ascending() {
        let book = book();
        let mut misaligned = bar(Interval::M15, T0 + M15); // on the hour, fine for 15m
        misaligned.time = T0 + M15 + 1; // off the 15m grid
        let mut wrong_interval = bar(Interval::M15, T0 + M15);
        wrong_interval.interval = "1m".into();
        let mut invalid = bar(Interval::M15, T0 + M15);
        invalid.high = invalid.low - 1.0;

        let report = book.seed(
            Interval::M15,
            &[
                bar(Interval::M15, T0 + M15),
                bar(Interval::M15, T0),
                misaligned,
                wrong_interval,
                invalid,
                bar(Interval::M15, T0), // duplicate timestamp
            ],
        );
        assert_eq!(report.rejected_unaligned, 2);
        assert_eq!(report.rejected_invalid, 1);
        assert_eq!(report.accepted, 3);
        assert_eq!(report.merged, 1);
        let times: Vec<i64> = book.closed(Interval::M15).iter().map(|c| c.time).collect();
        assert_eq!(times, vec![T0, T0 + M15]);
    }

    #[test]
    fn a_seed_row_never_reopens_a_bar() {
        let book = book();
        let mut forming = bar(Interval::M1, T0);
        forming.closed = false;
        let report = book.seed(Interval::M1, &[forming]);
        assert_eq!(report.accepted, 1);
        assert!(
            book.closed(Interval::M1)[0].closed,
            "seed rows are always closed"
        );
    }

    #[test]
    fn a_full_series_evicts_the_oldest_bar_never_the_newest() {
        let mut series = Series::new(Interval::M15, 3);
        for step in 0..5 {
            series.push(bar(Interval::M15, T0 + step * M15));
        }
        let times: Vec<i64> = series.bars.iter().map(|b| b.time).collect();
        assert_eq!(times, vec![T0 + 2 * M15, T0 + 3 * M15, T0 + 4 * M15]);

        // ...and a late bar lands in its own slot instead of the tail.
        series.push(bar(Interval::M15, T0 + M15));
        let times: Vec<i64> = series.bars.iter().map(|b| b.time).collect();
        assert_eq!(times, vec![T0 + M15, T0 + 2 * M15, T0 + 3 * M15]);
    }

    #[test]
    fn the_book_reports_edges_and_retains_the_recent_tail() {
        let book = book();
        book.seed(
            Interval::H1,
            &[
                bar(Interval::H1, T0),
                bar(Interval::H1, T0 - H1),
                bar(Interval::H1, T0 - 2 * H1),
            ],
        );
        assert_eq!(book.origin(Interval::H1), Some(T0 - 2 * H1));
        assert_eq!(book.edge(Interval::H1), Some(T0));
        assert_eq!(book.recent(Interval::H1, 2).len(), 2);
        assert_eq!(book.recent(Interval::H1, 2)[1].time, T0);
        assert_eq!(book.recent(Interval::H1, 99).len(), 3);
    }

    #[test]
    fn counters_add_up() {
        let book = book();
        book.on_tick(0.0, T0, None, NOW);
        book.on_tick(3380.0, T0 / 1000, None, NOW);
        book.on_tick(3380.0, NOW + 60_000, None, NOW);
        book.on_tick(3380.0, T0, None, NOW);
        book.on_tick(3380.0, T0, None, NOW);
        book.on_tick(3380.0, T0 + M1, None, NOW);
        book.on_tick(3381.0, T0 + 1_000, None, NOW);
        let counters = book.counters();
        assert_eq!(counters.accepted, 2);
        assert_eq!(counters.dropped(), 5);
        let dropped_by_reason = counters.price
            + counters.seconds_timestamp
            + counters.future
            + counters.repeat
            + counters.late;
        assert_eq!(dropped_by_reason, 5);
    }
}
