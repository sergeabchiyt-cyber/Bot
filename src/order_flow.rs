use std::collections::VecDeque;

use crate::types::{AbsorptionBubble, AggTrade};
use crate::volume_profile::VolumeProfileEngine;

pub struct OrderFlowAnalyzer {
    pub delta_history: VecDeque<(i64, f64, f64)>,
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

    pub fn detect_absorption(
        &mut self,
        vp: &VolumeProfileEngine,
        current_price: f64,
        delta_threshold: f64,
    ) -> Option<AbsorptionBubble> {
        let recent = self.recent_delta(500);
        if recent.abs() < delta_threshold {
            return None;
        }

        for level in vp.all_levels() {
            let near_poc = (current_price - level.poc).abs() <= 1.0;
            let near_vah = (current_price - level.vah).abs() <= 1.0;
            let near_val = (current_price - level.val).abs() <= 1.0;

            if !(near_poc || near_vah || near_val) {
                continue;
            }

            let reference = if near_vah {
                level.vah
            } else if near_val {
                level.val
            } else {
                level.poc
            };

            if recent < -delta_threshold && current_price >= reference {
                let bubble = AbsorptionBubble {
                    level: reference,
                    direction: "ABS_BUY".into(),
                    strength: recent.abs(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                self.push_bubble(bubble.clone());
                return Some(bubble);
            }

            if recent > delta_threshold && current_price <= reference {
                let bubble = AbsorptionBubble {
                    level: reference,
                    direction: "ABS_SELL".into(),
                    strength: recent,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };
                self.push_bubble(bubble.clone());
                return Some(bubble);
            }
        }

        None
    }

    fn push_bubble(&mut self, bubble: AbsorptionBubble) {
        self.bubble_history.push_back(bubble);
        while self.bubble_history.len() > 200 {
            self.bubble_history.pop_front();
        }
    }
}