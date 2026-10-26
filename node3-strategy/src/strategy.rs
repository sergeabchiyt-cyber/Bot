//! Break and retest state machine over the published volume-profile levels.
//!
//! Per level: a closed candle that breaks through the level flips it to
//! `BrokenAbove` or `BrokenBelow`. A later closed candle whose range touches
//! the ±$0.50 retest zone confirms it. A close beyond the $2.00 invalidation
//! distance cancels the break. At most one trigger is emitted per closed
//! candle, and levels are evaluated in sorted id order so the result is
//! deterministic.
//!
//! The volume gate is optional and OFF by default (decision D6). When enabled
//! with a positive threshold, a retest only confirms on a candle whose tick
//! volume reaches it.

use std::collections::{BTreeMap, HashMap, VecDeque};

use tracing::{debug, info};

use crate::drawings::level_identity;
use crate::risk::{RiskProjector, ATR_PERIOD, PIP_SIZE};
use crate::types::{ScannedLevelSetup, ScanningSnapshot, VpCandle, VpLevels};

/// Retest zone half-width in price units ($0.50).
pub const PROXIMITY_DOLLARS: f64 = 0.50;
/// Distance beyond the level that cancels a break ($2.00).
pub const INVALIDATION_DOLLARS: f64 = 2.00;
/// Bars retained for ATR. Wilder smoothing converges within a few multiples of the period.
const HISTORY_CAPACITY: usize = 200;

/// One published level the strategy watches.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelTarget {
    /// Short id, e.g. `PW_POC`. Sort key for deterministic evaluation.
    pub id: String,
    /// Display name, e.g. `PW PoC`.
    pub name: String,
    pub window: String,
    /// Level identity `(window, kind, start, end)` rendered as the drawing id.
    pub drawing_id: String,
    pub price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakState {
    None,
    BrokenAbove,
    BrokenBelow,
}

#[derive(Debug, Clone, Default)]
pub struct LevelDiagnosticsMeta {
    pub broken_at: Option<i64>,
    pub broken_price: Option<f64>,
    pub last_note: String,
}

/// A confirmed retest, ready to become an intent.
#[derive(Debug, Clone, PartialEq)]
pub struct Trigger {
    /// `buy` or `sell`.
    pub side: &'static str,
    pub reference_price: f64,
    pub level_id: String,
    pub level_name: String,
    pub drawing_id: String,
    pub source_candle_time: i64,
}

#[derive(Debug, Clone, Default)]
pub struct CandleEvalReport {
    pub triggered: Option<Trigger>,
    pub breaks_detected: u64,
    pub breaks_invalidated: u64,
    pub retests_rejected_low_volume: u64,
    pub signals_confirmed: u64,
    pub events: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub struct StrategyEngine {
    levels: BTreeMap<String, LevelTarget>,
    history: VecDeque<VpCandle>,
    break_states: HashMap<String, BreakState>,
    level_meta: HashMap<String, LevelDiagnosticsMeta>,
    last_triggered_candle: i64,
}

fn level_name(window: &str, kind: &str) -> String {
    let label = match kind {
        "poc" => "PoC",
        "vah" => "VaH",
        _ => "VaL",
    };
    format!("{window} {label}")
}

fn true_range(prev: &VpCandle, curr: &VpCandle) -> f64 {
    let high_low = curr.high - curr.low;
    let high_close = (curr.high - prev.close).abs();
    let low_close = (curr.low - prev.close).abs();
    high_low.max(high_close).max(low_close)
}

impl StrategyEngine {
    /// Replace the watched levels with the published set. Levels whose identity
    /// changed (week reset, new window bounds) lose their break state. Unchanged
    /// levels keep theirs.
    pub fn sync_levels(&mut self, levels: &[VpLevels]) {
        let mut next: BTreeMap<String, LevelTarget> = BTreeMap::new();
        for level in levels {
            let prices = [
                ("poc", Some(level.poc)),
                ("vah", level.vah),
                ("val", level.val),
            ];
            for (kind, price) in prices {
                let Some(price) = price else { continue };
                if !price.is_finite() || price <= 0.0 {
                    continue;
                }
                let id = format!("{}_{}", level.window, kind.to_uppercase());
                next.insert(
                    id.clone(),
                    LevelTarget {
                        id,
                        name: level_name(&level.window, kind),
                        window: level.window.clone(),
                        drawing_id: level_identity(&level.window, kind, level.start, level.end),
                        price,
                    },
                );
            }
        }

        let mut replaced = Vec::new();
        for (id, old) in &self.levels {
            let unchanged = matches!(next.get(id), Some(new) if new.drawing_id == old.drawing_id);
            if !unchanged {
                replaced.push(id.clone());
            }
        }
        for id in replaced {
            self.break_states.remove(&id);
            self.level_meta.remove(&id);
        }
        self.levels = next;
    }

    /// Record a closed bar for ATR without evaluating any level. Used for bars
    /// replayed on connect and for bars that must not trigger.
    pub fn seed(&mut self, candle: &VpCandle) {
        if self
            .history
            .back()
            .is_some_and(|last| candle.time <= last.time)
        {
            return;
        }
        self.history.push_back(candle.clone());
        while self.history.len() > HISTORY_CAPACITY {
            self.history.pop_front();
        }
    }

    /// ATR(`ATR_PERIOD`) in price units, Wilder-smoothed. `None` until enough
    /// bars have been seen. Strategy output then stays suppressed.
    pub fn atr(&self) -> Option<f64> {
        let bars: Vec<&VpCandle> = self.history.iter().collect();
        if bars.len() < ATR_PERIOD + 1 {
            return None;
        }
        let ranges: Vec<f64> = bars
            .windows(2)
            .map(|pair| true_range(pair[0], pair[1]))
            .collect();
        let period = ATR_PERIOD as f64;
        let mut atr = ranges[..ATR_PERIOD].iter().sum::<f64>() / period;
        for range in &ranges[ATR_PERIOD..] {
            atr = (atr * (period - 1.0) + range) / period;
        }
        Some(atr).filter(|value| value.is_finite() && *value > 0.0)
    }

    /// Evaluate one newly closed bar against every watched level.
    /// Callers pass only closed bars that are new for their interval.
    pub fn evaluate(&mut self, candle: &VpCandle, volume_threshold: f64) -> CandleEvalReport {
        self.seed(candle);
        let mut report = CandleEvalReport::default();
        if candle.time <= self.last_triggered_candle {
            return report;
        }
        let gate = volume_threshold > 0.0;
        let volume_ok = !gate || candle.volume >= volume_threshold;

        let ids: Vec<String> = self.levels.keys().cloned().collect();
        for id in ids {
            let Some(target) = self.levels.get(&id).cloned() else {
                continue;
            };
            if let Some(trigger) = self.step(
                &id,
                &target,
                candle,
                volume_ok,
                volume_threshold,
                &mut report,
            ) {
                report.triggered = Some(trigger);
                return report;
            }
        }
        report
    }

    fn step(
        &mut self,
        id: &str,
        target: &LevelTarget,
        candle: &VpCandle,
        volume_ok: bool,
        volume_threshold: f64,
        report: &mut CandleEvalReport,
    ) -> Option<Trigger> {
        let level = target.price;
        let state = self
            .break_states
            .get(id)
            .copied()
            .unwrap_or(BreakState::None);
        match state {
            BreakState::None => {
                let is_green = candle.close > candle.open;
                let is_red = candle.close < candle.open;
                if is_green && candle.close > level && candle.open <= level {
                    self.record_break(id, target, candle, BreakState::BrokenAbove, report);
                } else if is_red && candle.close < level && candle.open >= level {
                    self.record_break(id, target, candle, BreakState::BrokenBelow, report);
                }
                None
            }
            BreakState::BrokenAbove => {
                let retested = candle.low <= level + PROXIMITY_DOLLARS
                    && candle.close >= level - PROXIMITY_DOLLARS;
                if retested {
                    return self.confirm(
                        id,
                        target,
                        candle,
                        "buy",
                        volume_ok,
                        volume_threshold,
                        report,
                    );
                }
                if candle.close < level - INVALIDATION_DOLLARS {
                    let boundary = level - INVALIDATION_DOLLARS;
                    self.invalidate(id, target, candle, "bullish", boundary, report);
                }
                None
            }
            BreakState::BrokenBelow => {
                let retested = candle.high >= level - PROXIMITY_DOLLARS
                    && candle.close <= level + PROXIMITY_DOLLARS;
                if retested {
                    return self.confirm(
                        id,
                        target,
                        candle,
                        "sell",
                        volume_ok,
                        volume_threshold,
                        report,
                    );
                }
                if candle.close > level + INVALIDATION_DOLLARS {
                    let boundary = level + INVALIDATION_DOLLARS;
                    self.invalidate(id, target, candle, "bearish", boundary, report);
                }
                None
            }
        }
    }

    fn record_break(
        &mut self,
        id: &str,
        target: &LevelTarget,
        candle: &VpCandle,
        state: BreakState,
        report: &mut CandleEvalReport,
    ) {
        let (direction, retest_word) = match state {
            BreakState::BrokenAbove => ("ABOVE", "bullish"),
            _ => ("BELOW", "bearish"),
        };
        debug!(
            "Level {} broken {direction} at {:.2}",
            target.name, candle.close
        );
        self.break_states.insert(id.to_string(), state);
        let note = format!(
            "Broken {direction} @ {:.2} — awaiting {retest_word} retest of {:.2} (±${:.2})",
            candle.close, target.price, PROXIMITY_DOLLARS
        );
        self.level_meta.insert(
            id.to_string(),
            LevelDiagnosticsMeta {
                broken_at: Some(candle.time),
                broken_price: Some(candle.close),
                last_note: note.clone(),
            },
        );
        report.breaks_detected += 1;
        report
            .events
            .push(("info".into(), format!("{}: {}", target.name, note)));
    }

    #[allow(clippy::too_many_arguments)]
    fn confirm(
        &mut self,
        id: &str,
        target: &LevelTarget,
        candle: &VpCandle,
        side: &'static str,
        volume_ok: bool,
        volume_threshold: f64,
        report: &mut CandleEvalReport,
    ) -> Option<Trigger> {
        if !volume_ok {
            debug!(
                "Retest on {} but volume {:.1} < threshold {:.1}",
                target.name, candle.volume, volume_threshold
            );
            let note = format!(
                "Retested {:.2} but volume {:.0} < {:.0} threshold — waiting",
                target.price, candle.volume, volume_threshold
            );
            if let Some(meta) = self.level_meta.get_mut(id) {
                meta.last_note = note.clone();
            }
            report.retests_rejected_low_volume += 1;
            report
                .events
                .push(("warn".into(), format!("{}: {}", target.name, note)));
            return None;
        }
        info!(
            "{} SETUP CONFIRMED on {} @ {:.2}",
            if side == "buy" { "BULLISH" } else { "BEARISH" },
            target.name,
            candle.close
        );
        let verb = if side == "buy" { "BUY" } else { "SELL" };
        self.break_states.insert(id.to_string(), BreakState::None);
        self.last_triggered_candle = candle.time;
        let note = format!("RETEST CONFIRMED @ {:.2} — triggering {verb}", candle.close);
        self.level_meta.insert(
            id.to_string(),
            LevelDiagnosticsMeta {
                broken_at: None,
                broken_price: None,
                last_note: note.clone(),
            },
        );
        report.signals_confirmed += 1;
        report
            .events
            .push(("signal".into(), format!("{}: {}", target.name, note)));
        Some(Trigger {
            side,
            reference_price: candle.close,
            level_id: target.id.clone(),
            level_name: target.name.clone(),
            drawing_id: target.drawing_id.clone(),
            source_candle_time: candle.time,
        })
    }

    fn invalidate(
        &mut self,
        id: &str,
        target: &LevelTarget,
        candle: &VpCandle,
        direction: &str,
        boundary: f64,
        report: &mut CandleEvalReport,
    ) {
        self.break_states.insert(id.to_string(), BreakState::None);
        let note = format!(
            "{} break invalidated (close {:.2} beyond {:.2})",
            capitalise(direction),
            candle.close,
            boundary
        );
        self.level_meta.insert(
            id.to_string(),
            LevelDiagnosticsMeta {
                broken_at: None,
                broken_price: None,
                last_note: note.clone(),
            },
        );
        report.breaks_invalidated += 1;
        report
            .events
            .push(("warn".into(), format!("{}: {}", target.name, note)));
    }

    /// Scanning snapshot for `/scanning` and the `scanning` topic.
    pub fn scanning(&self, risk: &RiskProjector, context: &ScanContext<'_>) -> ScanningSnapshot {
        let last = self.history.back();
        let price = last.map(|bar| bar.close);
        let projection = risk.projection();
        let gate = context.volume_threshold > 0.0;
        let current_volume = last.map(|bar| bar.volume);
        let volume_confirmed = if gate {
            current_volume.map(|volume| volume >= context.volume_threshold)
        } else {
            None
        };

        let mut setups = Vec::new();
        let mut active = 0;
        for (id, target) in &self.levels {
            let state = self
                .break_states
                .get(id)
                .copied()
                .unwrap_or(BreakState::None);
            let meta = self.level_meta.get(id).cloned().unwrap_or_default();
            let (state_name, state_label, pending_side, invalidation) = match state {
                BreakState::None => ("scanning_break", "Scanning for break", None, None),
                BreakState::BrokenAbove => (
                    "broken_above",
                    "Broken above — awaiting retest",
                    Some("buy"),
                    Some(target.price - INVALIDATION_DOLLARS),
                ),
                BreakState::BrokenBelow => (
                    "broken_below",
                    "Broken below — awaiting retest",
                    Some("sell"),
                    Some(target.price + INVALIDATION_DOLLARS),
                ),
            };
            if state != BreakState::None {
                active += 1;
            }
            let distance = price.map(|p| (p - target.price).abs());
            let projected_buy = price.and_then(|p| risk.order(p, "buy"));
            let projected_sell = price.and_then(|p| risk.order(p, "sell"));
            let active_projection = match pending_side {
                Some("buy") => projected_buy.clone(),
                Some("sell") => projected_sell.clone(),
                _ => None,
            };
            setups.push(ScannedLevelSetup {
                id: id.clone(),
                name: target.name.clone(),
                window: target.window.clone(),
                level_price: target.price,
                drawing_id: target.drawing_id.clone(),
                state: state_name.into(),
                state_label: state_label.into(),
                pending_side: pending_side.map(str::to_string),
                current_price: price,
                distance_dollars: distance,
                distance_pips: distance.map(|d| d / PIP_SIZE),
                retest_zone_low: target.price - PROXIMITY_DOLLARS,
                retest_zone_high: target.price + PROXIMITY_DOLLARS,
                invalidation_price: invalidation,
                volume_required: gate.then_some(context.volume_threshold),
                volume_confirmed,
                projected_buy,
                projected_sell,
                active_projection,
                broken_at: meta.broken_at,
                broken_price: meta.broken_price,
                last_note: if meta.last_note.is_empty() {
                    "Monitoring for a break".to_string()
                } else {
                    meta.last_note
                },
            });
        }

        ScanningSnapshot {
            active_setups_count: active,
            total_levels_tracked: self.levels.len(),
            proximity_dollars: PROXIMITY_DOLLARS,
            invalidation_dollars: INVALIDATION_DOLLARS,
            volume_filter_enabled: gate,
            volume_threshold: context.volume_threshold,
            current_volume,
            volume_confirmed,
            last_price: price,
            strategy_interval: context.strategy_interval.to_string(),
            tradeable: context.tradeable,
            freshness_state: context.freshness_state.to_string(),
            atr: projection.map(|p| p.atr),
            atr_pips: projection.map(|p| p.atr_pips),
            sl_pips: projection.map(|p| p.sl_pips),
            tp_pips: projection.map(|p| p.tp_pips),
            rr: projection.map(|p| p.rr),
            setups,
            timestamp: context.timestamp,
        }
    }
}

/// Inputs to the scanning snapshot that come from outside the state machine.
pub struct ScanContext<'a> {
    pub tradeable: bool,
    pub freshness_state: &'a str,
    pub strategy_interval: &'a str,
    pub volume_threshold: f64,
    pub timestamp: i64,
}

fn capitalise(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::types::ProfileMeta;

    fn bar(time: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> VpCandle {
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

    fn meta() -> ProfileMeta {
        ProfileMeta {
            row_mode: "rows".into(),
            row_height: 0.01,
            rows: 128,
            range_low: 4000.0,
            range_high: 4200.0,
            va_pct: 0.7,
            total_volume: 1.0,
            input_interval: "5m".into(),
            input_bars: 1,
        }
    }

    fn pw_level(poc: f64) -> VpLevels {
        VpLevels {
            window: "PW".into(),
            poc,
            vah: Some(poc + 30.0),
            val: Some(poc - 30.0),
            start: 1_000,
            end: 2_000,
            timestamp: 0,
            sunday_open: Some(poc),
            meta: meta(),
        }
    }

    fn engine_with_poc(poc: f64) -> StrategyEngine {
        let mut engine = StrategyEngine::default();
        engine.sync_levels(&[pw_level(poc)]);
        engine
    }

    #[test]
    fn break_then_retest_emits_exactly_one_buy_intent_trigger() {
        let mut engine = engine_with_poc(4100.0);
        // Breaks above: green, close beyond 4100, open at or below it.
        let brk = engine.evaluate(&bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0), 0.0);
        assert_eq!(brk.breaks_detected, 1);
        assert!(brk.triggered.is_none());
        // Retest: low inside the zone (<= 4100.5) and close back above 4099.5.
        let retest = engine.evaluate(&bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 10.0), 0.0);
        let trigger = retest.triggered.expect("one trigger");
        assert_eq!(trigger.side, "buy");
        assert_eq!(trigger.level_id, "PW_POC");
        assert_eq!(trigger.drawing_id, "PW:poc:1000:2000");
        assert_eq!(trigger.source_candle_time, 2_000);
        assert!((trigger.reference_price - 4100.2).abs() < 1e-9);
        assert_eq!(retest.signals_confirmed, 1);
        // The same candle time can never trigger twice.
        let again = engine.evaluate(&bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 10.0), 0.0);
        assert!(again.triggered.is_none());
    }

    #[test]
    fn break_and_retest_are_deterministic_across_identical_runs() {
        let run = || {
            let mut engine = engine_with_poc(4100.0);
            let mut out = Vec::new();
            let bars = [
                bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0),
                bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 10.0),
            ];
            for candle in &bars {
                out.push(engine.evaluate(candle, 0.0).triggered);
            }
            out
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_close_beyond_invalidation_cancels_the_break() {
        let mut engine = engine_with_poc(4100.0);
        engine.evaluate(&bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0), 0.0);
        let report = engine.evaluate(&bar(2_000, 4099.0, 4099.2, 4097.0, 4097.5, 10.0), 0.0);
        assert_eq!(report.breaks_invalidated, 1);
        assert!(report.triggered.is_none());
        // Back to scanning: a later retest of a fresh break is needed.
        let context = ScanContext {
            tradeable: true,
            freshness_state: "live",
            strategy_interval: "15m",
            volume_threshold: 0.0,
            timestamp: 0,
        };
        let scan = engine.scanning(&RiskProjector::new(&Config::default()), &context);
        assert_eq!(scan.active_setups_count, 0);
    }

    #[test]
    fn volume_gate_is_off_by_default_so_a_thin_retest_still_triggers() {
        let mut engine = engine_with_poc(4100.0);
        engine.evaluate(&bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0), 0.0);
        // Volume 1 is far below any sensible threshold, yet the gate is off.
        let report = engine.evaluate(&bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 1.0), 0.0);
        assert!(report.triggered.is_some());
        assert_eq!(report.retests_rejected_low_volume, 0);
    }

    #[test]
    fn enabled_volume_gate_rejects_low_volume_then_confirms_on_volume() {
        let mut engine = engine_with_poc(4100.0);
        engine.evaluate(&bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 50.0), 10.0);
        let rejected = engine.evaluate(&bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 5.0), 10.0);
        assert!(rejected.triggered.is_none());
        assert_eq!(rejected.retests_rejected_low_volume, 1);
        let confirmed = engine.evaluate(&bar(3_000, 4100.2, 4100.6, 4099.9, 4100.3, 20.0), 10.0);
        assert!(confirmed.triggered.is_some());
    }

    #[test]
    fn bearish_break_and_retest_sell_on_the_same_mirror_rules() {
        let mut engine = engine_with_poc(4100.0);
        // Breaks below: red, close beneath 4100, open at or above it.
        engine.evaluate(&bar(1_000, 4100.2, 4100.4, 4098.8, 4099.0, 10.0), 0.0);
        // Retest: high reaches 4099.5 and close stays at or below 4100.5.
        let report = engine.evaluate(&bar(2_000, 4099.2, 4099.9, 4099.0, 4099.8, 10.0), 0.0);
        assert_eq!(report.triggered.map(|t| t.side), Some("sell"));
    }

    #[test]
    fn one_trigger_per_candle_even_when_several_levels_retest() {
        let mut engine = StrategyEngine::default();
        let mut level = pw_level(4100.0);
        level.vah = Some(4100.2);
        level.val = Some(4099.9);
        engine.sync_levels(&[level]);
        engine.evaluate(&bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0), 0.0);
        let report = engine.evaluate(&bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 10.0), 0.0);
        assert!(report.triggered.is_some());
        assert_eq!(report.signals_confirmed, 1);
    }

    #[test]
    fn seed_builds_history_without_triggering() {
        let mut engine = engine_with_poc(4100.0);
        engine.seed(&bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0));
        engine.seed(&bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 10.0));
        assert_eq!(engine.atr(), None, "two bars are not enough for ATR(14)");
        // Seeding never changes break state.
        let report = engine.evaluate(&bar(3_000, 4100.2, 4100.4, 4100.0, 4100.3, 10.0), 0.0);
        assert!(report.triggered.is_none());
    }

    #[test]
    fn atr_needs_fifteen_bars_then_follows_wilder_smoothing() {
        let mut engine = StrategyEngine::default();
        for index in 0..14 {
            let t = 1_000 * (index + 1);
            engine.seed(&bar(t, 4100.0, 4101.0, 4099.0, 4100.0, 1.0));
        }
        assert!(engine.atr().is_none(), "14 bars is one short of ATR(14)");
        engine.seed(&bar(15_000, 4100.0, 4101.0, 4099.0, 4100.0, 1.0));
        let atr = engine.atr().expect("ATR available");
        // Constant true range 2.0 gives ATR exactly 2.0.
        assert!((atr - 2.0).abs() < 1e-9, "atr {atr}");
    }

    #[test]
    fn replaced_levels_drop_state_and_unchanged_levels_keep_it() {
        let mut engine = engine_with_poc(4100.0);
        engine.evaluate(&bar(1_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0), 0.0);
        // Same identity: the break survives a re-sync.
        engine.sync_levels(&[pw_level(4100.0)]);
        let kept = engine.evaluate(&bar(2_000, 4101.0, 4101.3, 4099.8, 4100.2, 10.0), 0.0);
        assert!(kept.triggered.is_some());
        // New week bounds: a different identity, so the state resets.
        engine.evaluate(&bar(3_000, 4099.8, 4101.2, 4099.5, 4101.0, 10.0), 0.0);
        let mut next_week = pw_level(4100.0);
        next_week.start = 5_000;
        next_week.end = 6_000;
        engine.sync_levels(&[next_week]);
        let after = engine.evaluate(&bar(4_000, 4101.0, 4101.3, 4099.8, 4100.2, 10.0), 0.0);
        assert!(after.triggered.is_none());
    }

    #[test]
    fn levels_are_watched_in_sorted_id_order() {
        let engine = {
            let mut e = StrategyEngine::default();
            e.sync_levels(&[pw_level(4100.0)]);
            e
        };
        let ids: Vec<&str> = engine.levels.keys().map(String::as_str).collect();
        assert_eq!(ids, vec!["PW_POC", "PW_VAH", "PW_VAL"]);
    }
}
