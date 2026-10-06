use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};

use crate::config::Config;
use crate::types::{
    ActivityLogEntry, DiagnosticsSnapshot, ExecutionReport, ScanningSnapshot, SignalSnapshot,
    StrategyWorkDiagnostics, TradeIntent, VpCandle, WsFrame, INTENT_SCHEMA_VERSION,
};

const MAX_RECENT_EVENTS: usize = 50;
const MAX_RECENT_INTENTS: usize = 100;
const MAX_EXECUTION_REPORTS: usize = 100;
const CANDLE_BUFFER_CAPACITY: usize = 30;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct DiagnosticsInner {
    scanning: ScanningSnapshot,
    pending_intents: VecDeque<TradeIntent>,
    recent_intents: VecDeque<TradeIntent>,
    execution_reports: VecDeque<ExecutionReport>,
    work: StrategyWorkDiagnostics,
    recent_events: VecDeque<ActivityLogEntry>,
    max_pending_intents: usize,
}

#[derive(Clone)]
pub struct DiagnosticsHub {
    inner: Arc<RwLock<DiagnosticsInner>>,
    tx: broadcast::Sender<WsFrame>,
    public_ws_clients: Arc<AtomicUsize>,
    execution_client_connected: Arc<AtomicBool>,
    node4_token_configured: bool,
}

impl DiagnosticsHub {
    pub fn new(config: &Config) -> Self {
        let (tx, _) = broadcast::channel(512);
        let started_at = now_ms();
        let node4_token_configured = config.node4_link_configured();

        let scanning = ScanningSnapshot {
            active_setups_count: 0,
            total_levels_tracked: 0,
            proximity_dollars: 0.50,
            invalidation_dollars: 2.00,
            volume_threshold: config.volume_threshold,
            current_volume: None,
            volume_confirmed: false,
            last_price: None,
            atr: 3.0,
            atr_pips: 300.0,
            setups: Vec::new(),
            timestamp: started_at,
        };

        let work = StrategyWorkDiagnostics {
            service: "xauusd-node3-strategy".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            role: "strategy_only".into(),
            started_at,
            uptime_secs: 0,
            node1_ws_url: config.node1_ws_url.clone(),
            node1_connected: false,
            node1_state: "starting".into(),
            node1_reconnect_count: 0,
            last_node1_msg_ts: None,
            last_candle_ts: None,
            last_levels_ts: None,
            candles_received: 0,
            levels_received: 0,
            ws_messages_received: 0,
            candle_buffer_len: 0,
            candle_buffer_capacity: CANDLE_BUFFER_CAPACITY,
            last_price: None,
            last_candle: None,
            atr: 3.0,
            atr_pips: 300.0,
            current_sl_pips: config.sl_min_pips,
            current_tp_pips: config.tp_min_pips,
            current_rr: (config.tp_min_pips / config.sl_min_pips.max(1.0))
                .clamp(config.rr_min, config.rr_max),
            volume_threshold: config.volume_threshold,
            last_candle_volume: None,
            volume_ratio: None,
            sl_min_pips: config.sl_min_pips,
            sl_max_pips: config.sl_max_pips,
            tp_min_pips: config.tp_min_pips,
            tp_max_pips: config.tp_max_pips,
            rr_min: config.rr_min,
            rr_max: config.rr_max,
            breaks_detected: 0,
            breaks_invalidated: 0,
            retests_rejected_low_volume: 0,
            signals_confirmed: 0,
            intents_emitted: 0,
            intents_dropped: 0,
            execution_reports_received: 0,
            last_signal_ts: None,
            last_execution_report_ts: None,
            last_error: None,
            public_ws_clients_connected: 0,
            node4_connected: false,
            node4_token_configured,
        };

        let mut recent_events = VecDeque::with_capacity(MAX_RECENT_EVENTS);
        recent_events.push_front(ActivityLogEntry {
            timestamp: started_at,
            level: if node4_token_configured { "info" } else { "warn" }.into(),
            category: "system".into(),
            message: if node4_token_configured {
                "Node 3 strategy started; private Node 4 link is configured".into()
            } else {
                "Node 3 strategy started without NODE4_SHARED_TOKEN; intents will be queued but /execution is disabled".into()
            },
        });

        Self {
            inner: Arc::new(RwLock::new(DiagnosticsInner {
                scanning,
                pending_intents: VecDeque::with_capacity(config.max_pending_intents),
                recent_intents: VecDeque::with_capacity(MAX_RECENT_INTENTS),
                execution_reports: VecDeque::with_capacity(MAX_EXECUTION_REPORTS),
                work,
                recent_events,
                max_pending_intents: config.max_pending_intents,
            })),
            tx,
            public_ws_clients: Arc::new(AtomicUsize::new(0)),
            execution_client_connected: Arc::new(AtomicBool::new(false)),
            node4_token_configured,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsFrame> {
        self.tx.subscribe()
    }

    pub fn public_client_connected(&self) -> usize {
        self.public_ws_clients.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn public_client_disconnected(&self) -> usize {
        let previous = self.public_ws_clients.fetch_sub(1, Ordering::SeqCst);
        previous.saturating_sub(1)
    }

    pub fn public_clients(&self) -> usize {
        self.public_ws_clients.load(Ordering::SeqCst)
    }

    /// Only one Node 4 execution consumer may be active. Multiple consumers
    /// would race the same intent and are therefore rejected fail-closed.
    pub fn try_execution_client_connected(&self) -> bool {
        self.execution_client_connected
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn execution_client_disconnected(&self) {
        self.execution_client_connected
            .store(false, Ordering::SeqCst);
    }

    pub fn node4_connected(&self) -> bool {
        self.execution_client_connected.load(Ordering::SeqCst)
    }

    fn push_event_locked(
        inner: &mut DiagnosticsInner,
        level: &str,
        category: &str,
        message: String,
    ) -> ActivityLogEntry {
        let entry = ActivityLogEntry {
            timestamp: now_ms(),
            level: level.into(),
            category: category.into(),
            message,
        };
        inner.recent_events.push_front(entry.clone());
        while inner.recent_events.len() > MAX_RECENT_EVENTS {
            inner.recent_events.pop_back();
        }
        entry
    }

    fn build_signals_locked(
        inner: &DiagnosticsInner,
        node4_connected: bool,
        node4_token_configured: bool,
    ) -> SignalSnapshot {
        let timestamp = now_ms();
        let pending: Vec<_> = inner
            .pending_intents
            .iter()
            .filter(|intent| !intent.is_expired_at(timestamp))
            .cloned()
            .collect();
        SignalSnapshot {
            pending_count: pending.len(),
            pending,
            recent: inner.recent_intents.iter().cloned().collect(),
            execution_reports: inner.execution_reports.iter().cloned().collect(),
            node4_connected,
            node4_token_configured,
            timestamp,
        }
    }

    fn build_snapshot_locked(
        inner: &DiagnosticsInner,
        public_clients: usize,
        node4_connected: bool,
        node4_token_configured: bool,
    ) -> DiagnosticsSnapshot {
        let timestamp = now_ms();
        let mut work = inner.work.clone();
        work.uptime_secs = ((timestamp - work.started_at).max(0) / 1_000) as u64;
        work.public_ws_clients_connected = public_clients;
        work.node4_connected = node4_connected;
        work.node4_token_configured = node4_token_configured;
        DiagnosticsSnapshot {
            timestamp,
            scanning: inner.scanning.clone(),
            signals: Self::build_signals_locked(inner, node4_connected, node4_token_configured),
            work,
            recent_events: inner.recent_events.iter().cloned().collect(),
        }
    }

    pub async fn snapshot(&self) -> DiagnosticsSnapshot {
        let inner = self.inner.read().await;
        Self::build_snapshot_locked(
            &inner,
            self.public_clients(),
            self.node4_connected(),
            self.node4_token_configured,
        )
    }

    pub async fn scanning_snapshot(&self) -> ScanningSnapshot {
        self.inner.read().await.scanning.clone()
    }

    pub async fn signal_snapshot(&self) -> SignalSnapshot {
        let inner = self.inner.read().await;
        Self::build_signals_locked(&inner, self.node4_connected(), self.node4_token_configured)
    }

    pub async fn pending_intents(&self) -> Vec<TradeIntent> {
        let timestamp = now_ms();
        self.inner
            .read()
            .await
            .pending_intents
            .iter()
            .filter(|intent| !intent.is_expired_at(timestamp))
            .cloned()
            .collect()
    }

    pub async fn record_ws_message(&self) {
        let mut inner = self.inner.write().await;
        inner.work.ws_messages_received += 1;
        inner.work.last_node1_msg_ts = Some(now_ms());
    }

    pub async fn set_node1_state(&self, connected: bool, state: &str, reconnect: bool) {
        let (event, snapshot) = {
            let mut inner = self.inner.write().await;
            inner.work.node1_connected = connected;
            inner.work.node1_state = state.into();
            if reconnect {
                inner.work.node1_reconnect_count += 1;
            }
            let event = Self::push_event_locked(
                &mut inner,
                if connected { "info" } else { "warn" },
                "node1",
                format!("Node 1 stream: {state}"),
            );
            let snapshot = Self::build_snapshot_locked(
                &inner,
                self.public_clients(),
                self.node4_connected(),
                self.node4_token_configured,
            );
            (event, snapshot)
        };
        let _ = self.tx.send(WsFrame::DiagnosticEvent { data: event });
        let _ = self.tx.send(WsFrame::Diagnostics {
            data: Box::new(snapshot),
        });
    }

    pub async fn record_levels_update(
        &self,
        window: &str,
        poc: f64,
        vah: f64,
        val: f64,
        scanning: ScanningSnapshot,
    ) {
        let (event, snapshot) = {
            let mut inner = self.inner.write().await;
            let timestamp = now_ms();
            inner.work.levels_received += 1;
            inner.work.last_levels_ts = Some(timestamp);
            inner.work.last_node1_msg_ts = Some(timestamp);
            inner.scanning = scanning.clone();
            let event = Self::push_event_locked(
                &mut inner,
                "info",
                "levels",
                format!("Updated {window} levels (PoC {poc:.2}, VaH {vah:.2}, VaL {val:.2})"),
            );
            let snapshot = Self::build_snapshot_locked(
                &inner,
                self.public_clients(),
                self.node4_connected(),
                self.node4_token_configured,
            );
            (event, snapshot)
        };
        let _ = self.tx.send(WsFrame::Scanning { data: scanning });
        let _ = self.tx.send(WsFrame::DiagnosticEvent { data: event });
        let _ = self.tx.send(WsFrame::Diagnostics {
            data: Box::new(snapshot),
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_candle_update(
        &self,
        candle: &VpCandle,
        atr: f64,
        sl_pips: f64,
        tp_pips: f64,
        risk_reward: f64,
        candle_buffer_len: usize,
        breaks_delta: u64,
        invalidated_delta: u64,
        low_volume_delta: u64,
        signals_delta: u64,
        eval_events: Vec<(String, String)>,
        scanning: ScanningSnapshot,
    ) {
        let (events, snapshot) = {
            let mut inner = self.inner.write().await;
            let timestamp = now_ms();
            inner.work.candles_received += 1;
            inner.work.last_candle_ts = Some(timestamp);
            inner.work.last_node1_msg_ts = Some(timestamp);
            inner.work.candle_buffer_len = candle_buffer_len;
            inner.work.last_price = Some(candle.close);
            inner.work.last_candle = Some(candle.clone());
            inner.work.atr = atr;
            inner.work.atr_pips = atr * 100.0;
            inner.work.current_sl_pips = sl_pips;
            inner.work.current_tp_pips = tp_pips;
            inner.work.current_rr = risk_reward;
            inner.work.last_candle_volume = Some(candle.volume);
            inner.work.volume_ratio = (inner.work.volume_threshold > 0.0)
                .then_some(candle.volume / inner.work.volume_threshold);
            inner.work.breaks_detected += breaks_delta;
            inner.work.breaks_invalidated += invalidated_delta;
            inner.work.retests_rejected_low_volume += low_volume_delta;
            inner.work.signals_confirmed += signals_delta;
            inner.scanning = scanning.clone();

            let events: Vec<_> = eval_events
                .into_iter()
                .map(|(level, message)| {
                    Self::push_event_locked(&mut inner, &level, "scanner", message)
                })
                .collect();
            let snapshot = Self::build_snapshot_locked(
                &inner,
                self.public_clients(),
                self.node4_connected(),
                self.node4_token_configured,
            );
            (events, snapshot)
        };

        let _ = self.tx.send(WsFrame::Scanning { data: scanning });
        for event in events {
            let _ = self.tx.send(WsFrame::DiagnosticEvent { data: event });
        }
        let _ = self.tx.send(WsFrame::Diagnostics {
            data: Box::new(snapshot),
        });
    }

    /// Queue and broadcast an immutable intent. Returns false when the bounded
    /// replay queue is full of still-valid intents; in that case nothing is
    /// sent to Node 4, which is safer than producing an untracked order.
    pub async fn record_intent(&self, intent: TradeIntent) -> bool {
        let (accepted, event, signal_snapshot, snapshot) = {
            let mut inner = self.inner.write().await;
            let timestamp = now_ms();
            inner
                .pending_intents
                .retain(|pending| !pending.is_expired_at(timestamp));

            if inner.pending_intents.len() >= inner.max_pending_intents {
                inner.work.intents_dropped += 1;
                inner.work.last_error = Some("pending intent queue is full".into());
                let event = Self::push_event_locked(
                    &mut inner,
                    "error",
                    "strategy",
                    format!(
                        "Intent {} was not emitted: pending intent queue is full",
                        intent.intent_id
                    ),
                );
                let signals = Self::build_signals_locked(
                    &inner,
                    self.node4_connected(),
                    self.node4_token_configured,
                );
                let snapshot = Self::build_snapshot_locked(
                    &inner,
                    self.public_clients(),
                    self.node4_connected(),
                    self.node4_token_configured,
                );
                (false, event, signals, snapshot)
            } else {
                inner.work.intents_emitted += 1;
                inner.work.last_signal_ts = Some(timestamp);
                inner.pending_intents.push_back(intent.clone());
                inner.recent_intents.push_front(intent.clone());
                while inner.recent_intents.len() > MAX_RECENT_INTENTS {
                    inner.recent_intents.pop_back();
                }
                let event = Self::push_event_locked(
                    &mut inner,
                    "signal",
                    "strategy",
                    format!(
                        "Emitted {} {} intent {} from {} (SL {:.2}, TP {:.2})",
                        intent.side.to_uppercase(),
                        intent.symbol,
                        intent.intent_id,
                        intent.level_name,
                        intent.stop_loss,
                        intent.take_profit
                    ),
                );
                let signals = Self::build_signals_locked(
                    &inner,
                    self.node4_connected(),
                    self.node4_token_configured,
                );
                let snapshot = Self::build_snapshot_locked(
                    &inner,
                    self.public_clients(),
                    self.node4_connected(),
                    self.node4_token_configured,
                );
                (true, event, signals, snapshot)
            }
        };

        if accepted {
            let _ = self.tx.send(WsFrame::TradeIntent {
                data: intent.clone(),
            });
        }
        let _ = self.tx.send(WsFrame::Signals {
            data: signal_snapshot,
        });
        let _ = self.tx.send(WsFrame::DiagnosticEvent { data: event });
        let _ = self.tx.send(WsFrame::Diagnostics {
            data: Box::new(snapshot),
        });
        accepted
    }

    pub async fn record_execution_report(&self, report: ExecutionReport) -> Result<(), String> {
        let (event, signal_snapshot, snapshot) = {
            let mut inner = self.inner.write().await;
            if report.schema_version != INTENT_SCHEMA_VERSION {
                return Err(format!(
                    "unsupported execution report schema_version {}",
                    report.schema_version
                ));
            }
            if !matches!(
                report.status.as_str(),
                "accepted" | "filled" | "partial" | "rejected" | "unknown" | "cancelled" | "closed"
            ) {
                return Err(format!("unsupported execution status '{}'", report.status));
            }

            let expected = inner
                .recent_intents
                .iter()
                .find(|intent| intent.intent_id == report.intent_id)
                .cloned()
                .ok_or_else(|| format!("unknown intent_id '{}'", report.intent_id))?;
            if report.symbol != expected.symbol || report.side != expected.side {
                return Err("execution report symbol/side does not match the intent".into());
            }
            if report.venue.trim().is_empty() {
                return Err("execution report venue is empty".into());
            }

            // Any valid report proves Node 4 persisted the idempotency key, so
            // this intent no longer needs replay from Node 3.
            inner
                .pending_intents
                .retain(|intent| intent.intent_id != report.intent_id);
            inner.work.execution_reports_received += 1;
            inner.work.last_execution_report_ts = Some(now_ms());
            inner.execution_reports.push_front(report.clone());
            while inner.execution_reports.len() > MAX_EXECUTION_REPORTS {
                inner.execution_reports.pop_back();
            }
            let event = Self::push_event_locked(
                &mut inner,
                if matches!(report.status.as_str(), "rejected" | "unknown") {
                    "warn"
                } else {
                    "info"
                },
                "node4",
                format!(
                    "Node 4 reported {} for intent {} on {}",
                    report.status, report.intent_id, report.venue
                ),
            );
            let signals = Self::build_signals_locked(
                &inner,
                self.node4_connected(),
                self.node4_token_configured,
            );
            let snapshot = Self::build_snapshot_locked(
                &inner,
                self.public_clients(),
                self.node4_connected(),
                self.node4_token_configured,
            );
            (event, signals, snapshot)
        };

        let _ = self.tx.send(WsFrame::Signals {
            data: signal_snapshot,
        });
        let _ = self.tx.send(WsFrame::DiagnosticEvent { data: event });
        let _ = self.tx.send(WsFrame::Diagnostics {
            data: Box::new(snapshot),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::INTENT_SCHEMA_VERSION;

    fn intent(id: &str) -> TradeIntent {
        let timestamp = now_ms();
        TradeIntent {
            schema_version: INTENT_SCHEMA_VERSION,
            intent_id: id.into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2_650.0,
            stop_loss: 2_647.0,
            take_profit: 2_656.0,
            risk_reward: 2.0,
            level_name: "PW PoC".into(),
            source_candle_time: timestamp,
            created_at: timestamp,
            expires_at: timestamp + 60_000,
        }
    }

    #[tokio::test]
    async fn execution_report_acknowledges_and_removes_pending_intent() {
        let config = Config {
            node4_shared_token: Some("test".into()),
            ..Config::default()
        };
        let hub = DiagnosticsHub::new(&config);
        assert!(hub.record_intent(intent("intent-1")).await);
        assert_eq!(hub.signal_snapshot().await.pending_count, 1);

        hub.record_execution_report(ExecutionReport {
            schema_version: INTENT_SCHEMA_VERSION,
            intent_id: "intent-1".into(),
            status: "accepted".into(),
            venue: "deriv_mt5_demo".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            timestamp: now_ms(),
            execution_id: None,
            filled_price: None,
            quantity: Some(0.01),
            quantity_unit: Some("lots".into()),
            error_code: None,
            error_message: None,
        })
        .await
        .unwrap();

        assert_eq!(hub.signal_snapshot().await.pending_count, 0);
    }

    #[tokio::test]
    async fn queue_refuses_untracked_overflow() {
        let config = Config {
            max_pending_intents: 1,
            ..Config::default()
        };
        let hub = DiagnosticsHub::new(&config);
        assert!(hub.record_intent(intent("intent-1")).await);
        assert!(!hub.record_intent(intent("intent-2")).await);
        assert_eq!(hub.signal_snapshot().await.pending_count, 1);
    }
}
