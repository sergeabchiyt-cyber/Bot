//! Wires live Node 1 closed candles into the strategy and the existing
//! execution pipeline.
//!
//! The runner is deliberately thin: it owns scheduling and diagnostics, while
//! [`crate::strategy`] owns the decision and [`crate::node3`] owns the durable
//! intent/venue path. A replay candle updates the engine's history but is
//! **never evaluated**; only a live, contract-valid, fresh `closed == true`
//! candle can produce an intent.

use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::node1::{FeedEvent, MarketFeed};
use crate::strategy::{
    build_intent, intent_id, Candle, Level, RiskParams, RiskProjector, StrategyEngine,
    STRATEGY_VERSION,
};
use crate::types::StrategySnapshot;

/// Shared strategy state for `/diagnostics`.
#[derive(Clone)]
pub struct StrategyHandle {
    inner: Arc<RwLock<StrategySnapshot>>,
}

impl StrategyHandle {
    pub fn new(config: &Config) -> Self {
        let local = config.intent_source.is_local();
        let snapshot = StrategySnapshot {
            enabled: local,
            mode: if local { "local" } else { "monitor" }.into(),
            strategy_version: STRATEGY_VERSION.into(),
            symbol: config.node1_symbol.clone(),
            interval: config.node1_interval.clone(),
            volume_threshold: config.strategy_volume_threshold,
            sl_min_pips: config.strategy_sl_min_pips,
            sl_max_pips: config.strategy_sl_max_pips,
            tp_min_pips: config.strategy_tp_min_pips,
            tp_max_pips: config.strategy_tp_max_pips,
            rr_min: config.strategy_rr_min,
            rr_max: config.strategy_rr_max,
            ttl_secs: config.strategy_ttl_secs,
            ..Default::default()
        };
        Self {
            inner: Arc::new(RwLock::new(snapshot)),
        }
    }

    pub fn snapshot(&self) -> StrategySnapshot {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn update(&self, change: impl FnOnce(&mut StrategySnapshot)) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        change(&mut guard);
        guard.updated_at = Some(chrono::Utc::now().timestamp_millis());
    }
}

struct Runner {
    config: Config,
    feed: MarketFeed,
    shared: crate::node3::LinkShared,
    handle: StrategyHandle,
    engine: StrategyEngine,
    risk: RiskProjector,
    symbol: String,
    interval: String,
}

impl Runner {
    fn new(
        config: Config,
        feed: MarketFeed,
        shared: crate::node3::LinkShared,
        handle: StrategyHandle,
    ) -> Self {
        let risk_params = RiskParams {
            sl_min_pips: config.strategy_sl_min_pips,
            sl_max_pips: config.strategy_sl_max_pips,
            tp_min_pips: config.strategy_tp_min_pips,
            tp_max_pips: config.strategy_tp_max_pips,
            rr_min: config.strategy_rr_min,
            rr_max: config.strategy_rr_max,
            atr_fallback: config.strategy_atr_fallback,
        };
        let engine = StrategyEngine::new(config.strategy_atr_fallback);
        Self {
            symbol: config.node1_symbol.clone(),
            interval: config.node1_interval.clone(),
            config,
            feed,
            shared,
            handle,
            engine,
            risk: RiskProjector::new(risk_params),
        }
    }

    fn on_levels(&mut self, level: Level) {
        self.engine.update_levels(&level);
        let levels = self.engine.levels_tracked();
        self.handle.update(|snapshot| {
            snapshot.levels_tracked = levels;
        });
    }

    async fn on_candle(&mut self, candle: Candle, replay: bool) {
        if !self.engine.observe_closed(&candle) {
            // A duplicate or out-of-order replay is history only.
            return;
        }
        let atr = self.engine.calculate_atr();
        self.risk.update_atr(atr);
        let (breaks, invalidated, rejected, confirmed) = self.engine.counters();
        let levels = self.engine.levels_tracked();
        self.handle.update(|snapshot| {
            snapshot.atr = atr;
            snapshot.candles_observed = snapshot.candles_observed.saturating_add(1);
            snapshot.last_candle_time = Some(candle.time);
            snapshot.levels_tracked = levels;
            snapshot.breaks_detected = breaks;
            snapshot.breaks_invalidated = invalidated;
            snapshot.retests_rejected_low_volume = rejected;
            snapshot.signals_confirmed = confirmed;
        });

        // The subscribe replay exists to seed history and levels.
        if replay {
            return;
        }

        // The gate is checked immediately before a decision, and again inside
        // the execution pipeline immediately before any broker write.
        if !self.feed.is_trading_ready() {
            let reason = self
                .feed
                .snapshot()
                .not_ready_reason
                .unwrap_or_else(|| "market feed is not trading-ready".into());
            self.suppress(&reason);
            return;
        }

        let now = chrono::Utc::now().timestamp_millis();
        let decision = self.engine.evaluate_last(
            &self.symbol,
            &self.interval,
            self.config.strategy_volume_threshold,
            now,
        );
        self.handle.update(|snapshot| {
            snapshot.atr = decision.atr;
            snapshot.last_decision_at_ms = Some(decision.evaluated_at_ms);
            snapshot.last_action = decision.action.clone();
            snapshot.levels_tracked = decision.levels_tracked;
        });
        debug!(
            "strategy evaluation {} {}: action={} breaks={} invalidated={} low_volume={} \
             confirmed={} events=[{}]",
            decision.symbol,
            decision.interval,
            decision.action,
            decision.breaks_detected,
            decision.breaks_invalidated,
            decision.retests_rejected_low_volume,
            decision.signals_confirmed,
            decision.events.join("; ")
        );

        let Some(trigger) = decision.trigger.clone() else {
            return;
        };

        let intent = build_intent(
            &trigger,
            &self.risk,
            self.config.strategy_ttl_secs,
            now,
        );
        let expected_id = intent_id(&trigger);
        debug_assert_eq!(intent.intent_id, expected_id);
        self.handle.update(|snapshot| {
            snapshot.last_trigger_level = Some(trigger.level_name.clone());
            snapshot.last_intent_id = Some(intent.intent_id.clone());
            snapshot.intents_emitted = snapshot.intents_emitted.saturating_add(1);
            snapshot.last_suppression_reason = None;
        });

        info!(
            "local strategy {} emitted {} for {} on {} (candle {}, level {} @ {:.2})",
            STRATEGY_VERSION,
            intent.intent_id,
            intent.side,
            intent.symbol,
            intent.source_candle_time,
            trigger.level_name,
            trigger.level_price
        );
        // The execution pipeline validates, durably records before any broker
        // write, deduplicates by intent_id and reconciles unclear outcomes.
        crate::node3::handle_local_intent(self.shared.clone(), intent).await;
    }

    fn suppress(&self, reason: &str) {
        warn!("local strategy suppressed a signal: {reason}");
        self.handle.update(|snapshot| {
            snapshot.intents_suppressed = snapshot.intents_suppressed.saturating_add(1);
            snapshot.last_suppression_reason = Some(reason.to_string());
        });
    }
}

pub async fn run(
    config: Config,
    feed: MarketFeed,
    shared: crate::node3::LinkShared,
    handle: StrategyHandle,
    mut rx: mpsc::Receiver<FeedEvent>,
) {
    info!(
        "local strategy runner started: {} {} {} (threshold {:.0})",
        STRATEGY_VERSION, config.node1_symbol, config.node1_interval,
        config.strategy_volume_threshold
    );
    let mut runner = Runner::new(config, feed, shared, handle);
    while let Some(event) = rx.recv().await {
        match event {
            FeedEvent::Levels { level } => runner.on_levels(level),
            FeedEvent::ClosedCandle { candle, replay } => {
                runner.on_candle(candle, replay).await
            }
        }
    }
    warn!("local strategy runner stopped: the market feed channel closed");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, IntentSource};
    use crate::diagnostics::DiagnosticsHub;
    use crate::execution::ExecutionManager;
    use crate::ledger::ExecutionLedger;
    use crate::node1::MarketFeed;
    use crate::strategy::Trigger;
    use std::sync::Arc;

    fn config() -> Config {
        Config {
            intent_source: IntentSource::Node1Strategy,
            node1_feed_enabled: true,
            node1_ws_url: "wss://engine.example/ws".into(),
            strategy_approved: true,
            node1_min_closed_candles: 1,
            execution_ledger_file: std::env::temp_dir()
                .join(format!("node4-strategy-test-{}.jsonl", std::process::id()))
                .display()
                .to_string(),
            ..Default::default()
        }
    }

    fn shared(config: &Config, feed: MarketFeed) -> crate::node3::LinkShared {
        let hub = DiagnosticsHub::new(config);
        let manager = Arc::new(ExecutionManager::new(config, hub.mt5()));
        let ledger = Arc::new(ExecutionLedger::open(&config.execution_ledger_file));
        crate::node3::LinkShared::for_local(
            config.clone(),
            hub,
            manager,
            ledger,
            feed,
        )
    }

    #[tokio::test]
    async fn a_replay_candle_updates_history_but_emits_nothing() {
        let config = config();
        let feed = MarketFeed::new(&config);
        let handle = StrategyHandle::new(&config);
        let shared = shared(&config, feed.clone());
        let mut runner = Runner::new(config, feed, shared, handle.clone());

        let candle = Candle {
            time: 1_700_000_000_000,
            open: 2_649.0,
            high: 2_652.0,
            low: 2_648.0,
            close: 2_651.0,
            volume: 12_000.0,
        };
        runner.on_candle(candle, true).await;
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.candles_observed, 1);
        assert_eq!(snapshot.intents_emitted, 0);
    }

    #[tokio::test]
    async fn a_replayed_strategy_event_cannot_place_a_second_order() {
        let mut config = config();
        config.execution_ledger_file = std::env::temp_dir()
            .join(format!(
                "node4-strategy-dedupe-{}.jsonl",
                std::process::id()
            ))
            .display()
            .to_string();
        let feed = MarketFeed::new(&config);
        let hub = DiagnosticsHub::new(&config);
        let manager = Arc::new(ExecutionManager::new(&config, hub.mt5()));
        let ledger = Arc::new(ExecutionLedger::open(&config.execution_ledger_file));
        let shared = crate::node3::LinkShared::for_local(
            config.clone(),
            hub.clone(),
            manager,
            ledger.clone(),
            feed,
        );

        let trigger = Trigger {
            symbol: "XAUUSD".into(),
            interval: "15m".into(),
            side: "buy".into(),
            level_id: "PW_POC".into(),
            level_name: "PW PoC".into(),
            level_price: 2_650.0,
            reference_price: 2_650.5,
            source_candle_time: 1_700_000_000_000,
        };
        let risk = RiskProjector::new(RiskParams::default());
        let intent = build_intent(
            &trigger,
            &risk,
            config.strategy_ttl_secs,
            chrono::Utc::now().timestamp_millis(),
        );

        // The same strategy event replayed (a reconnect, a resubscribe or a
        // duplicate feed frame) must resolve to the same durable key.
        crate::node3::handle_local_intent(shared.clone(), intent.clone()).await;
        crate::node3::handle_local_intent(shared.clone(), intent.clone()).await;

        let snapshot = hub.snapshot().await;
        assert_eq!(snapshot.work.intents_received, 2);
        assert_eq!(snapshot.work.intents_duplicate, 1);
        assert_eq!(
            snapshot.work.orders_placed, 0,
            "the default venue is an explicit dry run"
        );
        assert!(ledger.state(&intent.intent_id).await.is_some());
    }

    #[test]
    fn the_handle_reports_a_monitor_mode_when_node3_is_the_source() {
        let mut config = config();
        config.intent_source = IntentSource::Node3;
        let handle = StrategyHandle::new(&config);
        let snapshot = handle.snapshot();
        assert!(!snapshot.enabled);
        assert_eq!(snapshot.mode, "monitor");
        assert_eq!(snapshot.strategy_version, STRATEGY_VERSION);
    }
}
