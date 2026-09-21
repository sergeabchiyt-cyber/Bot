use std::collections::VecDeque;
use crate::types::{AbsorptionBubble, AggTrade};
use crate::volume_profile::VolumeProfileEngine;

pub struct OrderFlowAnalyzer {
    /// Rolling signed delta (buy - sell) per price bin
    pub delta_history: VecDeque<(i64, f64, f64)>, // (ts, price, delta)
    pub bubble_history: VecDeque<AbsorptionBubble>,
    pub cumulative_delta: f64,
    pub window_size: usize,
}

impl OrderFlowAnalyzer {
    pub fn new(window: usize) -> Self {
        Self {
            delta_history: VecDeque::with_capacity(window),
            bubble_history: VecDeque::with_capacity(200),
            cumulative_delta: 0.0,
            window_size: window,
        }
    }

    pub fn ingest(&mut self, trade: &AggTrade) {
        let delta = trade.signed_delta();
        self.cumulative_delta += delta;
        self.delta_history.push_back((trade.trade_time, trade.price_f64(), delta));
        while self.delta_history.len() > self.window_size {
            self.delta_history.pop_front();
        }
    }

    /// Detect absorption at a given VP level.
    /// ABS_BUY: heavy negative delta but price holds at/above the level
    /// ABS_SELL: heavy positive delta but price rejects from/below the level
    pub fn detect_absorption(
        &mut self,
        vp: &VolumeProfileEngine,
        current_price: f64,
        delta_threshold: f64,
    ) -> Option<AbsorptionBubble> {
        let recent: f64 = self.delta_history.iter().map(|(_, _, d)| d).sum();
        if recent.abs() < delta_threshold {
            return None;
        }
        for level in vp.all_levels() {
            let dist = (current_price - level.poc).abs();
            if dist > 1.0 { continue; } // within $1 of POC
            if recent < -delta_threshold && current_price >= level.val {
                let bubble = AbsorptionBubble {
                    level: level.poc,
                    direction: "ABS_BUY".into(),
                    strength: recent.abs(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                self.bubble_history.push_back(bubble.clone());
                while self.bubble_history.len() > 200 { self.bubble_history.pop_front(); }
                return Some(bubble);
            }
            if recent > delta_threshold && current_price <= level.vah {
                let bubble = AbsorptionBubble {
                    level: level.poc,
                    direction: "ABS_SELL".into(),
                    strength: recent,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                self.bubble_history.push_back(bubble.clone());
                while self.bubble_history.len() > 200 { self.bubble_history.pop_front(); }
                return Some(bubble);
            }
        }
        None
    }
}