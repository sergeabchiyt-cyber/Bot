use std::collections::{HashMap, VecDeque};
use chrono::Utc;

use crate::types::{AggTrade, OrderflowEvent};
use crate::volume_profile::VolumeProfileEngine;

const DEBOUNCE_MS: i64 = 2000;
const LOOKBACK: usize = 500;
const LEVEL_RADIUS: f64 = 10.0;
const MAG_BUFFER_SIZE: usize = 5000;
const MIN_SAMPLES: usize = 300;

// Absolute floors. Used during cold start and to prevent triggering on
// numerically tiny deltas in dead markets.
const FLOOR_BUBBLE: f64 = 8.0;
const FLOOR_ABSORPTION: f64 = 20.0;

pub struct OrderFlowAnalyzer {
    /// (timestamp, price, signed_delta) rolling buffer
    pub delta_history: VecDeque<(i64, f64, f64)>,
    /// Rolling history of |recent_delta| snapshots — the adaptive baseline
    pub magnitudes: VecDeque<f64>,
    pub event_history: VecDeque<OrderflowEvent>,
    pub cumulative_delta: f64,
    pub window_size: usize,
    pub last_exchange: String,
    pub last_emit: HashMap<String, i64>,
}

impl OrderFlowAnalyzer {
    pub fn new(window: usize) -> Self {
        Self {
            delta_history: VecDeque::with_capacity(window),
            magnitudes: VecDeque::with_capacity(MAG_BUFFER_SIZE),
            event_history: VecDeque::with_capacity(500),
            cumulative_delta: 0.0,
            window_size: window,
            last_exchange: "binance".into(),
            last_emit: HashMap::new(),
        }
    }

    pub fn ingest(&mut self, trade: &AggTrade) {
        let delta = trade.signed_delta();
        self.cumulative_delta += delta;
        self.last_exchange = trade.exchange.clone();
        self.delta_history
            .push_back((trade.trade_time, trade.price_f64(), delta));
        while self.delta_history.len() > self.window_size {
            self.delta_history.pop_front();
        }
    }

    pub fn recent_delta(&self, lookback: usize) -> f64 {
        let n = lookback.min(self.delta_history.len());
        self.delta_history
            .iter()
            .rev()
            .take(n)
            .map(|(_, _, d)| *d)
            .sum()
    }

    fn price_at(&self, lookback: usize) -> Option<f64> {
        if self.delta_history.len() < lookback {
            return None;
        }
        self.delta_history
            .iter()
            .rev()
            .nth(lookback - 1)
            .map(|(_, p, _)| *p)
    }

    fn nearest_level(vp: &VolumeProfileEngine, price: f64) -> Option<f64> {
        vp.all_levels()
            .iter()
            .flat_map(|l| [l.poc, l.vah, l.val])
            .filter(|l| (l - price).abs() <= LEVEL_RADIUS)
            .min_by(|a, b| {
                (a - price)
                    .abs()
                    .partial_cmp(&(b - price).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    fn should_emit(&mut self, kind: &str, now_ms: i64) -> bool {
        if let Some(&last) = self.last_emit.get(kind) {
            if now_ms - last < DEBOUNCE_MS {
                return false;
            }
        }
        self.last_emit.insert(kind.to_string(), now_ms);
        true
    }

    /// Percentile-based adaptive thresholds.
    /// `bubble_p` and `absorption_p` are in (0.0, 1.0) — e.g. 0.90 and 0.97.
    /// Falls back to the fixed floors until MIN_SAMPLES snapshots are available.
    fn adaptive_thresholds(&self, bubble_p: f64, absorption_p: f64) -> (f64, f64) {
        if self.magnitudes.len() < MIN_SAMPLES {
            return (FLOOR_BUBBLE, FLOOR_ABSORPTION);
        }
        let mut sorted: Vec<f64> = self.magnitudes.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();

        let bubble_idx = (((n as f64) - 1.0) * bubble_p).round() as usize;
        let abs_idx = (((n as f64) - 1.0) * absorption_p).round() as usize;

        let bubble = sorted[bubble_idx.min(n - 1)].max(FLOOR_BUBBLE);
        let absorption = sorted[abs_idx.min(n - 1)].max(FLOOR_ABSORPTION);

        (bubble, absorption)
    }

    pub fn detect_events(
        &mut self,
        vp: &VolumeProfileEngine,
        current_price: f64,
        bubble_percentile: f64,
        absorption_percentile: f64,
    ) -> Vec<OrderflowEvent> {
        let now_ms = Utc::now().timestamp_millis();
        let recent = self.recent_delta(LOOKBACK);
        let mag = recent.abs();

        // Thresholds computed BEFORE inserting the current snapshot, so the
        // current tick does not influence the baseline it is compared against.
        let (bubble_threshold, absorption_threshold) =
            self.adaptive_thresholds(bubble_percentile, absorption_percentile);

        // Update rolling baseline
        self.magnitudes.push_back(mag);
        while self.magnitudes.len() > MAG_BUFFER_SIZE {
            self.magnitudes.pop_front();
        }

        let mut events = Vec::new();

        // ----- Buy bubble -----
        if recent > bubble_threshold && self.should_emit("BUY_BUBBLE", now_ms) {
            events.push(OrderflowEvent {
                kind: "BUY_BUBBLE".into(),
                level: current_price,
                strength: recent,
                timestamp: now_ms,
                exchange: self.last_exchange.clone(),
            });
        }

        // ----- Sell bubble -----
        if recent < -bubble_threshold && self.should_emit("SELL_BUBBLE", now_ms) {
            events.push(OrderflowEvent {
                kind: "SELL_BUBBLE".into(),
                level: current_price,
                strength: recent.abs(),
                timestamp: now_ms,
                exchange: self.last_exchange.clone(),
            });
        }

        // ----- Absorption -----
        if let Some(start_price) = self.price_at(LOOKBACK) {
            let displacement = current_price - start_price;

            if recent < -absorption_threshold
                && displacement >= -0.5
                && self.should_emit("ABS_BUY", now_ms)
            {
                let reference = Self::nearest_level(vp, current_price)
                    .unwrap_or(current_price);
                events.push(OrderflowEvent {
                    kind: "ABS_BUY".into(),
                    level: reference,
                    strength: recent.abs(),
                    timestamp: now_ms,
                    exchange: self.last_exchange.clone(),
                });
            }

            if recent > absorption_threshold
                && displacement <= 0.5
                && self.should_emit("ABS_SELL", now_ms)
            {
                let reference = Self::nearest_level(vp, current_price)
                    .unwrap_or(current_price);
                events.push(OrderflowEvent {
                    kind: "ABS_SELL".into(),
                    level: reference,
                    strength: recent,
                    timestamp: now_ms,
                    exchange: self.last_exchange.clone(),
                });
            }
        }

        for e in &events {
            self.event_history.push_back(e.clone());
        }
        while self.event_history.len() > 500 {
            self.event_history.pop_front();
        }

        events
    }
}