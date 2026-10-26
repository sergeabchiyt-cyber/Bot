use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};

use crate::config::Config;
use crate::feed::Freshness;
use crate::market::MarketBook;
use crate::strategy::CandleEvalReport;
use crate::types::{
    ActivityLogEntry, DiagnosticsSnapshot, DrawingsPayload, ExecutionReport, HealthSnapshot,
    ScanningSnapshot, SignalSnapshot, StrategyWorkDiagnostics, TradeIntent, VpCandle, VpLevels,
    WsFrame, INTENT_SCHEMA_VERSION,
};
use crate::vp::VpAudit;

const MAX_RECENT_EVENTS: usize = 50;
const MAX_RECENT_INTENTS: usize = 100;
const MAX_EXECUTION_REPORTS: usize = 100;
/// No Node 1 frame for this long means the feed is silent. Trading is then blocked.
pub const FEED_SILENCE_MS: i64 = 90_000;

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
    market: Arc<RwLock<MarketBook>>,
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
        let settings = config.window_settings();

        let scanning = ScanningSnapshot {
            active_setups_count: 0,
            total_levels_tracked: 0,
            proximity_dollars: crate::strategy::PROXIMITY_DOLLARS,
            invalidation_dollars: crate::strategy::INVALIDATION_DOLLARS,
            volume_filter_enabled: config.volume_filter_enabled(),
            volume_threshold: config.volume_threshold,
            current_volume: None,
            volume_confirmed: None,
            last_price: None,
            strategy_interval: config.strategy_interval.clone(),
            tradeable: false,
            freshness_state: "starting".into(),
            atr: None,
            atr_pips: None,
            sl_pips: None,
            tp_pips: None,
            rr: None,
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
            contract_version: crate::feed::CONTRACT_VERSION.into(),
            last_node1_msg_ts: None,
            last_candle_ts: None,
            last_tick_volume_ts: None,
            last_calendar_ts: None,
            ws_messages_received: 0,
            closed_bars_stored: 0,
            duplicate_bars_ignored: 0,
            open_bars_ignored: 0,
            replayed_bars: 0,
            tick_volume_frames: 0,
            last_tick_volume: None,
            calendar_items: 0,
            contract_violations: 0,
            last_violation: None,
            last_violation_ts: None,
            feed_state: "starting".into(),
            freshness_state: "starting".into(),
            freshness_age_ms: None,
            market_open: None,
            tradeable: false,
            store_series: Vec::new(),
            retention_days: config.retention_days,
            week_anchor: config.week_anchor.clone(),
            vp_ready: false,
            windows: Vec::new(),
            va_pct: settings.model.va_pct,
            vp_rows: settings.model.rows,
            vp_row_mode: settings.model.row_mode.as_str().into(),
            vp_bin_size: settings.model.bin_size,
            vp_lower_tf: settings.lower_tf.as_str().into(),
            ps_mode: settings.ps_mode.as_str().into(),
            cw_first_publish: config.cw_first_publish.clone(),
            strategy_interval: config.strategy_interval.clone(),
            last_price: None,
            last_candle: None,
            atr: None,
            atr_pips: None,
            current_sl_pips: None,
            current_tp_pips: None,
            current_rr: None,
            volume_filter_enabled: config.volume_filter_enabled(),
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
            market: Arc::new(RwLock::new(MarketBook::new(config))),
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

    pub async fn health_snapshot(&self) -> HealthSnapshot {
        let inner = self.inner.read().await;
        let work = &inner.work;
        let status = if work.node1_connected && work.vp_ready && work.freshness_state == "live" {
            "ok"
        } else if work.node1_connected {
            "degraded"
        } else {
            "disconnected"
        };
        let timestamp = now_ms();
        HealthSnapshot {
            status: status.into(),
            ok: status == "ok",
            service: work.service.clone(),
            version: work.version.clone(),
            timestamp,
            uptime_secs: ((timestamp - work.started_at).max(0) / 1_000) as u64,
            contract_version: work.contract_version.clone(),
            node1_connected: work.node1_connected,
            node1_state: work.node1_state.clone(),
            freshness_state: work.freshness_state.clone(),
            tradeable: work.tradeable,
            vp_ready: work.vp_ready,
            contract_violations: work.contract_violations,
            intents_pending: inner.pending_intents.len(),
            node4_connected: self.node4_connected(),
            node4_token_configured: self.node4_token_configured,
        }
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

    /// Whether trading is currently allowed, and the freshness label behind it.
    pub async fn trading_state(&self) -> (bool, String) {
        let inner = self.inner.read().await;
        (inner.work.tradeable, inner.work.freshness_state.clone())
    }

    pub async fn market_levels(&self) -> Vec<VpLevels> {
        self.market.read().await.levels()
    }

    pub async fn drawings_payload(&self) -> DrawingsPayload {
        self.market.read().await.drawings()
    }

    /// Published audit for `PW`/`PS`/`CW`, optionally rebuilt with `rows`.
    pub async fn market_audit(&self, window: &str, rows: Option<usize>) -> Option<VpAudit> {
        let market = self.market.read().await;
        match rows {
            Some(rows) => market.audit_with_rows(window, rows, now_ms()),
            None => market.audit(window),
        }
    }

    pub async fn custom_audit(
        &self,
        start: i64,
        end: i64,
        rows: Option<usize>,
    ) -> Result<VpAudit, String> {
        self.market.read().await.custom_audit(start, end, rows)
    }

    /// Frames a new public subscriber receives first, so it starts from a full view.
    pub async fn market_replay_frames(&self) -> Vec<WsFrame> {
        let market = self.market.read().await;
        let mut frames = vec![
            WsFrame::Levels {
                data: market.levels(),
            },
            WsFrame::Drawing {
                data: market.drawings(),
            },
        ];
        for window in ["PW", "PS", "CW"] {
            if let Some(audit) = market.audit(window) {
                frames.push(WsFrame::Vp {
                    data: Box::new(audit),
                });
            }
        }
        frames
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
            if !connected {
                // Without a live connection nothing can be trusted for trading.
                inner.work.tradeable = false;
                inner.work.freshness_state = "disconnected".into();
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

    /// A contract violation: the connection is dropped and the frame ignored.
    pub async fn record_violation(&self, message: &str) {
        let (event, snapshot) = {
            let mut inner = self.inner.write().await;
            let timestamp = now_ms();
            inner.work.contract_violations += 1;
            inner.work.last_violation = Some(message.to_string());
            inner.work.last_violation_ts = Some(timestamp);
            inner.work.tradeable = false;
            inner.work.freshness_state = "violation".into();
            let event = Self::push_event_locked(
                &mut inner,
                "error",
                "node1",
                format!("Node 1 contract violation, reconnecting: {message}"),
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

    /// Apply the freshness reported on a validated frame.
    pub async fn record_freshness(&self, freshness: &Freshness) {
        let mut inner = self.inner.write().await;
        inner.work.feed_state = "receiving".into();
        inner.work.freshness_state = freshness.state.clone();
        inner.work.freshness_age_ms = freshness.age_ms;
        inner.work.market_open = freshness.market_open;
        inner.work.tradeable = freshness.tradeable();
    }

    /// Block trading when the feed has gone quiet.
    pub async fn check_silence(&self, now: i64) {
        let mut inner = self.inner.write().await;
        let silent = match inner.work.last_node1_msg_ts {
            Some(last) => now - last > FEED_SILENCE_MS,
            None => false,
        };
        if silent && inner.work.freshness_state != "silent" && inner.work.node1_connected {
            inner.work.freshness_state = "silent".into();
            inner.work.tradeable = false;
            inner.work.feed_state = "silent".into();
            let _ = Self::push_event_locked(
                &mut inner,
                "warn",
                "node1",
                "No Node 1 frame for over 90 seconds; trading is blocked until the feed resumes".into(),
            );
        }
    }

    pub async fn record_tick_volume(&self, ticks: u64, last_tick_ms: Option<i64>) {
        let mut inner = self.inner.write().await;
        inner.work.tick_volume_frames += 1;
        inner.work.last_tick_volume = Some(ticks);
        inner.work.last_tick_volume_ts = last_tick_ms.or(Some(now_ms()));
    }

    pub async fn record_calendar(&self, items: usize) {
        let mut inner = self.inner.write().await;
        inner.work.calendar_items = items;
        inner.work.last_calendar_ts = Some(now_ms());
    }

    /// Store a validated closed bar and update the per-frame counters.
    pub async fn record_bar(
        &self,
        interval: &str,
        candle: &VpCandle,
        closed: bool,
        is_new: bool,
        replay: bool,
    ) {
        if !closed {
            self.inner.write().await.work.open_bars_ignored += 1;
            return;
        }
        if !is_new {
            self.inner.write().await.work.duplicate_bars_ignored += 1;
        }
        // Store every closed bar, including duplicates, so a corrected re-send overwrites.
        let stored = self.market.write().await.upsert_closed(interval, candle.clone());
        let mut inner = self.inner.write().await;
        if stored {
            inner.work.closed_bars_stored += 1;
        }
        if replay {
            inner.work.replayed_bars += 1;
        }
        if interval == inner.work.strategy_interval {
            inner.work.last_candle_ts = Some(candle.time);
            inner.work.last_candle = Some(candle.clone());
            inner.work.last_candle_volume = Some(candle.volume);
        }
    }

    /// Rebuild profiles. When anything visible changed, broadcast the market frames.
    pub async fn recompute_market(&self, now: i64) -> bool {
        let changed = self.market.write().await.recompute(now);
        let (series, windows, vp_ready) = {
            let market = self.market.read().await;
            (market.series_stats(), market.windows(), market.vp_ready())
        };
        {
            let mut inner = self.inner.write().await;
            inner.work.store_series = series;
            inner.work.windows = windows;
            inner.work.vp_ready = vp_ready;
        }
        if changed {
            for frame in self.market_replay_frames().await {
                let _ = self.tx.send(frame);
            }
        }
        changed
    }

    /// Record one evaluated closed bar. Trade intents are recorded separately.
    pub async fn record_evaluation(&self, scanning: ScanningSnapshot, report: &CandleEvalReport) {
        let (events, signal_snapshot, snapshot, scanning_frame) = {
            let mut inner = self.inner.write().await;
            inner.work.breaks_detected += report.breaks_detected;
            inner.work.breaks_invalidated += report.breaks_invalidated;
            inner.work.retests_rejected_low_volume += report.retests_rejected_low_volume;
            inner.work.signals_confirmed += report.signals_confirmed;
            inner.work.last_price = scanning.last_price;
            inner.work.atr = scanning.atr;
            inner.work.atr_pips = scanning.atr_pips;
            inner.work.current_sl_pips = scanning.sl_pips;
            inner.work.current_tp_pips = scanning.tp_pips;
            inner.work.current_rr = scanning.rr;
            inner.work.volume_ratio = match (scanning.current_volume, scanning.volume_threshold) {
                (Some(volume), threshold) if threshold > 0.0 => Some(volume / threshold),
                _ => None,
            };
            inner.work.tradeable = scanning.tradeable;
            inner.work.freshness_state = scanning.freshness_state.clone();

            let mut events = Vec::new();
            for (level, message) in &report.events {
                events.push(Self::push_event_locked(
                    &mut inner,
                    level,
                    "strategy",
                    message.clone(),
                ));
            }
            inner.scanning = scanning.clone();
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
            (events, signals, snapshot, scanning)
        };
        for event in events {
            let _ = self.tx.send(WsFrame::DiagnosticEvent { data: event });
        }
        let _ = self.tx.send(WsFrame::Scanning {
            data: scanning_frame,
        });
        let _ = self.tx.send(WsFrame::Signals {
            data: signal_snapshot,
        });
        let _ = self.tx.send(WsFrame::Diagnostics {
            data: Box::new(snapshot),
        });
    }

    /// Record a scanning refresh for a bar that did not evaluate (replay or stale).
    pub async fn record_scanning(&self, scanning: ScanningSnapshot) {
        let snapshot = {
            let mut inner = self.inner.write().await;
            inner.work.last_price = scanning.last_price;
            inner.work.atr = scanning.atr;
            inner.work.atr_pips = scanning.atr_pips;
            inner.work.current_sl_pips = scanning.sl_pips;
            inner.work.current_tp_pips = scanning.tp_pips;
            inner.work.current_rr = scanning.rr;
            inner.work.tradeable = scanning.tradeable;
            inner.work.freshness_state = scanning.freshness_state.clone();
            inner.scanning = scanning.clone();
            scanning
        };
        let _ = self.tx.send(WsFrame::Scanning { data: snapshot });
    }

    /// Record and publish an intent. Returns false when the intent was dropped
    /// because the queue was full, or suppressed as a duplicate `intent_id`.
    pub async fn record_intent(&self, intent: TradeIntent) -> bool {
        let (accepted, event, signal_snapshot, snapshot) = {
            let mut inner = self.inner.write().await;
            let timestamp = now_ms();
            inner
                .pending_intents
                .retain(|pending| !pending.is_expired_at(timestamp));

            let duplicate = inner
                .recent_intents
                .iter()
                .any(|known| known.intent_id == intent.intent_id);
            if duplicate {
                let event = Self::push_event_locked(
                    &mut inner,
                    "info",
                    "strategy",
                    format!("Intent {} already emitted; duplicate suppressed", intent.intent_id),
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
            } else if inner.pending_intents.len() >= inner.max_pending_intents {
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
                        "Emitted {} {} intent {} from {} (SL {:.2}, TP {:.2}, RR {:.2})",
                        intent.side.to_uppercase(),
                        intent.symbol,
                        intent.intent_id,
                        intent.level_name,
                        intent.stop_loss,
                        intent.take_profit,
                        intent.risk_reward
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
    use crate::types::{INTENT_SCHEMA_VERSION, STRATEGY_NAME, STRATEGY_VERSION};

    fn intent(id: &str) -> TradeIntent {
        let timestamp = now_ms();
        TradeIntent {
            schema_version: INTENT_SCHEMA_VERSION,
            intent_id: id.into(),
            strategy: STRATEGY_NAME.into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2_650.0,
            stop_loss: 2_648.0,
            take_profit: 2_656.0,
            risk_reward: 3.0,
            level_name: "PW PoC".into(),
            source_candle_time: timestamp,
            created_at: timestamp,
            expires_at: timestamp + 60_000,
            atr: None,
            atr_pips: None,
            level_id: None,
            drawing_id: None,
            strategy_version: Some(STRATEGY_VERSION.into()),
            volume_confirmed: None,
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
            venue: "node4_venue".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            timestamp: now_ms(),
            execution_id: None,
            filled_price: None,
            quantity: None,
            quantity_unit: None,
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

    #[tokio::test]
    async fn duplicate_intent_id_is_suppressed_not_queued_twice() {
        let hub = DiagnosticsHub::new(&Config::default());
        assert!(hub.record_intent(intent("same")).await);
        assert!(!hub.record_intent(intent("same")).await);
        assert_eq!(hub.signal_snapshot().await.pending_count, 1);
    }

    #[tokio::test]
    async fn silence_blocks_trading_until_the_feed_resumes() {
        let hub = DiagnosticsHub::new(&Config::default());
        hub.set_node1_state(true, "connected", false).await;
        hub.record_ws_message().await;
        hub.record_freshness(&Freshness {
            state: "live".into(),
            age_ms: Some(10),
            market_open: Some(true),
            last_tick_ms: None,
        })
        .await;
        assert!(hub.snapshot().await.work.tradeable);
        hub.check_silence(now_ms() + FEED_SILENCE_MS + 1_000).await;
        let work = hub.snapshot().await.work;
        assert!(!work.tradeable);
        assert_eq!(work.freshness_state, "silent");
    }

    #[tokio::test]
    async fn health_reports_degraded_until_the_feed_is_live_and_profiles_exist() {
        let hub = DiagnosticsHub::new(&Config::default());
        assert_eq!(hub.health_snapshot().await.status, "disconnected");
        hub.set_node1_state(true, "connected", false).await;
        assert_eq!(hub.health_snapshot().await.status, "degraded");
    }

    #[test]
    fn strategy_identity_is_versioned() {
        assert_eq!(STRATEGY_NAME, "vp_break_retest_v1");
        assert_eq!(STRATEGY_VERSION, "2.0.0");
    }
}
