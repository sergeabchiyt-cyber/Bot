//! The approved XAUUSD strategy, ported verbatim from Node 3.
//!
//! Source of truth: `origin/Node3 @ fff6bfd`, `node3-strategy/src/strategy.rs`
//! and `node3-strategy/src/risk.rs`. This module is a deterministic port, not a
//! new model:
//!
//! * levels: `PW` PoC/VaH/VaL, `PS` PoC, `CW` PoC/VaH/VaL;
//! * break: a green candle that opens at/below a level and closes above it, or
//!   a red candle that opens at/above and closes below;
//! * retest: within ±$0.50 of the broken level, confirmed only when the
//!   candle's tick volume reaches the configured threshold (10,500);
//! * invalidation: a close $2.00 beyond the level resets the setup;
//! * one trigger per candle, levels visited in sorted ID order;
//! * risk: ATR-derived SL/TP in pips, clamped to the configured RR band.
//!
//! Everything here is pure: no clock, no network, no broker adapter. The
//! runner supplies a timestamp and receives an immutable [`TradeIntent`].

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::intent::TradeIntent;

/// Strategy identity written into every intent. Bump this whenever a parameter
/// or the rules change: it is part of the deterministic `intent_id`, so a
/// changed strategy must not reuse an old event's key.
pub const STRATEGY_VERSION: &str = "vp_break_retest_v1";

/// Retest proximity around a broken level, in price units.
pub const PROXIMITY_DOLLARS: f64 = 0.50;
/// A close this far beyond a broken level invalidates the setup.
pub const INVALIDATION_DOLLARS: f64 = 2.00;
/// Closed candles kept for ATR (Node 3 used the same window).
pub const HISTORY_LIMIT: usize = 30;

/// One closed 15m OHLCV bar. `volume` is the tick count, exactly as Node 1
/// defines it and Node 3 consumed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Candle {
    /// Structural validity. The feed validates the same fields; repeating the
    /// check here keeps the strategy safe if it ever gets another input.
    pub fn is_valid(&self) -> bool {
        self.time > 0
            && self.open.is_finite()
            && self.high.is_finite()
            && self.low.is_finite()
            && self.close.is_finite()
            && self.volume.is_finite()
            && self.open > 0.0
            && self.low > 0.0
            && self.close > 0.0
            && self.volume >= 0.0
            && self.high >= self.open.max(self.close)
            && self.low <= self.open.min(self.close)
    }
}

/// A volume-profile level as consumed by the strategy. Only the identity and
/// the three prices matter; the rest is provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub window: String,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    #[allow(dead_code)]
    #[serde(default)]
    pub start: i64,
    #[allow(dead_code)]
    #[serde(default)]
    pub end: i64,
    #[serde(default)]
    pub timestamp: i64,
}

/// A confirmed setup. `source_candle_time` is the bucket-open time of the
/// **closed** candle that triggered it, which is also the intent provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub symbol: String,
    pub interval: String,
    pub side: String,
    pub level_id: String,
    pub level_name: String,
    pub level_price: f64,
    pub reference_price: f64,
    pub source_candle_time: i64,
}

/// One evaluation of the last observed candle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDecision {
    pub evaluated_at_ms: i64,
    pub symbol: String,
    pub interval: String,
    /// `buy`, `sell` or `flat`.
    pub action: String,
    pub trigger: Option<Trigger>,
    pub atr: f64,
    pub levels_tracked: usize,
    pub breaks_detected: u64,
    pub breaks_invalidated: u64,
    pub retests_rejected_low_volume: u64,
    pub signals_confirmed: u64,
    pub events: Vec<String>,
}

impl StrategyDecision {
    fn flat(symbol: &str, interval: &str, now_ms: i64, atr: f64, levels: usize) -> Self {
        Self {
            evaluated_at_ms: now_ms,
            symbol: symbol.to_string(),
            interval: interval.to_string(),
            action: "flat".into(),
            trigger: None,
            atr,
            levels_tracked: levels,
            breaks_detected: 0,
            breaks_invalidated: 0,
            retests_rejected_low_volume: 0,
            signals_confirmed: 0,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakState {
    None,
    BrokenAbove,
    BrokenBelow,
}

#[derive(Debug, Clone)]
struct LevelTarget {
    name: String,
    price: f64,
}

/// The state machine. Deterministic: `observe_closed` then `evaluate_last`.
pub struct StrategyEngine {
    levels: BTreeMap<String, LevelTarget>,
    break_states: BTreeMap<String, BreakState>,
    history: VecDeque<Candle>,
    last_triggered_candle: i64,
    last_observed_candle: i64,
    atr_fallback: f64,
    breaks_detected: u64,
    breaks_invalidated: u64,
    retests_rejected_low_volume: u64,
    signals_confirmed: u64,
}

impl StrategyEngine {
    pub fn new(atr_fallback: f64) -> Self {
        Self {
            levels: BTreeMap::new(),
            break_states: BTreeMap::new(),
            history: VecDeque::with_capacity(HISTORY_LIMIT),
            last_triggered_candle: 0,
            last_observed_candle: 0,
            atr_fallback: if atr_fallback.is_finite() && atr_fallback > 0.0 {
                atr_fallback
            } else {
                3.0
            },
            breaks_detected: 0,
            breaks_invalidated: 0,
            retests_rejected_low_volume: 0,
            signals_confirmed: 0,
        }
    }

    /// Upsert the three prices of one profile window. Unknown windows (swing
    /// profiles, calendar windows) are ignored exactly as Node 3 ignored them.
    pub fn update_levels(&mut self, level: &Level) {
        match level.window.as_str() {
            "PW" => {
                self.upsert_level("PW_POC", "PW PoC", level.poc);
                self.upsert_level("PW_VAH", "PW VaH", level.vah);
                self.upsert_level("PW_VAL", "PW VaL", level.val);
            }
            "PS" => {
                self.upsert_level("PS_POC", "PS PoC", level.poc);
            }
            "CW" => {
                self.upsert_level("CW_POC", "CW PoC", level.poc);
                self.upsert_level("CW_VAH", "CW VaH", level.vah);
                self.upsert_level("CW_VAL", "CW VaL", level.val);
            }
            _ => {}
        }
    }

    fn upsert_level(&mut self, id: &str, name: &str, price: f64) {
        if !price.is_finite() || price <= 0.0 {
            return;
        }
        self.levels.insert(
            id.to_string(),
            LevelTarget {
                name: name.to_string(),
                price,
            },
        );
    }

    /// Observe one closed candle. Returns `false` for a duplicate or an older
    /// bar, which must not be evaluated a second time.
    pub fn observe_closed(&mut self, candle: &Candle) -> bool {
        if !candle.is_valid() {
            return false;
        }
        if candle.time <= self.last_observed_candle {
            return false;
        }
        self.last_observed_candle = candle.time;
        self.history.push_back(candle.clone());
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        true
    }

    /// ATR over the observed closed candles: the mean true range, exactly the
    /// Node 3 calculation (not Wilder's smoothing).
    pub fn calculate_atr(&self) -> f64 {
        if self.history.len() < 2 {
            return self.atr_fallback;
        }
        let candles: Vec<&Candle> = self.history.iter().collect();
        let mut tr_sum = 0.0;
        let mut count = 0usize;
        for pair in candles.windows(2) {
            let prev = pair[0];
            let curr = pair[1];
            let hl = curr.high - curr.low;
            let hc = (curr.high - prev.close).abs();
            let lc = (curr.low - prev.close).abs();
            tr_sum += hl.max(hc).max(lc);
            count += 1;
        }
        if count > 0 {
            tr_sum / count as f64
        } else {
            self.atr_fallback
        }
    }

    /// Evaluate the most recently observed candle. Must be called exactly once
    /// per accepted closed candle; a replay that is not evaluated cannot
    /// trigger a trade.
    pub fn evaluate_last(
        &mut self,
        symbol: &str,
        interval: &str,
        volume_threshold: f64,
        now_ms: i64,
    ) -> StrategyDecision {
        let atr = self.calculate_atr();
        let mut decision = StrategyDecision::flat(symbol, interval, now_ms, atr, self.levels.len());

        let Some(candle) = self.history.back().cloned() else {
            decision.events.push("no closed candle".into());
            return decision;
        };

        // Node 3's guard: a candle at or before the last trigger is skipped.
        if candle.time <= self.last_triggered_candle {
            return decision;
        }

        let price = candle.close;
        let is_green = candle.close > candle.open;
        let is_red = candle.close < candle.open;
        let vol_ok = candle.volume >= volume_threshold;

        // `BTreeMap` iteration is the deterministic sorted-by-id order Node 3
        // built explicitly, so the first confirmed setup wins identically.
        let level_ids: Vec<String> = self.levels.keys().cloned().collect();
        for id in level_ids {
            let Some(target) = self.levels.get(&id).cloned() else {
                continue;
            };
            let lvl_price = target.price;
            let state = self
                .break_states
                .get(&id)
                .copied()
                .unwrap_or(BreakState::None);

            match state {
                BreakState::None => {
                    if is_green && candle.close > lvl_price && candle.open <= lvl_price {
                        self.break_states
                            .insert(id.clone(), BreakState::BrokenAbove);
                        self.breaks_detected += 1;
                        decision.breaks_detected += 1;
                        decision.events.push(format!(
                            "{}: broken ABOVE @ {:.2} — awaiting bullish retest of {:.2} (±${:.2})",
                            target.name, candle.close, lvl_price, PROXIMITY_DOLLARS
                        ));
                    } else if is_red && candle.close < lvl_price && candle.open >= lvl_price {
                        self.break_states
                            .insert(id.clone(), BreakState::BrokenBelow);
                        self.breaks_detected += 1;
                        decision.breaks_detected += 1;
                        decision.events.push(format!(
                            "{}: broken BELOW @ {:.2} — awaiting bearish retest of {:.2} (±${:.2})",
                            target.name, candle.close, lvl_price, PROXIMITY_DOLLARS
                        ));
                    }
                }
                BreakState::BrokenAbove => {
                    let retested = candle.low <= lvl_price + PROXIMITY_DOLLARS
                        && candle.close >= lvl_price - PROXIMITY_DOLLARS;
                    if retested {
                        if vol_ok {
                            self.break_states.insert(id.clone(), BreakState::None);
                            self.last_triggered_candle = candle.time;
                            self.signals_confirmed += 1;
                            decision.signals_confirmed += 1;
                            decision.action = "buy".into();
                            decision.trigger = Some(Trigger {
                                symbol: symbol.to_string(),
                                interval: interval.to_string(),
                                side: "buy".into(),
                                level_id: id.clone(),
                                level_name: target.name.clone(),
                                level_price: lvl_price,
                                reference_price: price,
                                source_candle_time: candle.time,
                            });
                            decision.events.push(format!(
                                "{}: BULLISH RETEST CONFIRMED @ {:.2} (vol {:.0} >= {:.0}) — \
                                 triggering BUY",
                                target.name, price, candle.volume, volume_threshold
                            ));
                            return decision;
                        }
                        self.retests_rejected_low_volume += 1;
                        decision.retests_rejected_low_volume += 1;
                        decision.events.push(format!(
                            "{}: retested {:.2} (low {:.2}) but volume {:.0} < {:.0} threshold — \
                             waiting",
                            target.name, lvl_price, candle.low, candle.volume, volume_threshold
                        ));
                    } else if candle.close < lvl_price - INVALIDATION_DOLLARS {
                        self.break_states.insert(id.clone(), BreakState::None);
                        self.breaks_invalidated += 1;
                        decision.breaks_invalidated += 1;
                        decision.events.push(format!(
                            "{}: bullish break invalidated (close {:.2} < {:.2})",
                            target.name,
                            candle.close,
                            lvl_price - INVALIDATION_DOLLARS
                        ));
                    }
                }
                BreakState::BrokenBelow => {
                    let retested = candle.high >= lvl_price - PROXIMITY_DOLLARS
                        && candle.close <= lvl_price + PROXIMITY_DOLLARS;
                    if retested {
                        if vol_ok {
                            self.break_states.insert(id.clone(), BreakState::None);
                            self.last_triggered_candle = candle.time;
                            self.signals_confirmed += 1;
                            decision.signals_confirmed += 1;
                            decision.action = "sell".into();
                            decision.trigger = Some(Trigger {
                                symbol: symbol.to_string(),
                                interval: interval.to_string(),
                                side: "sell".into(),
                                level_id: id.clone(),
                                level_name: target.name.clone(),
                                level_price: lvl_price,
                                reference_price: price,
                                source_candle_time: candle.time,
                            });
                            decision.events.push(format!(
                                "{}: BEARISH RETEST CONFIRMED @ {:.2} (vol {:.0} >= {:.0}) — \
                                 triggering SELL",
                                target.name, price, candle.volume, volume_threshold
                            ));
                            return decision;
                        }
                        self.retests_rejected_low_volume += 1;
                        decision.retests_rejected_low_volume += 1;
                        decision.events.push(format!(
                            "{}: retested {:.2} (high {:.2}) but volume {:.0} < {:.0} threshold — \
                             waiting",
                            target.name, lvl_price, candle.high, candle.volume, volume_threshold
                        ));
                    } else if candle.close > lvl_price + INVALIDATION_DOLLARS {
                        self.break_states.insert(id.clone(), BreakState::None);
                        self.breaks_invalidated += 1;
                        decision.breaks_invalidated += 1;
                        decision.events.push(format!(
                            "{}: bearish break invalidated (close {:.2} > {:.2})",
                            target.name,
                            candle.close,
                            lvl_price + INVALIDATION_DOLLARS
                        ));
                    }
                }
            }
        }

        decision
    }

    pub fn levels_tracked(&self) -> usize {
        self.levels.len()
    }

    #[cfg(test)]
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    #[cfg(test)]
    pub fn last_triggered_candle(&self) -> i64 {
        self.last_triggered_candle
    }

    pub fn counters(&self) -> (u64, u64, u64, u64) {
        (
            self.breaks_detected,
            self.breaks_invalidated,
            self.retests_rejected_low_volume,
            self.signals_confirmed,
        )
    }
}

/// The exact Node 3 risk projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskParams {
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub atr_fallback: f64,
}

impl Default for RiskParams {
    fn default() -> Self {
        Self {
            sl_min_pips: 200.0,
            sl_max_pips: 300.0,
            tp_min_pips: 600.0,
            tp_max_pips: 800.0,
            rr_min: 2.0,
            rr_max: 3.0,
            atr_fallback: 3.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiskProjector {
    params: RiskParams,
    atr: f64,
}

impl RiskProjector {
    pub fn new(params: RiskParams) -> Self {
        let atr = if params.atr_fallback.is_finite() && params.atr_fallback > 0.0 {
            params.atr_fallback
        } else {
            3.0
        };
        Self { params, atr }
    }

    pub fn update_atr(&mut self, atr: f64) {
        if atr.is_finite() && atr > 0.0 {
            self.atr = atr;
        }
    }

    /// `(stop_loss, take_profit, sl_pips, tp_pips, effective_rr)`.
    pub fn compute_details(&self, entry: f64, side: &str) -> (f64, f64, f64, f64, f64) {
        let safe_entry = if entry.is_finite() && entry > 0.0 {
            entry
        } else {
            2_650.0
        };
        let atr_pct = (self.atr / safe_entry).clamp(0.001, 0.05);
        let sl_pips = self.params.sl_min_pips
            + (self.params.sl_max_pips - self.params.sl_min_pips) * (atr_pct * 20.0).min(1.0);
        let requested_tp_pips = self.params.tp_min_pips
            + (self.params.tp_max_pips - self.params.tp_min_pips) * (atr_pct * 20.0).min(1.0);

        let sl_distance = sl_pips * 0.01;
        let mut tp_distance = requested_tp_pips * 0.01;
        let raw_rr = tp_distance / sl_distance.max(0.0001);
        if raw_rr < self.params.rr_min {
            tp_distance = sl_distance * self.params.rr_min;
        } else if raw_rr > self.params.rr_max {
            tp_distance = sl_distance * self.params.rr_max;
        }

        let effective_tp_pips = tp_distance * 100.0;
        let effective_rr = tp_distance / sl_distance.max(0.0001);
        let (stop_loss, take_profit) = match side {
            "sell" => (safe_entry + sl_distance, safe_entry - tp_distance),
            _ => (safe_entry - sl_distance, safe_entry + tp_distance),
        };

        (
            stop_loss,
            take_profit,
            sl_pips,
            effective_tp_pips,
            effective_rr,
        )
    }
}

/// Lowercase ASCII slug used in the deterministic intent ID. The execution
/// protocol only allows alphanumerics, `-`, `_`, `.` and `:` in an ID.
pub fn intent_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Deterministic, replay-safe intent ID:
/// `n4-<strategy version>-<symbol>-<timeframe>-<source candle>-<side>-<level>`.
///
/// The source candle is the closed bar's bucket-open epoch-ms, so a replay of
/// the same event produces the same ID regardless of wall-clock time. Two
/// different levels triggering on the same candle are distinct IDs; the
/// strategy itself only emits the first (sorted) one, exactly like Node 3.
pub fn intent_id(trigger: &Trigger) -> String {
    format!(
        "n4-{}-{}-{}-{}-{}-{}",
        intent_component(STRATEGY_VERSION),
        intent_component(&trigger.symbol),
        intent_component(&trigger.interval),
        trigger.source_candle_time,
        intent_component(&trigger.side),
        intent_component(&trigger.level_id),
    )
}

/// Build the complete immutable intent. `created_at` is supplied by the
/// caller; `expires_at` is the Node 3 TTL rule (default 120 s).
pub fn build_intent(
    trigger: &Trigger,
    risk: &RiskProjector,
    ttl_secs: u64,
    created_at: i64,
) -> TradeIntent {
    let (stop_loss, take_profit, _, _, risk_reward) =
        risk.compute_details(trigger.reference_price, &trigger.side);
    let ttl_ms = (ttl_secs.min(3_600) as i64).saturating_mul(1_000);
    TradeIntent {
        schema_version: crate::intent::INTENT_SCHEMA_VERSION,
        intent_id: intent_id(trigger),
        strategy: STRATEGY_VERSION.into(),
        symbol: trigger.symbol.clone(),
        side: trigger.side.clone(),
        order_type: "market".into(),
        reference_price: trigger.reference_price,
        stop_loss,
        take_profit,
        risk_reward,
        level_name: trigger.level_name.clone(),
        source_candle_time: trigger.source_candle_time,
        created_at,
        expires_at: created_at.saturating_add(ttl_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(window: &str, poc: f64, vah: f64, val: f64) -> Level {
        Level {
            window: window.into(),
            poc,
            vah,
            val,
            start: 0,
            end: 0,
            timestamp: 1,
        }
    }

    fn candle(time: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle {
            time,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    #[test]
    fn a_clean_break_and_high_volume_retest_triggers_a_buy() {
        let mut engine = StrategyEngine::new(3.0);
        engine.update_levels(&level("PW", 2_650.0, 2_660.0, 2_640.0));

        // Break above PW PoC.
        let c1 = candle(1_000, 2_649.0, 2_652.0, 2_648.5, 2_651.5, 8_000.0);
        assert!(engine.observe_closed(&c1));
        let d1 = engine.evaluate_last("XAUUSD", "15m", 10_500.0, 1_100);
        assert!(d1.trigger.is_none());
        assert_eq!(d1.breaks_detected, 1);

        // Low-volume retest stays armed.
        let c2 = candle(2_000, 2_651.5, 2_651.8, 2_650.1, 2_650.4, 9_000.0);
        engine.observe_closed(&c2);
        let d2 = engine.evaluate_last("XAUUSD", "15m", 10_500.0, 2_100);
        assert!(d2.trigger.is_none());
        assert_eq!(d2.retests_rejected_low_volume, 1);

        // Confirmed retest.
        let c3 = candle(3_000, 2_650.4, 2_651.2, 2_649.9, 2_650.3, 12_000.0);
        engine.observe_closed(&c3);
        let d3 = engine.evaluate_last("XAUUSD", "15m", 10_500.0, 3_100);
        let trigger = d3.trigger.expect("buy trigger");
        assert_eq!(d3.action, "buy");
        assert_eq!(trigger.side, "buy");
        assert_eq!(trigger.level_id, "PW_POC");
        assert_eq!(trigger.level_name, "PW PoC");
        assert_eq!(trigger.reference_price, 2_650.3);
        assert_eq!(trigger.source_candle_time, 3_000);
        assert_eq!(engine.last_triggered_candle(), 3_000);
    }

    #[test]
    fn a_break_below_triggers_a_sell_after_a_retest() {
        let mut engine = StrategyEngine::new(3.0);
        engine.update_levels(&level("CW", 2_650.0, 2_660.0, 2_640.0));

        let c1 = candle(1_000, 2_651.0, 2_651.5, 2_648.0, 2_648.5, 9_000.0);
        engine.observe_closed(&c1);
        let d1 = engine.evaluate_last("XAUUSD", "15m", 10_500.0, 1_100);
        assert_eq!(d1.breaks_detected, 1);

        let c2 = candle(2_000, 2_648.5, 2_650.3, 2_648.4, 2_649.9, 12_000.0);
        engine.observe_closed(&c2);
        let d2 = engine.evaluate_last("XAUUSD", "15m", 10_500.0, 2_100);
        let trigger = d2.trigger.expect("sell trigger");
        assert_eq!(d2.action, "sell");
        assert_eq!(trigger.level_id, "CW_POC");
    }

    #[test]
    fn a_close_beyond_the_invalidation_distance_resets_the_break() {
        let mut engine = StrategyEngine::new(3.0);
        engine.update_levels(&level("PW", 2_650.0, 2_660.0, 2_640.0));

        let c1 = candle(1_000, 2_649.0, 2_652.0, 2_648.5, 2_651.5, 8_000.0);
        engine.observe_closed(&c1);
        engine.evaluate_last("XAUUSD", "15m", 10_500.0, 1_100);

        let c2 = candle(2_000, 2_651.5, 2_651.6, 2_647.0, 2_647.5, 12_000.0);
        engine.observe_closed(&c2);
        let d2 = engine.evaluate_last("XAUUSD", "15m", 10_500.0, 2_100);
        assert!(d2.trigger.is_none());
        assert_eq!(d2.breaks_invalidated, 1);
    }

    #[test]
    fn duplicate_and_out_of_order_candles_are_never_evaluated() {
        let mut engine = StrategyEngine::new(3.0);
        let c1 = candle(1_000, 2_649.0, 2_652.0, 2_648.5, 2_651.5, 8_000.0);
        assert!(engine.observe_closed(&c1));
        assert!(!engine.observe_closed(&c1));
        assert!(!engine.observe_closed(&candle(900, 2_649.0, 2_652.0, 2_648.5, 2_651.5, 8_000.0)));
        assert_eq!(engine.history_len(), 1);
    }

    #[test]
    fn atr_is_the_mean_true_range_of_closed_candles() {
        let mut engine = StrategyEngine::new(3.0);
        engine.observe_closed(&candle(1, 100.0, 102.0, 99.0, 101.0, 1.0));
        engine.observe_closed(&candle(2, 101.0, 103.0, 100.0, 102.0, 1.0));
        // TR1 = 2 - 99 = 3; TR2 = max(3, 2, 1) = 3.
        assert!((engine.calculate_atr() - 3.0).abs() < 1e-9);
        assert_eq!(StrategyEngine::new(3.0).calculate_atr(), 3.0);
    }

    #[test]
    fn the_risk_projection_matches_the_node3_formula() {
        let mut risk = RiskProjector::new(RiskParams::default());
        risk.update_atr(1.80);
        let (sl, tp, sl_pips, tp_pips, rr) = risk.compute_details(3_419.80, "buy");
        // atr_pct = 1.8/3419.8 = 0.000526 -> clamp 0.001; *20 = 0.02:
        // SL = 200 + 100*0.02 = 202 pips ($2.02), TP = 600 + 200*0.02 = 604
        // pips ($6.04), RR = 604/202 = 2.9901.
        assert!((sl_pips - 202.0).abs() < 1e-9);
        assert!((sl - (3_419.80 - 2.02)).abs() < 1e-9);
        assert!((tp_pips - 604.0).abs() < 1e-9);
        assert!((tp - (3_419.80 + 6.04)).abs() < 1e-9);
        assert!((rr - 2.990_099).abs() < 1e-5);

        let (sell_sl, sell_tp, _, _, _) = risk.compute_details(3_419.80, "sell");
        assert!(sell_sl > 3_419.80 && sell_tp < 3_419.80);
    }

    #[test]
    fn the_risk_projection_clamps_the_rr_band() {
        // A large ATR saturates the pip ladder: 300 SL pips = 3.00, 800 TP = 8.00
        // -> raw RR 2.67, inside [2, 3].
        let mut risk = RiskProjector::new(RiskParams::default());
        risk.update_atr(500.0);
        let (_, _, sl_pips, tp_pips, rr) = risk.compute_details(2_650.0, "buy");
        assert!((sl_pips - 300.0).abs() < 1e-9);
        assert!((tp_pips - 800.0).abs() < 1e-9);
        assert!((2.0..=3.0).contains(&rr));

        // Force an out-of-band raw RR by requesting a tiny TP band.
        let params = RiskParams {
            sl_min_pips: 300.0,
            sl_max_pips: 300.0,
            tp_min_pips: 10.0,
            tp_max_pips: 10.0,
            rr_min: 2.0,
            rr_max: 3.0,
            atr_fallback: 3.0,
        };
        let risk = RiskProjector::new(params);
        let (_, _, sl_pips, tp_pips, rr) = risk.compute_details(2_650.0, "buy");
        assert!((sl_pips - 300.0).abs() < 1e-9);
        assert!((tp_pips - 600.0).abs() < 1e-9, "TP clamped to rr_min");
        assert!((rr - 2.0).abs() < 1e-9);
    }

    #[test]
    fn the_intent_id_is_deterministic_and_carries_no_wall_clock() {
        let trigger = Trigger {
            symbol: "XAUUSD".into(),
            interval: "15m".into(),
            side: "buy".into(),
            level_id: "PW_POC".into(),
            level_name: "PW PoC".into(),
            level_price: 2_650.0,
            reference_price: 2_650.3,
            source_candle_time: 1_792_780_200_000,
        };
        let first = intent_id(&trigger);
        let second = intent_id(&trigger);
        assert_eq!(first, second);
        assert_eq!(
            first,
            "n4-vp-break-retest-v1-xauusd-15m-1792780200000-buy-pw-poc"
        );

        let risk = RiskProjector::new(RiskParams::default());
        let intent = build_intent(&trigger, &risk, 120, 1_792_780_201_000);
        assert_eq!(intent.intent_id, first);
        assert_eq!(intent.strategy, STRATEGY_VERSION);
        assert_eq!(intent.expires_at, 1_792_780_201_000 + 120_000);
        assert_eq!(intent.source_candle_time, 1_792_780_200_000);
        assert_eq!(intent.level_name, "PW PoC");
        assert!(intent.stop_loss < intent.reference_price);
        assert!(intent.take_profit > intent.reference_price);
    }

    #[test]
    fn different_events_and_sides_get_different_ids() {
        let base = Trigger {
            symbol: "XAUUSD".into(),
            interval: "15m".into(),
            side: "buy".into(),
            level_id: "PW_POC".into(),
            level_name: "PW PoC".into(),
            level_price: 2_650.0,
            reference_price: 2_650.3,
            source_candle_time: 1_000,
        };
        let buy = intent_id(&base);
        let mut sell = base.clone();
        sell.side = "sell".into();
        assert_ne!(buy, intent_id(&sell));
        let mut next = base.clone();
        next.source_candle_time = 2_000;
        assert_ne!(buy, intent_id(&next));
        let mut other_level = base;
        other_level.level_id = "CW_POC".into();
        assert_ne!(buy, intent_id(&other_level));
    }
}
