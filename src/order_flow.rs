use std::collections::{HashMap, VecDeque};
use chrono::Utc;

use crate::types::{AggTrade, OrderflowEvent};
use crate::volume_profile::VolumeProfileEngine;

const DEBOUNCE_MS: i64 = 2000;
const ABSORPTION_LOOKBACK: usize = 500;
const LEVEL_REFERENCE_RADIUS: f64 = 10.0;

pub struct OrderFlowAnalyzer {
    pub delta_history: VecDeque<(i64, f64, f64)>,
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

    /// Price `lookback` trades ago. Used to measure price displacement.
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
            .filter(|l| (l - price).abs() <= LEVEL_REFERENCE_RADIUS)
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

    /// Runs on every ingested tick. Returns zero or more events.
    /// `bubble_threshold` triggers buy/sell bubbles.
    /// `absorption_threshold` triggers absorption (higher bar).
    pub fn detect_events(
        &mut self,
        vp: &VolumeProfileEngine,
        current_price: f64,
        bubble_threshold: f64,
        absorption_threshold: f64,
    ) -> Vec<OrderflowEvent> {
        let now_ms = Utc::now().timestamp_millis();
        let recent = self.recent_delta(ABSORPTION_LOOKBACK);
        let mut events = Vec::new();

        // ----- 1. Buy bubble: net aggressive buying -----
        if recent > bubble_threshold && self.should_emit("BUY_BUBBLE", now_ms) {
            events.push(OrderflowEvent {
                kind: "BUY_BUBBLE".into(),
                level: current_price,
                strength: recent,
                timestamp: now_ms,
                exchange: self.last_exchange.clone(),
            });
        }

        // ----- 2. Sell bubble: net aggressive selling -----
        if recent < -bubble_threshold && self.should_emit("SELL_BUBBLE", now_ms) {
            events.push(OrderflowEvent {
                kind: "SELL_BUBBLE".into(),
                level: current_price,
                strength: recent.abs(),
                timestamp: now_ms,
                exchange: self.last_exchange.clone(),
            });
        }

        // ----- 3/4. Absorption: heavy opposing delta, price holds -----
        if let Some(start_price) = self.price_at(ABSORPTION_LOOKBACK) {
            let displacement = current_price - start_price;

            // ABS_BUY: heavy selling, but price didn't fall
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

            // ABS_SELL: heavy buying, but price didn't rise
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