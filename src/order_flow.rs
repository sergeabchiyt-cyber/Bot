use std::collections::{HashMap, VecDeque};
use chrono::Utc;

use crate::types::{AggTrade, OrderflowEvent};
use crate::volume_profile::VolumeProfileEngine;

const DEBOUNCE_MS: i64 = 2000;
const LOOKBACK: usize = 500;
const LEVEL_RADIUS: f64 = 10.0;
const SLIDING_WINDOW_MS: i64 = 300_000; // 5 minutes
const MIN_SAMPLES: usize = 50;

// Absolute floors in USD notional.
const FLOOR_BUBBLE: f64 = 50_000.0;   // $50k net delta over 500 trades
const FLOOR_ABSORPTION: f64 = 100_000.0; // $100k total volume over 500 trades
const ABSORPTION_DELTA_CAP: f64 = 20_000.0; // |net delta| must be < $20k to count as "near zero"

pub struct OrderFlowAnalyzer {
    pub delta_history: VecDeque<(i64, f64, f64, f64)>, // (time_ms, price, notional_delta, notional_volume)
    pub event_history: VecDeque<OrderflowEvent>,
    pub cumulative_delta: f64,
    pub window_size: usize,
    pub last_exchange: String,
    pub last_emit: HashMap<String, i64>,

    pub bubble_mags: VecDeque<(i64, f64)>,
    pub abs_vols: VecDeque<(i64, f64)>,
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
            bubble_mags: VecDeque::new(),
            abs_vols: VecDeque::new(),
        }
    }

    pub fn ingest(&mut self, trade: &AggTrade) {
        let price = trade.price_f64();
        let qty = trade.qty_f64();
        // Feeds without a taker side (quote ticks) must not fabricate
        // directional delta — they only count toward total volume.
        let signed_qty = if trade.has_flow_side { trade.signed_delta() } else { 0.0 };

        let notional_delta = signed_qty * price;
        let notional_volume = qty * price;
        
        self.cumulative_delta += notional_delta;
        self.last_exchange = trade.exchange.clone();
        self.delta_history.push_back((trade.trade_time, price, notional_delta, notional_volume));
        
        while self.delta_history.len() > self.window_size {
            self.delta_history.pop_front();
        }

        if self.delta_history.len() >= LOOKBACK {
            let net_delta: f64 = self.delta_history.iter().rev().take(LOOKBACK).map(|(_, _, d, _)| *d).sum();
            let total_vol: f64 = self.delta_history.iter().rev().take(LOOKBACK).map(|(_, _, _, v)| *v).sum();
            let t = self.delta_history.back().unwrap().0;

            self.bubble_mags.push_back((t, net_delta.abs()));
            
            if net_delta.abs() < ABSORPTION_DELTA_CAP {
                self.abs_vols.push_back((t, total_vol));
            }

            let cutoff = t - SLIDING_WINDOW_MS;
            while let Some(&front) = self.bubble_mags.front() {
                if front.0 < cutoff { self.bubble_mags.pop_front(); } else { break; }
            }
            while let Some(&front) = self.abs_vols.front() {
                if front.0 < cutoff { self.abs_vols.pop_front(); } else { break; }
            }
        }
    }

    pub fn recent_delta_notional(&self) -> f64 {
        let n = LOOKBACK.min(self.delta_history.len());
        self.delta_history.iter().rev().take(n).map(|(_, _, d, _)| *d).sum()
    }

    pub fn recent_total_volume(&self) -> f64 {
        let n = LOOKBACK.min(self.delta_history.len());
        self.delta_history.iter().rev().take(n).map(|(_, _, _, v)| *v).sum()
    }

    fn price_at(&self, lookback: usize) -> Option<f64> {
        if self.delta_history.len() < lookback {
            return None;
        }
        self.delta_history
            .iter()
            .rev()
            .nth(lookback - 1)
            .map(|(_, p, _, _)| *p)
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

    fn percentile(sorted: &[f64], p: f64) -> f64 {
        if sorted.is_empty() { return 0.0; }
        let idx = (((sorted.len() as f64) - 1.0) * p).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    fn adaptive_thresholds(&self, bubble_p: f64, absorption_p: f64) -> (f64, f64) {
        let mut b_mags: Vec<f64> = self.bubble_mags.iter().map(|(_, v)| *v).collect();
        b_mags.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mut a_vols: Vec<f64> = self.abs_vols.iter().map(|(_, v)| *v).collect();
        a_vols.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let bubble_thresh = if b_mags.len() >= MIN_SAMPLES {
            Self::percentile(&b_mags, bubble_p).max(FLOOR_BUBBLE)
        } else {
            FLOOR_BUBBLE
        };

        let abs_thresh = if a_vols.len() >= MIN_SAMPLES {
            Self::percentile(&a_vols, absorption_p).max(FLOOR_ABSORPTION)
        } else {
            FLOOR_ABSORPTION
        };

        (bubble_thresh, abs_thresh)
    }

    pub fn detect_events(
        &mut self,
        vp: &VolumeProfileEngine,
        current_price: f64,
        bubble_percentile: f64,
        absorption_percentile: f64,
    ) -> Vec<OrderflowEvent> {
        let now_ms = Utc::now().timestamp_millis();
        let net_delta = self.recent_delta_notional();
        let total_vol = self.recent_total_volume();

        let (bubble_threshold, absorption_threshold) =
            self.adaptive_thresholds(bubble_percentile, absorption_percentile);

        let mut events = Vec::new();

        if net_delta > bubble_threshold && self.should_emit("BUY_BUBBLE", now_ms) {
            events.push(OrderflowEvent {
                kind: "BUY_BUBBLE".into(),
                level: current_price,
                strength: net_delta,
                timestamp: now_ms,
                exchange: self.last_exchange.clone(),
            });
        }

        if net_delta < -bubble_threshold && self.should_emit("SELL_BUBBLE", now_ms) {
            events.push(OrderflowEvent {
                kind: "SELL_BUBBLE".into(),
                level: current_price,
                strength: net_delta.abs(),
                timestamp: now_ms,
                exchange: self.last_exchange.clone(),
            });
        }

        if let Some(start_price) = self.price_at(LOOKBACK) {
            let displacement = current_price - start_price;

            if total_vol > absorption_threshold 
                && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0 
                && displacement >= -0.5 
                && self.should_emit("ABS_BUY", now_ms)
            {
                let reference = Self::nearest_level(vp, current_price)
                    .unwrap_or(current_price);
                events.push(OrderflowEvent {
                    kind: "ABS_BUY".into(),
                    level: reference,
                    strength: total_vol,
                    timestamp: now_ms,
                    exchange: self.last_exchange.clone(),
                });
            }

            if total_vol > absorption_threshold 
                && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0
                && displacement <= 0.5 
                && self.should_emit("ABS_SELL", now_ms)
            {
                let reference = Self::nearest_level(vp, current_price)
                    .unwrap_or(current_price);
                events.push(OrderflowEvent {
                    kind: "ABS_SELL".into(),
                    level: reference,
                    strength: total_vol,
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