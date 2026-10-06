//! In-memory diagnostics shared by the HTTP resources, the Deriv monitor,
//! the MT5 bridge link, and the Node 3 link.
//!
//! Node 4 reports two independent health surfaces:
//!
//! * **Node 3 link** (`node3_link`) — the authenticated strategy handoff.
//!   Down here means "no new intents can arrive"; broker state is
//!   unaffected.
//! * **Broker health** (`mt5` / `deriv_account`) — the venues themselves.
//!
//! There is no market scanning, levels, or trigger state here: the `work`
//! section counts intent pipeline activity and ledger depth only.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::config::{Config, SERVICE_NAME};
use crate::execution_deriv::{deriv_setup_hint, token_kind_label};
use crate::execution_mt5::Mt5BridgeLink;
use crate::types::{
    ActivityLogEntry, DerivAccountSnapshot, DerivOpenContract, DiagnosticsSnapshot,
    EngineWorkDiagnostics, Mt5AccountSnapshot, Mt5BridgeStatus, Mt5Diagnostics,
    Mt5HistorySnapshot, Mt5PositionsSnapshot, Node3LinkDiagnostics, OpenTradesSnapshot, TradeEvent,
    WsFrame, now_ms,
};

const MAX_ACTIVITY: usize = 200;
const BROADCAST_CAPACITY: usize = 256;

#[derive(Default)]
struct Counters {
    intents_received: u64,
    intents_accepted: u64,
    intents_rejected: u64,
    intents_expired: u64,
    intents_duplicate: u64,
    trades_executed: u64,
    trades_failed: u64,
    reports_sent: u64,
    reports_acked: u64,
    reports_dropped: u64,
}

struct HubInner {
    counters: Counters,
    node3: Node3LinkDiagnostics,
    deriv: DerivAccountSnapshot,
    /// Trades Node 4 executed from Node 3 intents, keyed by `trade_id`.
    trades: HashMap<String, TradeEvent>,
    recent_trades: Vec<TradeEvent>,
    activity: Vec<ActivityLogEntry>,
    reports_pending: usize,
    ledger_depth: usize,
    last_error: Option<String>,
    ws_clients: usize,
}

#[derive(Clone)]
pub struct DiagnosticsHub {
    started_at: Instant,
    started_at_epoch_ms: i64,
    ledger_file: String,
    venue: String,
    mt5: Option<Arc<Mt5BridgeLink>>,
    /// Frames the public `/ws` clients should see (bridge state updates,
    /// trade events). Never bridge secrets: bridge command/ack frames are
    /// never broadcast.
    frame_tx: tokio::sync::broadcast::Sender<WsFrame>,
    inner: Arc<Mutex<HubInner>>,
}

impl DiagnosticsHub {
    pub fn new(config: &Config, mt5: Option<Arc<Mt5BridgeLink>>) -> Self {
        let token = config.deriv_demo_api.clone();
        let deriv = DerivAccountSnapshot {
            configured: config.deriv_configured(),
            connected: false,
            authorized: false,
            account_id: None,
            account_type: String::new(),
            balance: None,
            currency: String::new(),
            open_trades_count: 0,
            total_open_stake: 0.0,
            total_unrealized_pnl: 0.0,
            open_trades: Vec::new(),
            last_updated: None,
            error: None,
            app_id_configured: config.deriv_app_id.as_deref().map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            token_kind: token_kind_label(token.as_deref()).into(),
            setup_hint: deriv_setup_hint(token.as_deref(), config.deriv_app_id.as_deref()),
        };

        let node3 = Node3LinkDiagnostics {
            configured: config.node3_configured(),
            url: config.node3_ws_url.clone(),
            connected: false,
            state: if config.node3_configured() {
                "connecting".into()
            } else {
                "not_configured".into()
            },
            protocol_version: 1,
            ..Default::default()
        };

        let started_at = Instant::now();
        let (frame_tx, _) = tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
        Self {
            started_at_epoch_ms: now_ms(),
            started_at,
            ledger_file: config.execution_ledger_file.to_string_lossy().into_owned(),
            venue: config.execution_venue().label().to_string(),
            mt5,
            frame_tx,
            inner: Arc::new(Mutex::new(HubInner {
                counters: Counters::default(),
                node3,
                deriv,
                trades: HashMap::new(),
                recent_trades: Vec::new(),
                activity: Vec::new(),
                reports_pending: 0,
                ledger_depth: 0,
                last_error: None,
                ws_clients: 0,
            })),
        }
    }

    /// The MT5 bridge link shared with the execution venue, if configured.
    pub fn mt5(&self) -> Option<&Arc<Mt5BridgeLink>> {
        self.mt5.as_ref()
    }

    pub async fn push_event(&self, level: &str, category: &str, message: String) {
        let entry = ActivityLogEntry {
            timestamp: now_ms(),
            level: level.into(),
            category: category.into(),
            message,
        };
        let mut guard = self.inner.lock().await;
        guard.activity.insert(0, entry);
        guard.activity.truncate(MAX_ACTIVITY);
    }

    // -- Node 3 link ---------------------------------------------------------

    /// Mirror the link state for `/diagnostics`; `should_count` marks another
    /// reconnect (state went to a non-connected value).
    pub async fn set_node3_state(&self, connected: bool, state: &str, should_count: bool) {
        let mut guard = self.inner.lock().await;
        guard.node3.state = state.into();
        guard.node3.connected = connected;
        if should_count {
            guard.node3.reconnect_count += 1;
        }
    }

    pub async fn note_node3_message(&self) {
        let mut guard = self.inner.lock().await;
        guard.node3.last_msg_ts = Some(now_ms());
    }

    pub async fn note_node3_error(&self, message: &str) {
        let mut guard = self.inner.lock().await;
        guard.node3.last_error = Some(message.into());
    }

    pub async fn set_reports_pending(&self, pending: usize) {
        let mut guard = self.inner.lock().await;
        guard.reports_pending = pending;
    }

    // -- Report delivery -------------------------------------------------------

    pub async fn on_report_queued(&self, intent_id: &str, status: &str) {
        let mut guard = self.inner.lock().await;
        guard.reports_pending += 1;
    }

    pub async fn on_report_dropped(&self, intent_id: &str, status: &str) {
        let mut guard = self.inner.lock().await;
        guard.counters.reports_dropped += 1;
        guard.last_error = Some(format!(
            "report for intent {intent_id} ({status}) could not be queued"
        ));
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "error".into(),
            category: "report".into(),
            message: format!("report {} ({status}) dropped", intent_id),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    pub async fn on_report_ack(&self, intent_id: &str, accepted: bool) {
        let mut guard = self.inner.lock().await;
        if accepted {
            guard.counters.reports_acked += 1;
            guard.reports_pending = guard.reports_pending.saturating_sub(1);
        } else {
            guard.last_error = Some(format!(
                "Node 3 rejected the execution report for intent {intent_id}"
            ));
        }
    }

    pub async fn on_report_sent(&self) {
        let mut guard = self.inner.lock().await;
        guard.counters.reports_sent += 1;
        guard.reports_pending = guard.reports_pending.saturating_sub(1);
    }

    // -- Intent pipeline ------------------------------------------------------

    pub async fn on_intent_received(&self, intent: &crate::types::TradeIntent) {
        let mut guard = self.inner.lock().await;
        guard.counters.intents_received += 1;
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "info".into(),
            category: "intent".into(),
            message: format!(
                "intent {} received ({} {} ref={:.2})",
                intent.intent_id, intent.side, intent.symbol, intent.reference_price
            ),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    pub async fn on_intent_accepted(&self, intent_id: &str) {
        let mut guard = self.inner.lock().await;
        guard.counters.intents_accepted += 1;
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "info".into(),
            category: "intent".into(),
            message: format!("intent {intent_id} accepted"),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    pub async fn on_intent_rejected(&self, intent_id: &str, code: &str) {
        let mut guard = self.inner.lock().await;
        guard.counters.intents_rejected += 1;
        guard.last_error = Some(format!("intent {intent_id} rejected: {code}"));
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "warn".into(),
            category: "intent".into(),
            message: format!("intent {intent_id} rejected ({code})"),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    pub async fn on_intent_expired(&self, intent_id: &str) {
        let mut guard = self.inner.lock().await;
        guard.counters.intents_expired += 1;
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "warn".into(),
            category: "intent".into(),
            message: format!("intent {intent_id} expired"),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    pub async fn on_intent_duplicate(&self, intent_id: &str, status: &str) {
        let mut guard = self.inner.lock().await;
        guard.counters.intents_duplicate += 1;
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "warn".into(),
            category: "intent".into(),
            message: format!(
                "duplicate intent {intent_id} — replaying recorded state {status} (no second order)"
            ),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    // -- Trade events ----------------------------------------------------------

    /// A broker-confirmed fill reached the ledger (filled/partial).
    pub async fn on_trade_executed(&self, trade_id: &str, status: &str) {
        let mut guard = self.inner.lock().await;
        guard.counters.trades_executed += 1;
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "info".into(),
            category: "trade".into(),
            message: format!("trade {trade_id} {status}"),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    /// Register one trade Node 4 opened (from a broker-confirmed fill).
    /// Counting happens in `on_trade_executed`; this keeps the `/open-trades`
    /// view populated.
    pub async fn record_trade_opened(&self, trade: TradeEvent) {
        let mut guard = self.inner.lock().await;
        guard.trades.insert(trade.trade_id.clone(), trade.clone());
        guard.recent_trades.insert(0, trade.clone());
        guard.recent_trades.truncate(100);
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "info".into(),
            category: "trade".into(),
            message: format!(
                "trade {} opened ({}, size {:.2})",
                trade.trade_id,
                trade.venue.as_deref().unwrap_or("venue"),
                trade.size
            ),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    pub async fn on_trade_failed(&self, trade_id: &str, message: &str) {
        let mut guard = self.inner.lock().await;
        guard.counters.trades_failed += 1;
        guard.last_error = Some(format!("trade {trade_id} failed: {message}"));
        guard.activity.insert(0, ActivityLogEntry {
            timestamp: now_ms(),
            level: "error".into(),
            category: "trade".into(),
            message: format!("trade {trade_id} failed: {message}"),
        });
        guard.activity.truncate(MAX_ACTIVITY);
    }

    /// Update an open trade's status (e.g. closed, unknown, reconciled).
    pub async fn update_trade(
        &self,
        trade_id: &str,
        status: &str,
        closed_at: Option<i64>,
        note: Option<&str>,
    ) {
        let mut guard = self.inner.lock().await;
        if let Some(trade) = guard.trades.get_mut(trade_id) {
            trade.status = status.into();
            if let Some(at) = closed_at {
                trade.closed_at = Some(at);
            }
        }
        if let Some(note) = note {
            guard.activity.insert(0, ActivityLogEntry {
                timestamp: now_ms(),
                level: "info".into(),
                category: "trade".into(),
                message: format!("trade {trade_id} -> {status}: {note}"),
            });
            guard.activity.truncate(MAX_ACTIVITY);
        }
    }

    // -- Deriv account ----------------------------------------------------------

    pub async fn update_deriv_status(
        &self,
        connected: bool,
        authorized: bool,
        loginid: Option<String>,
        balance: Option<f64>,
        currency: Option<String>,
        error: Option<String>,
    ) {
        let mut guard = self.inner.lock().await;
        guard.deriv.connected = connected;
        guard.deriv.authorized = authorized;
        if let Some(id) = loginid {
            guard.deriv.account_id = Some(id);
        }
        if let Some(b) = balance {
            guard.deriv.balance = Some(b);
        }
        if let Some(c) = currency {
            guard.deriv.currency = c;
        }
        guard.deriv.error = error;
        guard.deriv.last_updated = Some(now_ms());
    }

    pub async fn update_deriv_balance(
        &self,
        balance: f64,
        currency: Option<String>,
        loginid: Option<String>,
    ) {
        let mut guard = self.inner.lock().await;
        guard.deriv.balance = Some(balance);
        if let Some(c) = currency {
            guard.deriv.currency = c;
        }
        if let Some(id) = loginid {
            guard.deriv.account_id = Some(id);
        }
        guard.deriv.last_updated = Some(now_ms());
    }

    pub async fn update_deriv_portfolio(&self, contracts: Vec<DerivOpenContract>) {
        let mut guard = self.inner.lock().await;
        let stake: f64 = contracts
            .iter()
            .filter(|c| c.status == "open" || c.status.is_empty())
            .map(|c| c.buy_price)
            .sum();
        let pnl: f64 = contracts
            .iter()
            .filter(|c| c.status == "open" || c.status.is_empty())
            .map(|c| c.profit)
            .sum();
        let open: Vec<DerivOpenContract> = contracts
            .iter()
            .filter(|c| c.status == "open" || c.status.is_empty())
            .cloned()
            .collect();
        guard.deriv.open_trades = open.clone();
        guard.deriv.open_trades_count = open.len();
        guard.deriv.total_open_stake = stake;
        guard.deriv.total_unrealized_pnl = pnl;
        guard.deriv.last_updated = Some(now_ms());
    }

    pub async fn upsert_deriv_contract(&self, contract: DerivOpenContract, is_closed: bool) {
        let mut guard = self.inner.lock().await;
        let idx = guard
            .deriv
            .open_trades
            .iter()
            .position(|c| c.contract_id == contract.contract_id);
        if is_closed {
            if let Some(i) = idx {
                guard.deriv.open_trades.remove(i);
                guard.deriv.open_trades_count = guard.deriv.open_trades.len();
                guard.activity.insert(0, ActivityLogEntry {
                    timestamp: now_ms(),
                    level: "info".into(),
                    category: "trade".into(),
                    message: format!("Deriv contract {} closed", contract.contract_id),
                });
                guard.activity.truncate(MAX_ACTIVITY);
            }
        } else {
            match idx {
                Some(i) => guard.deriv.open_trades[i] = contract,
                None => {
                    guard.deriv.open_trades.push(contract);
                    guard.deriv.open_trades_count = guard.deriv.open_trades.len();
                }
            }
        }
        guard.deriv.last_updated = Some(now_ms());
    }

    // -- WS client count ---------------------------------------------------------

    /// Track a public `/ws` client connecting; returns the new total.
    pub async fn client_connected(&self) -> usize {
        let mut guard = self.inner.lock().await;
        guard.ws_clients += 1;
        guard.ws_clients
    }

    /// Track a public `/ws` client disconnecting; returns the remaining total.
    pub async fn client_disconnected(&self) -> usize {
        let mut guard = self.inner.lock().await;
        guard.ws_clients = guard.ws_clients.saturating_sub(1);
        guard.ws_clients
    }

    pub async fn set_ledger_depth(&self, depth: usize) {
        let mut guard = self.inner.lock().await;
        guard.ledger_depth = depth;
    }

    // -- Frame broadcast (public /ws only) ---------------------------------------

    /// Subscribe for frames to push to a public `/ws` client.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<WsFrame> {
        self.frame_tx.subscribe()
    }

    /// Push a frame to every public `/ws` client. Bridge command/ack frames
    /// must never be pushed here.
    pub fn broadcast_frame(&self, frame: WsFrame) {
        // No subscribers yet: the sender still needs to stay alive, which the
        // struct holding `frame_tx` guarantees.
        let _ = self.frame_tx.send(frame);
    }

    /// Apply a frame the **bridge** sent on its private session to the shared
    /// MT5 state, then broadcast whatever should reach frontends.
    pub async fn apply_mt5_frame(&self, frame: WsFrame) -> Option<WsFrame> {
        let Some(link) = &self.mt5 else {
            return None;
        };
        let rebroadcast = link.apply_frame(frame).await;
        if let Some(frame) = &rebroadcast {
            self.broadcast_frame(frame.clone());
        }
        rebroadcast
    }

    // -- Resource snapshots (HTTP routes) ------------------------------------------

    pub async fn open_trades_snapshot(&self) -> OpenTradesSnapshot {
        let snap = self.snapshot().await;
        snap.open_trades
    }

    pub async fn deriv_snapshot(&self) -> DerivAccountSnapshot {
        let guard = self.inner.lock().await;
        guard.deriv.clone()
    }

    pub async fn mt5_account_snapshot(&self) -> Mt5AccountSnapshot {
        match &self.mt5 {
            Some(link) => link.account_snapshot().await,
            None => {
                let mut account = Mt5AccountSnapshot::default();
                account.configured = false;
                account.error = Some("MT5_BRIDGE_TOKEN is not set".into());
                account
            }
        }
    }

    pub async fn mt5_positions_snapshot(&self) -> Mt5PositionsSnapshot {
        match &self.mt5 {
            Some(link) => link.positions_snapshot().await,
            None => Mt5PositionsSnapshot::default(),
        }
    }

    pub async fn mt5_history_snapshot(&self) -> Mt5HistorySnapshot {
        match &self.mt5 {
            Some(link) => link.history_snapshot().await,
            None => Mt5HistorySnapshot::default(),
        }
    }

    pub async fn mt5_status_snapshot(&self) -> Mt5BridgeStatus {
        match &self.mt5 {
            Some(link) => link.status_snapshot().await,
            None => Mt5BridgeStatus::default(),
        }
    }

    // -- Snapshot -----------------------------------------------------------------

    pub async fn snapshot(&self) -> DiagnosticsSnapshot {
        let guard = self.inner.lock().await;
        let uptime = self.started_at.elapsed().as_secs();

        let mut node4_open: Vec<TradeEvent> = guard
            .trades
            .values()
            .filter(|t| t.status == "open" || t.status == "accepted" || t.status == "partial")
            .cloned()
            .collect();
        node4_open.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let mt5: Mt5Diagnostics = if let Some(link) = &self.mt5 {
            let state = link.snapshot_state().await;
            Mt5Diagnostics {
                account: state.account,
                positions: state.positions,
                history: state.history,
                status: state.status,
            }
        } else {
            Mt5Diagnostics::default()
        };

        DiagnosticsSnapshot {
            timestamp: now_ms(),
            node3_link: guard.node3.clone(),
            open_trades: OpenTradesSnapshot {
                node4_open_trades: node4_open.clone(),
                deriv_open_trades: guard.deriv.open_trades.clone(),
                mt5_open_positions: mt5.positions.positions.clone(),
                recent_trades: guard.recent_trades.clone(),
                total_open_count: node4_open.len()
                    + guard.deriv.open_trades_count
                    + mt5.positions.count,
                mt5_open_count: mt5.positions.count,
                timestamp: now_ms(),
            },
            deriv_account: guard.deriv.clone(),
            mt5,
            work: EngineWorkDiagnostics {
                service: SERVICE_NAME.into(),
                version: env!("CARGO_PKG_VERSION").into(),
                role: SERVICE_ROLE.into(),
                venue: self.venue.clone(),
                started_at: self.started_at_epoch_ms,
                uptime_secs: uptime,
                intents_received: guard.counters.intents_received,
                intents_accepted: guard.counters.intents_accepted,
                intents_rejected: guard.counters.intents_rejected,
                intents_expired: guard.counters.intents_expired,
                intents_duplicate: guard.counters.intents_duplicate,
                trades_executed: guard.counters.trades_executed,
                trades_failed: guard.counters.trades_failed,
                ledger_entries: guard.ledger_depth,
                ledger_file: self.ledger_file.clone(),
                last_error: guard.last_error.clone(),
                ws_clients_connected: guard.ws_clients,
            },
            recent_events: guard.activity.clone(),
        }
    }
}
