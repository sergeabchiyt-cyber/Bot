//! Operator-facing state for the Node 4 execution service.
//!
//! Everything here is broker- or service-authoritative: the Node 3 **link**
//! health, the idempotency **ledger**, the MT5 **bridge** snapshot, the Deriv
//! options account monitor, and the trades Node 4 placed. There is deliberately
//! no market-data or strategy state — Node 4 never reproduces a strategy
//! decision.
//!
//! Link health and broker health are reported in separate sections
//! (`node3` vs `mt5`) so a dead strategy socket is never confused with a dead
//! broker session.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};

use crate::config::{Config, EXECUTION_PROTOCOL_VERSION};
use crate::execution_mt5::Mt5BridgeLink;
use crate::intent::{ExecutionReport, ExecutionStatus, IntentRejection, TradeIntent};
use crate::ledger::ExecutionLedger;
use crate::types::{
    ActivityLogEntry, DerivAccountSnapshot, DerivOpenContract, ExecutionSnapshot,
    ExecutionTrade, ExecutionWorkDiagnostics, Mt5AccountSnapshot, Mt5BridgeStatus,
    Mt5Diagnostics, Mt5HistorySnapshot, Mt5PositionsSnapshot, Node3LinkStatus,
    OpenTradesSnapshot, WsFrame,
};

const MAX_RECENT_EVENTS: usize = 50;
const MAX_RECENT_TRADES: usize = 50;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct DiagnosticsInner {
    node3: Node3LinkStatus,
    node4_open_trades: Vec<ExecutionTrade>,
    recent_trades: VecDeque<ExecutionTrade>,
    deriv_account: DerivAccountSnapshot,
    /// Latest MT5 bridge snapshots (the authoritative copy lives in the link;
    /// this one is what `/diagnostics` and `/open-trades` render).
    mt5_account: Mt5AccountSnapshot,
    mt5_positions: Mt5PositionsSnapshot,
    mt5_history: Mt5HistorySnapshot,
    mt5_status: Mt5BridgeStatus,
    work: ExecutionWorkDiagnostics,
    recent_events: VecDeque<ActivityLogEntry>,
}

#[derive(Clone)]
pub struct DiagnosticsHub {
    inner: Arc<RwLock<DiagnosticsInner>>,
    tx: broadcast::Sender<WsFrame>,
    ws_clients: Arc<AtomicUsize>,
    mt5: Arc<Mt5BridgeLink>,
    ledger: Arc<RwLock<Option<Arc<ExecutionLedger>>>>,
    ledger_cache: Arc<RwLock<LedgerCache>>,
}

#[derive(Debug, Clone, Default)]
struct LedgerCache {
    path: String,
    available: bool,
    records: u64,
    intents: usize,
}

impl DiagnosticsHub {
    pub fn new(config: &Config) -> Self {
        let (tx, _) = broadcast::channel(256);
        let started_at = now_ms();
        let mt5 = Arc::new(Mt5BridgeLink::new(config));

        // Seed the MT5 view from configuration so `/mt5/account` is meaningful
        // before the bridge ever connects (configured=false, clear error).
        let mt5_account = Mt5AccountSnapshot {
            configured: config.mt5_configured(),
            requested_symbol: config.mt5_symbol.clone(),
            error: if config.mt5_configured() {
                None
            } else {
                Some("MT5_BRIDGE_TOKEN is not set".into())
            },
            ..Default::default()
        };
        let mt5_status = Mt5BridgeStatus {
            configured: config.mt5_configured(),
            protocol: crate::execution_mt5::BRIDGE_PROTOCOL_VERSION,
            ..Default::default()
        };

        // Actionable hint (which env var to set) for /account and /diagnostics.
        let deriv_hint = crate::execution_deriv::deriv_setup_hint(
            config.deriv_demo_api.as_deref(),
            config.deriv_app_id.as_deref(),
        );
        let deriv_configured = config.deriv_configured();

        let deriv_account = DerivAccountSnapshot {
            configured: deriv_configured,
            connected: false,
            authorized: false,
            account_id: None,
            account_type: if deriv_configured {
                "demo".into()
            } else {
                "unconfigured".into()
            },
            balance: None,
            currency: "USD".into(),
            open_trades_count: 0,
            total_open_stake: 0.0,
            total_unrealized_pnl: 0.0,
            open_trades: Vec::new(),
            last_updated: None,
            error: if deriv_configured {
                None
            } else {
                Some("DERIV_DEMO_API not configured".into())
            },
            app_id_configured: config.deriv_app_id_configured(),
            token_kind: crate::execution_deriv::token_kind_label(config.deriv_demo_api.as_deref())
                .to_string(),
            setup_hint: deriv_hint,
        };

        let node3 = Node3LinkStatus {
            url: config.node3_ws_url.clone(),
            token_configured: config.node4_shared_token.is_some(),
            protocol_version: EXECUTION_PROTOCOL_VERSION,
            connected: false,
            authenticated: false,
            state: "starting".into(),
            ..Default::default()
        };

        let work = ExecutionWorkDiagnostics {
            service: "xauusd-node4-execution".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            role: "execution_only".into(),
            venue: config.execution_venue().label().into(),
            started_at,
            uptime_secs: 0,
            execution_protocol_version: EXECUTION_PROTOCOL_VERSION,
            ledger_path: config.execution_ledger_file.clone(),
            ledger_available: false,
            ledger_records: 0,
            ledger_intents: 0,
            intents_received: 0,
            intents_accepted: 0,
            intents_rejected: 0,
            intents_expired: 0,
            intents_duplicate: 0,
            orders_placed: 0,
            orders_filled: 0,
            orders_partial: 0,
            orders_rejected: 0,
            orders_unknown: 0,
            orders_reconciled: 0,
            last_intent_ts: None,
            last_report_ts: None,
            last_error: config.venue_selection_error().map(|err| err.to_string()),
            ws_clients_connected: 0,
        };

        let mut recent_events = VecDeque::with_capacity(MAX_RECENT_EVENTS);
        recent_events.push_front(ActivityLogEntry {
            timestamp: started_at,
            level: "info".into(),
            category: "system".into(),
            message: format!(
                "Node 4 execution started (venue: {}, protocol version {})",
                config.execution_venue().label(),
                EXECUTION_PROTOCOL_VERSION
            ),
        });

        Self {
            inner: Arc::new(RwLock::new(DiagnosticsInner {
                node3,
                node4_open_trades: Vec::new(),
                recent_trades: VecDeque::with_capacity(MAX_RECENT_TRADES),
                deriv_account,
                mt5_account,
                mt5_positions: Mt5PositionsSnapshot::default(),
                mt5_history: Mt5HistorySnapshot::default(),
                mt5_status,
                work,
                recent_events,
            })),
            tx,
            ws_clients: Arc::new(AtomicUsize::new(0)),
            mt5,
            ledger: Arc::new(RwLock::new(None)),
            ledger_cache: Arc::new(RwLock::new(LedgerCache::default())),
        }
    }

    /// Shared MT5 bridge link (session registry + snapshot cache).
    pub fn mt5(&self) -> Arc<Mt5BridgeLink> {
        self.mt5.clone()
    }

    /// Attach the durable ledger so `/diagnostics` reports its state.
    pub async fn attach_ledger(&self, ledger: Arc<ExecutionLedger>) {
        let stats = ledger.stats().await;
        {
            let mut guard = self.ledger.write().await;
            *guard = Some(ledger);
        }
        let mut cache = self.ledger_cache.write().await;
        cache.path = stats.path;
        cache.available = stats.available;
        cache.records = stats.records;
        cache.intents = stats.intents;
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsFrame> {
        self.tx.subscribe()
    }

    pub fn client_connected(&self) -> usize {
        self.ws_clients.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn client_disconnected(&self) -> usize {
        let prev = self.ws_clients.fetch_sub(1, Ordering::SeqCst);
        prev.saturating_sub(1)
    }

    pub fn connected_clients(&self) -> usize {
        self.ws_clients.load(Ordering::SeqCst)
    }

    async fn ledger_stats(&self) -> LedgerCache {
        if let Some(ledger) = self.ledger.read().await.clone() {
            let stats = ledger.stats().await;
            let mut cache = self.ledger_cache.write().await;
            cache.path = stats.path;
            cache.available = stats.available;
            cache.records = stats.records;
            cache.intents = stats.intents;
            return cache.clone();
        }
        self.ledger_cache.read().await.clone()
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

    fn build_open_trades_locked(inner: &DiagnosticsInner, ts: i64) -> OpenTradesSnapshot {
        let total_open_count = inner.node4_open_trades.len()
            + inner.deriv_account.open_trades.len()
            + inner.mt5_positions.positions.len();
        OpenTradesSnapshot {
            node4_open_trades: inner.node4_open_trades.clone(),
            deriv_open_trades: inner.deriv_account.open_trades.clone(),
            mt5_open_positions: inner.mt5_positions.positions.clone(),
            recent_trades: inner.recent_trades.iter().cloned().collect(),
            total_open_count,
            mt5_open_count: inner.mt5_positions.positions.len(),
            timestamp: ts,
        }
    }

    fn build_mt5_locked(inner: &DiagnosticsInner) -> Mt5Diagnostics {
        Mt5Diagnostics {
            account: inner.mt5_account.clone(),
            positions: inner.mt5_positions.clone(),
            history: inner.mt5_history.clone(),
            status: inner.mt5_status.clone(),
        }
    }

    fn build_work_locked(inner: &DiagnosticsInner, ws_clients: usize) -> ExecutionWorkDiagnostics {
        let ts = now_ms();
        let mut work = inner.work.clone();
        work.uptime_secs = ((ts - work.started_at).max(0) / 1000) as u64;
        work.ws_clients_connected = ws_clients;
        work
    }

    fn build_snapshot_locked(
        inner: &DiagnosticsInner,
        ws_clients: usize,
        ledger: &LedgerCache,
    ) -> ExecutionSnapshot {
        let ts = now_ms();
        // Link age is derived from the last accepted handshake, so an operator
        // can tell a freshly reconnected socket from a long-lived one.
        let mut node3 = inner.node3.clone();
        node3.session_age_secs = if node3.connected {
            node3
                .last_hello_ack_ts
                .map(|hello| ((ts - hello).max(0) / 1000) as u64)
        } else {
            None
        };
        let mut work = Self::build_work_locked(inner, ws_clients);
        work.ledger_path = ledger.path.clone();
        work.ledger_available = ledger.available;
        work.ledger_records = ledger.records;
        work.ledger_intents = ledger.intents;
        ExecutionSnapshot {
            timestamp: ts,
            node3,
            work,
            open_trades: Self::build_open_trades_locked(inner, ts),
            deriv_account: inner.deriv_account.clone(),
            mt5: Self::build_mt5_locked(inner),
            recent_events: inner.recent_events.iter().cloned().collect(),
        }
    }

    pub async fn snapshot(&self) -> ExecutionSnapshot {
        let ledger = self.ledger_stats().await;
        let inner = self.inner.read().await;
        Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger)
    }

    pub async fn open_trades_snapshot(&self) -> OpenTradesSnapshot {
        let inner = self.inner.read().await;
        Self::build_open_trades_locked(&inner, now_ms())
    }

    pub async fn deriv_snapshot(&self) -> DerivAccountSnapshot {
        let inner = self.inner.read().await;
        inner.deriv_account.clone()
    }


    // ---- Node 3 link health ----------------------------------------------

    pub async fn set_node3_state(&self, connected: bool, authenticated: bool, state: &str) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            inner.node3.connected = connected;
            inner.node3.authenticated = authenticated;
            inner.node3.state = state.into();
            if connected {
                inner.node3.reconnect_count = inner.node3.reconnect_count.saturating_add(1);
            }
            if !connected {
                inner.node3.session_age_secs = None;
            }
            Self::push_event_locked(
                &mut inner,
                if connected { "info" } else { "warn" },
                "node3",
                format!("Node 3 intent link: {state}"),
            );
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_node3_message(&self) {
        let mut inner = self.inner.write().await;
        inner.node3.last_message_ts = Some(now_ms());
        inner.node3.frames_received = inner.node3.frames_received.saturating_add(1);
    }

    pub async fn record_node3_hello_ack(&self, protocol_version: Option<u32>, accepted: bool) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            inner.node3.authenticated = accepted;
            if accepted {
                inner.node3.last_hello_ack_ts = Some(now_ms());
                if let Some(version) = protocol_version {
                    inner.work.execution_protocol_version = version;
                }
            }
            Self::push_event_locked(
                &mut inner,
                if accepted { "info" } else { "error" },
                "node3",
                if accepted {
                    "Node 3 accepted execution_hello".to_string()
                } else {
                    "Node 3 rejected execution_hello".to_string()
                },
            );
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_report_sent(&self) {
        let mut inner = self.inner.write().await;
        inner.node3.reports_sent = inner.node3.reports_sent.saturating_add(1);
        inner.node3.last_report_ts = Some(now_ms());
    }

    pub async fn record_report_ack(&self, accepted: bool) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            if accepted {
                inner.node3.reports_acked = inner.node3.reports_acked.saturating_add(1);
            }
            if !accepted {
                inner.node3.last_error = Some("Node 3 rejected an execution_report".into());
            }
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    // ---- intent / report accounting ---------------------------------------

    pub async fn record_intent_received(&self, intent: &TradeIntent) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            inner.work.intents_received = inner.work.intents_received.saturating_add(1);
            inner.work.last_intent_ts = Some(now_ms());
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
        let _ = intent;
    }

    pub async fn record_intent_accepted(&self, intent: &TradeIntent) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            inner.work.intents_accepted = inner.work.intents_accepted.saturating_add(1);
            Self::push_event_locked(
                &mut inner,
                "info",
                "execution",
                format!(
                    "Accepted intent {} ({} {}) — responsibility is durable",
                    intent.intent_id, intent.side, intent.symbol
                ),
            );
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_intent_rejected(&self, intent: &TradeIntent, rejection: &IntentRejection) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            if rejection.code == "intent_expired" {
                inner.work.intents_expired = inner.work.intents_expired.saturating_add(1);
            }
            inner.work.intents_rejected = inner.work.intents_rejected.saturating_add(1);
            Self::push_event_locked(
                &mut inner,
                "warn",
                "execution",
                format!(
                    "Rejected intent {}: {} ({})",
                    intent.intent_id, rejection.message, rejection.code
                ),
            );
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_duplicate_intent(&self, intent_id: &str, recorded_status: Option<&str>) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            inner.work.intents_duplicate = inner.work.intents_duplicate.saturating_add(1);
            Self::push_event_locked(
                &mut inner,
                "info",
                "execution",
                format!(
                    "Duplicate intent {} replayed — returning the recorded result ({}), no new \
                     order",
                    intent_id,
                    recorded_status.unwrap_or("accepted")
                ),
            );
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_report(&self, report: &ExecutionReport) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            match report.status {
                ExecutionStatus::Filled => {
                    inner.work.orders_filled = inner.work.orders_filled.saturating_add(1);
                }
                ExecutionStatus::Partial => {
                    inner.work.orders_partial = inner.work.orders_partial.saturating_add(1);
                }
                ExecutionStatus::Rejected => {
                    inner.work.orders_rejected = inner.work.orders_rejected.saturating_add(1);
                }
                ExecutionStatus::Unknown => {
                    inner.work.orders_unknown = inner.work.orders_unknown.saturating_add(1);
                    inner.work.last_error = report
                        .error_message
                        .clone()
                        .or_else(|| Some("unclear broker outcome".into()));
                }
                _ => {}
            }
            inner.work.last_report_ts = Some(report.timestamp);
            Self::push_event_locked(
                &mut inner,
                match report.status {
                    ExecutionStatus::Filled | ExecutionStatus::Partial => "signal",
                    ExecutionStatus::Rejected | ExecutionStatus::Unknown => "warn",
                    _ => "info",
                },
                "execution",
                format!(
                    "Report {} → {} (venue {}{})",
                    report.intent_id,
                    report.status.as_str(),
                    report.venue,
                    report
                        .error_code
                        .as_ref()
                        .map(|code| format!(", {code}"))
                        .unwrap_or_default()
                ),
            );
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_reconciled(&self) {
        let mut inner = self.inner.write().await;
        inner.work.orders_reconciled = inner.work.orders_reconciled.saturating_add(1);
    }

    pub async fn record_trade_opened(&self, trade: ExecutionTrade) {
        let (trade_clone, open_trades_snap, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.work.orders_placed = inner.work.orders_placed.saturating_add(1);
            inner.node4_open_trades.push(trade.clone());
            inner.recent_trades.push_front(trade.clone());
            while inner.recent_trades.len() > MAX_RECENT_TRADES {
                inner.recent_trades.pop_back();
            }
            let open_trades_snap = Self::build_open_trades_locked(&inner, ts);
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger);
            (trade, open_trades_snap, snap)
        };

        let _ = self.tx.send(WsFrame::Trades { data: trade_clone });
        let _ = self.tx.send(WsFrame::OpenTrades {
            data: open_trades_snap,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_order_failed(&self, code: &str, message: &str) {
        let (ledger, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            inner.work.last_error = Some(format!("{code}: {message}"));
            Self::push_event_locked(
                &mut inner,
                "error",
                "execution",
                format!("Order refused ({code}): {message}"),
            );
            (ledger, Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger))
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    // ---- Deriv options account monitor ------------------------------------

    pub async fn update_deriv_status(
        &self,
        connected: bool,
        authorized: bool,
        account_id: Option<String>,
        balance: Option<f64>,
        currency: Option<String>,
        error: Option<String>,
    ) {
        let (deriv_snap, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.deriv_account.connected = connected;
            inner.deriv_account.authorized = authorized;
            if let Some(acc) = account_id {
                inner.deriv_account.account_id = Some(acc);
            }
            if let Some(bal) = balance {
                inner.deriv_account.balance = Some(bal);
            }
            if let Some(curr) = currency {
                inner.deriv_account.currency = curr;
            }
            inner.deriv_account.error = error.clone();
            inner.deriv_account.last_updated = Some(ts);
            if let Some(err) = &error {
                inner.work.last_error = Some(err.clone());
                let message = err.clone();
                Self::push_event_locked(&mut inner, "error", "deriv", message);
            }
            let deriv_snap = inner.deriv_account.clone();
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger);
            (deriv_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn update_deriv_balance(
        &self,
        balance: f64,
        currency: Option<String>,
        account_id: Option<String>,
    ) {
        let (deriv_snap, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.deriv_account.connected = true;
            inner.deriv_account.authorized = true;
            inner.deriv_account.balance = Some(balance);
            if let Some(curr) = currency {
                inner.deriv_account.currency = curr;
            }
            if let Some(acc) = account_id {
                inner.deriv_account.account_id = Some(acc);
            }
            inner.deriv_account.error = None;
            inner.deriv_account.last_updated = Some(ts);

            let deriv_snap = inner.deriv_account.clone();
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger);
            (deriv_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn update_deriv_portfolio(&self, contracts: Vec<DerivOpenContract>) {
        let (deriv_snap, open_trades_snap, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.deriv_account.open_trades = contracts;
            Self::recompute_deriv_totals(&mut inner.deriv_account);
            inner.deriv_account.last_updated = Some(ts);

            let deriv_snap = inner.deriv_account.clone();
            let open_trades_snap = Self::build_open_trades_locked(&inner, ts);
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger);
            (deriv_snap, open_trades_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::OpenTrades {
            data: open_trades_snap,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn upsert_deriv_contract(&self, contract: DerivOpenContract, is_closed: bool) {
        let (deriv_snap, open_trades_snap, snap) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            let ts = now_ms();

            if is_closed {
                inner
                    .deriv_account
                    .open_trades
                    .retain(|c| c.contract_id != contract.contract_id);

                // Close the matching Node 4 trade record for this contract.
                if let Some(position) = inner
                    .node4_open_trades
                    .iter()
                    .position(|t| t.trade_id == contract.contract_id)
                {
                    let mut closed = inner.node4_open_trades.remove(position);
                    closed.status = contract.status.clone();
                    closed.current_price = contract.current_spot;
                    closed.unrealized_pnl = Some(contract.profit);
                    closed.closed_at = Some(ts);
                    if let Some(existing) = inner
                        .recent_trades
                        .iter_mut()
                        .find(|t| t.trade_id == closed.trade_id)
                    {
                        *existing = closed;
                    }
                }

                Self::push_event_locked(
                    &mut inner,
                    "info",
                    "deriv",
                    format!(
                        "Deriv contract #{} closed ({}, PnL: {:+.2} {})",
                        contract.contract_id, contract.status, contract.profit, contract.currency
                    ),
                );
            } else {
                if let Some(existing) = inner
                    .deriv_account
                    .open_trades
                    .iter_mut()
                    .find(|c| c.contract_id == contract.contract_id)
                {
                    *existing = contract.clone();
                } else {
                    inner.deriv_account.open_trades.push(contract.clone());
                }

                // Keep the Node 4 trade record's mark-to-market in step.
                if let Some(trade) = inner
                    .node4_open_trades
                    .iter_mut()
                    .find(|t| t.trade_id == contract.contract_id)
                {
                    if let Some(spot) = contract.current_spot {
                        trade.current_price = Some(spot);
                    }
                    trade.unrealized_pnl = Some(contract.profit);
                }
            }

            Self::recompute_deriv_totals(&mut inner.deriv_account);
            inner.deriv_account.last_updated = Some(ts);

            let deriv_snap = inner.deriv_account.clone();
            let open_trades_snap = Self::build_open_trades_locked(&inner, ts);
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger);
            (deriv_snap, open_trades_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::OpenTrades {
            data: open_trades_snap,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    fn recompute_deriv_totals(deriv: &mut DerivAccountSnapshot) {
        deriv.open_trades_count = deriv.open_trades.len();
        deriv.total_open_stake = deriv.open_trades.iter().map(|c| c.buy_price).sum();
        deriv.total_unrealized_pnl = deriv.open_trades.iter().map(|c| c.profit).sum();
    }

    // ---- MT5 bridge state -------------------------------------------------

    pub async fn mt5_account_snapshot(&self) -> Mt5AccountSnapshot {
        let inner = self.inner.read().await;
        inner.mt5_account.clone()
    }

    pub async fn mt5_positions_snapshot(&self) -> Mt5PositionsSnapshot {
        let inner = self.inner.read().await;
        inner.mt5_positions.clone()
    }

    pub async fn mt5_history_snapshot(&self) -> Mt5HistorySnapshot {
        let inner = self.inner.read().await;
        inner.mt5_history.clone()
    }

    pub async fn mt5_status_snapshot(&self) -> Mt5BridgeStatus {
        let inner = self.inner.read().await;
        inner.mt5_status.clone()
    }

    /// Apply a frame pushed by the bridge: update the display copies, then
    /// rebroadcast it (plus a fresh diagnostics snapshot) to frontend clients.
    ///
    /// The bridge itself is not a subscriber, so this never echoes back to it.
    pub async fn apply_mt5_frame(&self, frame: WsFrame) -> Option<WsFrame> {
        let rebroadcast = self.mt5.apply_frame(frame).await;
        let Some(frame) = rebroadcast else {
            return None;
        };

        let (extra, snapshot) = {
            let ledger = self.ledger_stats().await;
            let mut inner = self.inner.write().await;
            match &frame {
                WsFrame::Mt5Account { data } => {
                    inner.mt5_account = data.clone();
                    inner.mt5_status.connected = data.connected;
                    inner.mt5_status.authorized = data.authorized;
                    inner.mt5_status.halted = data.halted;
                    inner.mt5_status.halt_reason = data.halt_reason.clone();
                }
                WsFrame::Mt5Positions { data } => {
                    inner.mt5_positions = data.clone();
                    inner.mt5_status.positions_open = data.count;
                }
                WsFrame::Mt5History { data } => {
                    inner.mt5_history = data.clone();
                    inner.mt5_status.history_deals = data.count;
                }
                WsFrame::BridgeStatus { data } => {
                    inner.mt5_status = data.clone();
                }
                WsFrame::BridgeEvent { event, data } => {
                    let detail = data
                        .get("reason")
                        .or_else(|| data.get("error"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    Self::push_event_locked(
                        &mut inner,
                        "warn",
                        "mt5_bridge",
                        format!("MT5 bridge event: {event} {detail}").trim().to_string(),
                    );
                }
                _ => {}
            }

            // Bridge events move the open-trades view too (positions change);
            // mirror the change for clients subscribed to `open_trades` only.
            let extra = if matches!(frame, WsFrame::BridgeEvent { .. }) {
                Some(WsFrame::OpenTrades {
                    data: Self::build_open_trades_locked(&inner, now_ms()),
                })
            } else {
                None
            };
            let snapshot = Self::build_snapshot_locked(&inner, self.connected_clients(), &ledger);
            (extra, snapshot)
        };

        let _ = self.tx.send(frame.clone());
        if let Some(extra) = extra {
            let _ = self.tx.send(extra);
        }
        let _ = self.tx.send(WsFrame::Diagnostics { data: snapshot });
        Some(frame)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ExecutionTrade;

    fn config() -> Config {
        Config {
            node3_ws_url: "wss://strategy.example/execution".into(),
            node4_shared_token: Some("token".into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn the_snapshot_separates_the_node3_link_from_the_broker() {
        let hub = DiagnosticsHub::new(&config());
        hub.set_node3_state(true, true, "connected").await;
        let snapshot = hub.snapshot().await;

        assert_eq!(snapshot.work.role, "execution_only");
        assert_eq!(snapshot.work.service, "xauusd-node4-execution");
        assert!(snapshot.node3.connected);
        assert!(snapshot.node3.authenticated);
        assert_eq!(snapshot.node3.state, "connected");
        // Broker health is a separate section and stays inert.
        assert!(!snapshot.mt5.account.configured);
        assert_eq!(snapshot.work.venue, "none");
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["work"]["role"], "execution_only");
        assert!(json["node3"]["state"].is_string());
        assert!(json.get("scanning").is_none(), "no market state in Node 4");
    }

    #[tokio::test]
    async fn open_trades_are_execution_records_keyed_by_intent() {
        let hub = DiagnosticsHub::new(&config());
        hub.record_trade_opened(ExecutionTrade {
            trade_id: "mt5-42".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            size: 0.01,
            entry: 2650.0,
            sl: 2648.0,
            tp: 2654.0,
            status: "open".into(),
            timestamp: 1,
            intent_id: Some("n3-xauusd-1-buy-pw-poc".into()),
            level_name: None,
            venue: Some("deriv_mt5_demo".into()),
            rr: None,
            current_price: None,
            unrealized_pnl: None,
            closed_at: None,
        })
        .await;

        let snapshot = hub.open_trades_snapshot().await;
        assert_eq!(snapshot.node4_open_trades.len(), 1);
        assert_eq!(snapshot.total_open_count, 1);
        assert_eq!(
            snapshot.node4_open_trades[0].intent_id.as_deref(),
            Some("n3-xauusd-1-buy-pw-poc")
        );
        let json = serde_json::to_value(&snapshot).unwrap();
        assert!(json.get("node4_open_trades").is_some());
        assert!(json.get("node3_open_trades").is_none());
    }

    #[tokio::test]
    async fn report_counters_track_broker_outcomes() {
        let hub = DiagnosticsHub::new(&config());
        let intent = TradeIntent {
            schema_version: 1,
            intent_id: "n3-xauusd-1-buy-pw-poc".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2650.0,
            stop_loss: 2648.0,
            take_profit: 2654.0,
            risk_reward: 2.0,
            level_name: String::new(),
            source_candle_time: 0,
            created_at: 1,
            expires_at: 2,
        };
        hub.record_intent_received(&intent).await;
        hub.record_intent_accepted(&intent).await;
        hub.record_report(&ExecutionReport::new(
            &intent,
            ExecutionStatus::Filled,
            "none",
            3,
        ))
        .await;
        hub.record_report(&ExecutionReport::new(
            &intent,
            ExecutionStatus::Unknown,
            "none",
            4,
        ))
        .await;

        let work = hub.snapshot().await.work;
        assert_eq!(work.intents_received, 1);
        assert_eq!(work.intents_accepted, 1);
        assert_eq!(work.orders_filled, 1);
        assert_eq!(work.orders_unknown, 1);
        assert_eq!(work.last_report_ts, Some(4));
    }
}
