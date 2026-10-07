//! Node 4's half of the MT5 demo bridge integration.
//!
//! Topology (see `docs/mt5/EXECUTION_ARCHITECTURE.md`): the **bridge dials
//! out** to this service's private `WS /mt5/bridge` endpoint, authenticates with
//! `bridge_hello { token }`, and then pushes snapshots while answering commands.
//! Node 4 therefore never needs an inbound route to the terminal host and never
//! holds the MT5 account password.
//!
//! Three responsibilities live here:
//!
//! 1. [`Mt5BridgeLink`] — session registry, request/response correlation, the
//!    snapshot cache the `/mt5/*` resources read, and reconciliation lookups.
//! 2. [`Mt5Execution`] — the execution venue: pre-write health checks, one
//!    `mt5_order` carrying `intent_id` as the idempotency key, and the broker's
//!    outcome (never a fabricated fill).
//! 3. Reconciliation of an unclear write by `intent_id`/broker comment — never
//!    by resending the order.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, warn};

use crate::config::Config;
use crate::intent::{ExecutionStatus, ReconciliationOutcome, TradeIntent};
use crate::types::{
    BridgeErrorPayload, Mt5AccountSnapshot, Mt5BridgeStatus, Mt5OrderOutcome, Mt5PositionsSnapshot,
    Mt5RecentEvent, Mt5SnapshotState, WsFrame,
};

pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;

/// A bridge that has sent nothing for this long is treated as dead, even if the
/// TCP socket is still open. The bridge pushes account/position snapshots every
/// `MT5_PUSH_INTERVAL_MS` (2 s by default) plus heartbeats, so silence this long
/// means something is wrong.
pub const SESSION_STALE_MS: i64 = 15_000;
const PENDING_CAPACITY: usize = 128;
const MAX_RECENT_BRIDGE_EVENTS: usize = 30;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mt5LinkError {
    NotConnected,
    Timeout { what: String, timeout_ms: u64 },
    Malformed(String),
    Bridge { code: String, message: String },
    Refused(String),
}

impl std::fmt::Display for Mt5LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mt5LinkError::NotConnected => write!(
                f,
                "no MT5 bridge session is connected (the bridge dials out to this service; check \
                 NODE4_WS_URL and MT5_BRIDGE_TOKEN on the bridge)"
            ),
            Mt5LinkError::Timeout { what, timeout_ms } => {
                write!(f, "{what} timed out after {timeout_ms}ms")
            }
            Mt5LinkError::Malformed(msg) => write!(f, "malformed bridge frame: {msg}"),
            Mt5LinkError::Bridge { code, message } => {
                write!(f, "bridge refused the request ({code}): {message}")
            }
            Mt5LinkError::Refused(msg) => write!(f, "refused: {msg}"),
        }
    }
}

impl std::error::Error for Mt5LinkError {}

/// Bridge → Node 4 answer to one command.
#[derive(Debug, Clone)]
pub struct BridgeReply {
    pub req_id: String,
    pub ok: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<BridgeErrorPayload>,
}

impl BridgeReply {
    pub fn error_message(&self) -> String {
        match (&self.error, self.ok) {
            (Some(err), _) => format!("{}: {}", err.code, err.message),
            (None, false) => "bridge reported failure without an error body".into(),
            (None, true) => String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BridgeSessionInfo {
    pub token_ok: bool,
    pub protocol: u32,
    pub bridge_version: String,
    pub venue: String,
    pub capabilities: Vec<String>,
    pub connected_at: i64,
    pub last_message_at: i64,
    pub frames_received: u64,
    pub account: Option<Mt5AccountSnapshot>,
}

struct BridgeSession {
    generation: u64,
    tx: mpsc::Sender<String>,
}

struct Mt5LinkInner {
    session: RwLock<Option<BridgeSession>>,
    info: RwLock<BridgeSessionInfo>,
    state: RwLock<Mt5SnapshotState>,
    pending: tokio::sync::Mutex<HashMap<String, oneshot::Sender<BridgeReply>>>,
    next_req: AtomicU64,
    next_generation: AtomicU64,
}

/// Shared handle to the bridge session and its snapshot cache.
#[derive(Clone)]
pub struct Mt5BridgeLink {
    inner: Arc<Mt5LinkInner>,
    configured: bool,
    control_enabled: bool,
    symbol: String,
    volume_lots: f64,
    order_timeout_ms: u64,
}

impl Mt5BridgeLink {
    pub fn new(config: &Config) -> Self {
        let account = Mt5AccountSnapshot {
            configured: config.mt5_configured(),
            requested_symbol: config.mt5_symbol.clone(),
            error: if config.mt5_configured() {
                None
            } else {
                Some("MT5_BRIDGE_TOKEN is not set".into())
            },
            ..Default::default()
        };
        let status = Mt5BridgeStatus {
            configured: config.mt5_configured(),
            protocol: BRIDGE_PROTOCOL_VERSION,
            ..Default::default()
        };

        Self {
            inner: Arc::new(Mt5LinkInner {
                session: RwLock::new(None),
                info: RwLock::new(BridgeSessionInfo::default()),
                state: RwLock::new(Mt5SnapshotState {
                    account,
                    status,
                    ..Default::default()
                }),
                pending: tokio::sync::Mutex::new(HashMap::new()),
                next_req: AtomicU64::new(1),
                next_generation: AtomicU64::new(0),
            }),
            configured: config.mt5_configured(),
            control_enabled: config.mt5_control_enabled(),
            symbol: config.mt5_symbol.clone(),
            volume_lots: config.mt5_volume_lots,
            order_timeout_ms: config.mt5_order_timeout_ms,
        }
    }

    pub fn configured(&self) -> bool {
        self.configured
    }

    pub fn control_enabled(&self) -> bool {
        self.control_enabled
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn volume_lots(&self) -> f64 {
        self.volume_lots
    }

    pub fn order_timeout_ms(&self) -> u64 {
        self.order_timeout_ms
    }

    pub async fn connected(&self) -> bool {
        self.inner.session.read().await.is_some()
    }

    /// Age of the most recent frame from the bridge, in milliseconds.
    /// `None` means a session exists but has not proven itself with a frame yet.
    pub async fn last_frame_age_ms(&self) -> Option<i64> {
        let state = self.inner.state.read().await;
        state.last_bridge_frame.map(|ts| (now_ms() - ts).max(0))
    }

    /// Validate the `bridge_hello` token against `MT5_BRIDGE_TOKEN`.
    pub async fn authorize_hello(
        &self,
        presented: Option<&str>,
        expected: Option<&str>,
    ) -> Result<(), String> {
        let Some(expected) = expected else {
            return Err(
                "MT5_BRIDGE_TOKEN is not set on this service, so no bridge can be authenticated"
                    .into(),
            );
        };
        match presented {
            Some(token) if token == expected => Ok(()),
            Some(_) => Err("bridge presented an invalid MT5_BRIDGE_TOKEN".into()),
            None => Err("bridge_hello did not carry a token".into()),
        }
    }

    /// Register a freshly authenticated bridge session. A newer session
    /// replaces an older one (the bridge reconnects after a restart).
    pub async fn register_session(
        &self,
        info: BridgeSessionInfo,
    ) -> (u64, mpsc::Receiver<String>) {
        let generation = self.inner.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel::<String>(PENDING_CAPACITY);
        let previous = {
            let mut guard = self.inner.session.write().await;
            guard.replace(BridgeSession { generation, tx })
        };
        if previous.is_some() {
            warn!("a new MT5 bridge session replaced the previous one");
        }
        let protocol = info.protocol;
        let venue = info.venue.clone();
        let capability_count = info.capabilities.len();
        {
            let mut info_guard = self.inner.info.write().await;
            *info_guard = BridgeSessionInfo {
                connected_at: now_ms(),
                last_message_at: now_ms(),
                protocol: info.protocol,
                bridge_version: info.bridge_version,
                venue: info.venue,
                capabilities: info.capabilities,
                token_ok: info.token_ok,
                ..Default::default()
            };
        }
        info!(
            "MT5 bridge connected (protocol {protocol}, venue {venue}, {capability_count} capabilities)"
        );
        (generation, rx)
    }

    /// Release a session if it is still the active one.
    pub async fn unregister_session(&self, generation: u64) {
        let mut guard = self.inner.session.write().await;
        if guard.as_ref().map(|s| s.generation) == Some(generation) {
            *guard = None;
            drop(guard);
            // Fail every waiter instead of letting them hang.
            let mut pending = self.inner.pending.lock().await;
            for (_, reply) in pending.drain() {
                let _ = reply.send(BridgeReply {
                    req_id: String::new(),
                    ok: false,
                    data: None,
                    error: Some(BridgeErrorPayload {
                        code: "bridge_disconnected".into(),
                        message: "the MT5 bridge session ended".into(),
                        detail: None,
                    }),
                });
            }
        }
    }

    pub async fn account_snapshot(&self) -> Mt5AccountSnapshot {
        let state = self.inner.state.read().await;
        let mut account = state.account.clone();
        account.configured = self.configured;
        account.requested_symbol = self.symbol.clone();
        account
    }

    pub async fn positions_snapshot(&self) -> Mt5PositionsSnapshot {
        self.inner.state.read().await.positions.clone()
    }

    pub async fn status_snapshot(&self) -> Mt5BridgeStatus {
        let mut status = self.inner.state.read().await.status.clone();
        status.configured = self.configured;
        status.connected = self.connected().await;
        status
    }

    pub async fn recent_events(&self) -> Vec<Mt5RecentEvent> {
        self.inner.state.read().await.events.clone()
    }

    /// Apply a frame pushed by the bridge. Returns the frame that should be
    /// rebroadcast to frontend clients, if any.
    ///
    /// Every applied frame counts as proof of life (see `last_frame_age_ms`),
    /// whatever its type.
    pub async fn apply_frame(&self, frame: WsFrame) -> Option<WsFrame> {
        self.touch().await;
        match frame {
            WsFrame::Mt5Account { data } => {
                {
                    let mut state = self.inner.state.write().await;
                    state.account = data.clone();
                    state.account.configured = true;
                    state.status.connected = true;
                    state.status.authorized = data.authorized;
                    state.status.halted = data.halted;
                    state.status.halt_reason = data.halt_reason.clone();
                    state.status.ea_connected = data.connected;
                    state.status.ea_mode = Some(data.account_type.clone());
                    state.status.ea_login = data.login;
                    state.status.ea_server = data.server.clone();
                    state.status.ea_write_enabled = data.authorized;
                }
                Some(WsFrame::Mt5Account { data })
            }
            WsFrame::Mt5Positions { data } => {
                let mut state = self.inner.state.write().await;
                state.positions = data.clone();
                state.status.positions_open = data.count;
                Some(WsFrame::Mt5Positions { data })
            }
            WsFrame::Mt5History { data } => {
                let mut state = self.inner.state.write().await;
                state.history = data.clone();
                state.status.history_deals = data.count;
                Some(WsFrame::Mt5History { data })
            }
            WsFrame::BridgeStatus { data } => {
                {
                    let mut state = self.inner.state.write().await;
                    state.status = data.clone();
                    state.status.configured = true;
                }
                Some(WsFrame::BridgeStatus { data })
            }
            WsFrame::BridgeAck {
                req_id,
                ok,
                data,
                error,
            } => {
                let reply = BridgeReply {
                    req_id: req_id.clone(),
                    ok,
                    data,
                    error: error.clone(),
                };
                let waiter = {
                    let mut pending = self.inner.pending.lock().await;
                    pending.remove(&req_id)
                };
                if let Some(waiter) = waiter {
                    let _ = waiter.send(reply);
                } else {
                    warn!("bridge_ack for an unknown request id {req_id}");
                }
                None
            }
            WsFrame::BridgeEvent { event, data } => {
                let detail = data
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        data.get("reason")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| data.to_string());
                {
                    let mut state = self.inner.state.write().await;
                    state.events.insert(
                        0,
                        Mt5RecentEvent {
                            ts: now_ms(),
                            event: event.clone(),
                            detail,
                        },
                    );
                    state.events.truncate(MAX_RECENT_BRIDGE_EVENTS);
                }
                // Only events that describe broker state are worth rebroadcasting
                // on their own; the bridge also pushes authoritative snapshots.
                match event.as_str() {
                    "halted" | "resumed" | "order_filled" | "order_rejected" | "order_unknown"
                    | "ea_link_disconnected" => Some(WsFrame::BridgeEvent { event, data }),
                    _ => None,
                }
            }
            WsFrame::Heartbeat => {
                let mut info = self.inner.info.write().await;
                info.last_message_at = now_ms();
                None
            }
            other => {
                // Frames from other sources are not ours to interpret.
                let _ = other;
                None
            }
        }
    }

    async fn touch(&self) {
        let now = now_ms();
        {
            let mut state = self.inner.state.write().await;
            state.last_bridge_frame = Some(now);
            state.frames_received = state.frames_received.saturating_add(1);
        }
        let mut info = self.inner.info.write().await;
        info.last_message_at = now;
        info.frames_received = info.frames_received.saturating_add(1);
    }

    /// Send a command and await its `bridge_ack` within `timeout_ms`.
    pub async fn request(
        &self,
        frame: WsFrame,
        timeout_ms: u64,
    ) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = match &frame {
            WsFrame::Mt5Order { req_id, .. }
            | WsFrame::Mt5Modify { req_id, .. }
            | WsFrame::Mt5Close { req_id, .. }
            | WsFrame::Mt5CloseAll { req_id, .. }
            | WsFrame::Mt5Halt { req_id, .. }
            | WsFrame::Mt5Resume { req_id }
            | WsFrame::Mt5SnapshotRequest { req_id, .. }
            | WsFrame::Mt5Ping { req_id } => req_id.clone(),
            other => {
                return Err(Mt5LinkError::Malformed(format!(
                    "not a bridge command: {}",
                    serde_json::to_string(other).unwrap_or_default()
                )))
            }
        };

        let (tx, rx) = oneshot::channel();
        let sender = {
            let guard = self.inner.session.read().await;
            guard.as_ref().map(|session| session.tx.clone())
        };
        let Some(sender) = sender else {
            return Err(Mt5LinkError::NotConnected);
        };
        {
            let mut pending = self.inner.pending.lock().await;
            pending.insert(req_id.clone(), tx);
        }

        let text = match serde_json::to_string(&frame) {
            Ok(text) => text,
            Err(err) => {
                self.inner.pending.lock().await.remove(&req_id);
                return Err(Mt5LinkError::Malformed(err.to_string()));
            }
        };
        if sender.send(text).await.is_err() {
            self.inner.pending.lock().await.remove(&req_id);
            return Err(Mt5LinkError::NotConnected);
        }

        match tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(reply)) => {
                if reply.ok {
                    Ok(reply)
                } else {
                    Err(Mt5LinkError::Bridge {
                        code: reply
                            .error
                            .as_ref()
                            .map(|e| e.code.clone())
                            .unwrap_or_else(|| "bridge_error".into()),
                        message: reply.error_message(),
                    })
                }
            }
            Ok(Err(_)) => {
                self.inner.pending.lock().await.remove(&req_id);
                Err(Mt5LinkError::NotConnected)
            }
            Err(_) => {
                self.inner.pending.lock().await.remove(&req_id);
                Err(Mt5LinkError::Timeout {
                    what: format!("bridge command {req_id}"),
                    timeout_ms,
                })
            }
        }
    }

    /// Build the next request id. Deterministic, monotonic, no UUID dependency.
    pub fn next_request_id(&self, prefix: &str) -> String {
        let seq = self.inner.next_req.fetch_add(1, Ordering::SeqCst);
        format!("{prefix}-{}-{seq}", now_ms())
    }

    pub async fn ping(&self, timeout_ms: u64) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = self.next_request_id("ping");
        self.request(WsFrame::Mt5Ping { req_id }, timeout_ms).await
    }

    pub async fn request_snapshot(&self, what: &[&str]) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = self.next_request_id("snap");
        self.request(
            WsFrame::Mt5SnapshotRequest {
                req_id,
                what: Some(what.iter().map(|s| s.to_string()).collect()),
            },
            5_000,
        )
        .await
    }

    pub async fn halt(&self, reason: &str, flatten: bool) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = self.next_request_id("halt");
        self.request(
            WsFrame::Mt5Halt {
                req_id,
                reason: Some(reason.to_string()),
                flatten: Some(flatten),
            },
            10_000,
        )
        .await
    }

    pub async fn resume(&self) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = self.next_request_id("resume");
        self.request(WsFrame::Mt5Resume { req_id }, 10_000).await
    }

    pub async fn close_position(
        &self,
        position_ticket: i64,
        volume: Option<f64>,
    ) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = self.next_request_id("close");
        self.request(
            WsFrame::Mt5Close {
                req_id,
                position_ticket,
                volume,
            },
            20_000,
        )
        .await
    }

    pub async fn close_all(&self, reason: &str) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = self.next_request_id("closeall");
        self.request(
            WsFrame::Mt5CloseAll {
                req_id,
                reason: Some(reason.to_string()),
            },
            30_000,
        )
        .await
    }

    /// Place one market order carrying `idempotency_key` (the Node 3
    /// `intent_id`) and return the broker's outcome.
    ///
    /// The bridge refuses a second order for a key it has already answered, and
    /// marks an unclear outcome `unknown` — the caller must never resend.
    pub async fn place_order(
        &self,
        intent: Mt5OrderIntent,
        timeout_ms: Option<u64>,
    ) -> Result<Mt5OrderOutcome, Mt5LinkError> {
        let req_id = format!("order-{}", intent.idempotency_key);
        let frame = WsFrame::Mt5Order {
            req_id: req_id.clone(),
            idempotency_key: intent.idempotency_key.clone(),
            strategy_id: intent.strategy_id.clone(),
            symbol: Some(intent.symbol.clone()),
            side: intent.side.clone(),
            volume: Some(intent.volume),
            sl: Some(intent.sl),
            tp: Some(intent.tp),
            level_name: Some(intent.level_name.clone()),
            entry_ref: Some(intent.entry_ref),
            timeout_ms: Some(timeout_ms.unwrap_or(self.order_timeout_ms)),
        };
        let reply = self
            .request(frame, timeout_ms.unwrap_or(self.order_timeout_ms) + 5_000)
            .await?;
        let data = reply
            .data
            .ok_or_else(|| Mt5LinkError::Malformed("bridge_ack carried no outcome".into()))?;
        serde_json::from_value::<Mt5OrderOutcome>(data)
            .map_err(|err| Mt5LinkError::Malformed(err.to_string()))
    }

    /// Reconcile an unclear write by asking the bridge for positions and
    /// history and looking for this `intent_id`.
    ///
    /// The broker comment written by the bridge carries the intent id (see
    /// `mt5-bridge/src/terminal.rs::intent_comment`), so a position or deal
    /// proves the order reached the broker. Returns `None` when nothing is
    /// found — the outcome stays `unknown` and is **never** retried.
    pub async fn reconcile_intent(&self, intent_id: &str) -> Option<ReconciliationOutcome> {
        let reply = self
            .request_snapshot(&["positions", "history"])
            .await
            .ok()?;
        let data = reply.data?;
        reconcile_from_snapshot(&data, intent_id)
    }
}

/// True when a broker comment belongs to this intent.
///
/// The bridge writes the intent id into the MT5 comment, truncated to the
/// broker's limit (`mt5-bridge/src/terminal.rs::intent_comment`), so a prefix
/// match in either direction is the correct comparison.
pub fn comment_matches_intent(comment: &str, intent_id: &str) -> bool {
    let comment = comment.trim();
    if comment.is_empty() || intent_id.is_empty() {
        return false;
    }
    comment == intent_id || intent_id.starts_with(comment) || comment.starts_with(intent_id)
}

/// Interpret a `mt5_snapshot_request` payload (`{positions, history}`) as
/// evidence about one intent.
///
/// Returns `None` when the broker holds no trace of the intent — the outcome
/// stays `unknown` and is never retried.
pub fn reconcile_from_snapshot(
    data: &serde_json::Value,
    intent_id: &str,
) -> Option<ReconciliationOutcome> {
    // 1. An open position is the strongest evidence: the order filled.
    if let Some(positions) = data
        .get("positions")
        .and_then(|value| value.get("positions"))
        .and_then(|value| value.as_array())
    {
        for position in positions {
            let comment = position
                .get("comment")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if comment_matches_intent(comment, intent_id) {
                let ticket = position.get("ticket").and_then(|v| v.as_i64());
                return Some(ReconciliationOutcome {
                    status: ExecutionStatus::Filled,
                    source: "mt5_positions".into(),
                    detail: format!(
                        "broker position {} carries the intent comment",
                        ticket
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "?".into())
                    ),
                    execution_id: ticket.map(|t| format!("mt5-{t}")),
                    filled_price: position.get("price_open").and_then(|v| v.as_f64()),
                    quantity: position.get("volume").and_then(|v| v.as_f64()),
                    position_ticket: ticket,
                });
            }
        }
    }

    // 2. A deal proves execution even if the position is already gone.
    if let Some(deals) = data
        .get("history")
        .and_then(|value| value.get("deals"))
        .and_then(|value| value.as_array())
    {
        let mut best: Option<&serde_json::Value> = None;
        for deal in deals {
            let comment = deal
                .get("comment")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if comment_matches_intent(comment, intent_id) {
                // Prefer an entry (order-in) deal: it carries the fill.
                let entry = deal
                    .get("entry")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                if best.is_none() || entry == "in" {
                    best = Some(deal);
                }
            }
        }
        if let Some(deal) = best {
            let ticket = deal.get("ticket").and_then(|value| value.as_i64());
            let entry = deal
                .get("entry")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let closed = matches!(entry.as_str(), "out" | "out_by" | "inout");
            return Some(ReconciliationOutcome {
                status: if closed {
                    ExecutionStatus::Closed
                } else {
                    ExecutionStatus::Filled
                },
                source: "mt5_history".into(),
                detail: format!(
                    "broker deal {ticket:?} (entry '{entry}') carries the intent comment"
                ),
                execution_id: ticket.map(|t| format!("mt5-deal-{t}")),
                filled_price: deal.get("price").and_then(|value| value.as_f64()),
                quantity: deal.get("volume").and_then(|value| value.as_f64()),
                position_ticket: deal.get("position_ticket").and_then(|value| value.as_i64()),
            });
        }
    }

    None
}

/// A validated order request on its way to the bridge.
#[derive(Debug, Clone, PartialEq)]
pub struct Mt5OrderIntent {
    /// The Node 3 `intent_id`: the system-wide idempotency key.
    pub idempotency_key: String,
    pub strategy_id: String,
    pub symbol: String,
    pub side: String,
    pub volume: f64,
    pub sl: f64,
    pub tp: f64,
    pub level_name: String,
    pub entry_ref: f64,
}

/// Map a bridge outcome onto the protocol's execution status.
pub fn outcome_status(outcome: &Mt5OrderOutcome) -> ExecutionStatus {
    match outcome.status.as_str() {
        "filled" => ExecutionStatus::Filled,
        "partial" => ExecutionStatus::Partial,
        "rejected" => ExecutionStatus::Rejected,
        // timeout / unknown / anything unrecognised: the write may have landed.
        _ => ExecutionStatus::Unknown,
    }
}

/// Estimated money risk of an order in account currency, using the broker's
/// contract size. `None` when the contract size is unknown, which is itself a
/// reason to refuse (the bridge validates it authoritatively as well).
pub fn estimated_risk(
    entry: f64,
    sl: f64,
    volume: f64,
    contract_size: Option<f64>,
) -> Option<f64> {
    let contract_size = contract_size?;
    if !entry.is_finite() || !sl.is_finite() || !volume.is_finite() || contract_size <= 0.0 {
        return None;
    }
    Some((entry - sl).abs() * contract_size * volume)
}

/// MT5 execution venue used by [`crate::execution::ExecutionManager`].
pub struct Mt5Execution {
    pub link: Arc<Mt5BridgeLink>,
    pub config: Config,
}

impl Mt5Execution {
    pub fn new(config: &Config, link: Arc<Mt5BridgeLink>) -> Self {
        Self {
            link,
            config: config.clone(),
        }
    }

    /// Every reason the MT5 venue may not accept a broker write right now.
    ///
    /// This is the Node 4 half of a layered guard: the bridge and the EA repeat
    /// their own checks, so a stale Node 4 view can never place a live order on
    /// a non-demo account.
    pub async fn venue_health(&self) -> Result<(), VenueRefusal> {
        if !self.link.configured() {
            return Err(VenueRefusal::new(
                "mt5_not_configured",
                "MT5 venue selected but MT5_BRIDGE_TOKEN is not set — refusing to trade \
                 (no fallback to another venue)",
            ));
        }
        if !self.link.connected().await {
            return Err(VenueRefusal::new(
                "mt5_bridge_not_connected",
                "MT5 bridge is not connected — refusing to trade (the bridge dials out to this \
                 service; check NODE4_WS_URL / MT5_BRIDGE_TOKEN on the bridge)",
            ));
        }

        // A socket that is open but silent is not evidence of a healthy bridge:
        // the bridge pushes account state every couple of seconds.
        match self.link.last_frame_age_ms().await {
            Some(age) if age <= SESSION_STALE_MS => {}
            Some(age) => {
                return Err(VenueRefusal::new(
                    "mt5_bridge_stale",
                    format!(
                        "MT5 bridge has been silent for {age} ms (limit {SESSION_STALE_MS} ms) — \
                         refusing to trade on stale account state"
                    ),
                ))
            }
            None => {
                return Err(VenueRefusal::new(
                    "mt5_bridge_stale",
                    "MT5 bridge session has not sent any state yet — refusing to trade until it \
                     does",
                ))
            }
        }

        let account = self.link.account_snapshot().await;
        if !account.authorized {
            return Err(VenueRefusal::new(
                "mt5_not_authorized",
                format!(
                    "MT5 bridge reports authorized=false (EA link down or demo guard failed: {}) — \
                     refusing to trade",
                    account
                        .error
                        .clone()
                        .unwrap_or_else(|| "no reason reported".into())
                ),
            ));
        }
        if account.halted {
            return Err(VenueRefusal::new(
                "mt5_halted",
                format!(
                    "MT5 bridge is halted: {}",
                    account
                        .halt_reason
                        .clone()
                        .unwrap_or_else(|| "no reason reported".into())
                ),
            ));
        }
        if !account.account_type.eq_ignore_ascii_case("demo") {
            return Err(VenueRefusal::new(
                "mt5_not_demo",
                format!(
                    "MT5 account mode is '{}' — this venue executes demo accounts only",
                    account.account_type
                ),
            ));
        }
        Ok(())
    }

    /// Refuse an order whose stop implies more risk than `MT5_MAX_RISK_PER_TRADE`.
    pub async fn check_risk(&self, intent: &TradeIntent, volume: f64) -> Result<(), VenueRefusal> {
        let Some(max) = self.config.mt5_max_risk_per_trade else {
            return Ok(());
        };
        let account = self.link.account_snapshot().await;
        let risk = estimated_risk(
            intent.reference_price,
            intent.stop_loss,
            volume,
            account.symbol_contract_size,
        );
        // Without contract metadata there is no estimate at all: fail closed
        // rather than trading with an unverified size.
        let Some(risk) = risk else {
            return Err(VenueRefusal::new(
                "mt5_risk_unknown",
                "MT5_MAX_RISK_PER_TRADE is set but the broker contract size is not known yet — \
                 refusing to trade until the bridge reports it",
            ));
        };
        if risk > max {
            return Err(VenueRefusal::new(
                "mt5_risk_limit",
                format!(
                    "MT5 order risk {risk:.2} {} exceeds MT5_MAX_RISK_PER_TRADE {max:.2} — \
                     refused before sending",
                    account.currency
                ),
            ));
        }
        Ok(())
    }

    /// Send exactly one order for this intent and return the broker's outcome.
    ///
    /// `broker_symbol` is the explicitly mapped MT5 symbol (see
    /// [`crate::config::Config::resolve_symbol`]).
    pub async fn execute(
        &self,
        intent: &TradeIntent,
        broker_symbol: &str,
    ) -> Result<Mt5OrderOutcome, VenueRefusal> {
        self.venue_health().await?;

        let volume = self.config.mt5_volume_lots;
        self.check_risk(intent, volume).await?;

        let order = Mt5OrderIntent {
            idempotency_key: intent.intent_id.clone(),
            strategy_id: intent.strategy.clone(),
            symbol: broker_symbol.to_string(),
            side: intent.side.clone(),
            volume,
            sl: intent.stop_loss,
            tp: intent.take_profit,
            level_name: intent.level_name.clone(),
            entry_ref: intent.reference_price,
        };

        info!(
            "MT5 order intent {}: {} {:.2} lots {} (SL {:.2}, TP {:.2})",
            order.idempotency_key, order.side, order.volume, order.symbol, order.sl, order.tp
        );

        let outcome = self
            .link
            .place_order(order, Some(self.config.mt5_order_timeout_ms))
            .await
            .map_err(|err| {
                let code = match &err {
                    Mt5LinkError::Timeout { .. } => "mt5_order_timeout",
                    Mt5LinkError::NotConnected => "mt5_bridge_not_connected",
                    Mt5LinkError::Bridge { code, .. } => match code.as_str() {
                        "order_timeout" | "timeout" => "mt5_order_timeout",
                        _ => "mt5_order_rejected",
                    },
                    _ => "mt5_order_failed",
                };
                // A timeout means the write may have landed: the caller turns
                // this into `unknown` and reconciles. Anything the bridge
                // positively refused never reached the broker.
                VenueRefusal::new(code, format!("MT5 order failed: {err}"))
            })?;

        Ok(outcome)
    }

    /// Add [`Mt5OrderOutcome`] to the intent-independent close path.
    pub async fn close_all(&self, reason: &str) -> Result<usize, String> {
        let reply = self
            .link
            .close_all(reason)
            .await
            .map_err(|err| format!("close_all failed: {err}"))?;
        let closed = reply
            .data
            .as_ref()
            .and_then(|data| data.get("closed"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        Ok(closed)
    }
}

/// A refusal that happened **before** any broker write, so it is safe to report
/// as `rejected` (nothing unknown reached the broker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VenueRefusal {
    pub code: String,
    pub message: String,
}

impl VenueRefusal {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for VenueRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            mt5_bridge_token: Some("bridge-token".into()),
            mt5_control_token: Some("control-token".into()),
            mt5_symbol_map: vec![("XAUUSD".into(), "XAUUSD.a".into())],
            mt5_max_risk_per_trade: Some(5.0),
            execution_venue_override: Some("deriv_mt5_demo".into()),
            venue: crate::config::ExecutionVenue::DerivMt5Demo,
            ..Default::default()
        }
    }

    fn intent(side: &str) -> TradeIntent {
        TradeIntent {
            schema_version: 1,
            intent_id: "n3-xauusd-1-buy-pw-poc".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: side.into(),
            order_type: "market".into(),
            reference_price: 2_650.0,
            stop_loss: if side == "buy" { 2_648.0 } else { 2_652.0 },
            take_profit: if side == "buy" { 2_656.0 } else { 2_644.0 },
            risk_reward: 3.0,
            level_name: "PW PoC".into(),
            source_candle_time: 1_700_000_000_000,
            created_at: 1_700_000_000_100,
            expires_at: 1_700_000_120_100,
        }
    }

    #[test]
    fn risk_estimate_needs_the_broker_contract_size() {
        // 2 dollars of stop, 0.01 lots, 100 oz contract = 2.00 USD.
        assert_eq!(estimated_risk(2650.0, 2648.0, 0.01, Some(100.0)), Some(2.0));
        // Without contract metadata there is no estimate at all (fail closed).
        assert_eq!(estimated_risk(2650.0, 2648.0, 0.01, None), None);
        // A nonsense contract size is not usable either.
        assert_eq!(estimated_risk(2650.0, 2648.0, 0.01, Some(0.0)), None);

        let risk = estimated_risk(2650.0, 2645.0, 0.10, Some(100.0)).unwrap();
        assert!((risk - 50.0).abs() < 1e-9);
    }

    #[test]
    fn outcome_status_maps_the_bridge_contract() {
        let mut outcome = Mt5OrderOutcome {
            status: "filled".into(),
            ..Default::default()
        };
        assert_eq!(outcome_status(&outcome), ExecutionStatus::Filled);
        outcome.status = "partial".into();
        assert_eq!(outcome_status(&outcome), ExecutionStatus::Partial);
        outcome.status = "rejected".into();
        assert_eq!(outcome_status(&outcome), ExecutionStatus::Rejected);
        // Anything unclear is "unknown", never "rejected": the write may have
        // landed and must be reconciled instead of resent.
        for status in ["unknown", "timeout", "weird"] {
            outcome.status = status.into();
            assert_eq!(outcome_status(&outcome), ExecutionStatus::Unknown);
        }
    }

    #[test]
    fn link_defaults_are_unconfigured_and_fail_closed() {
        let mut cfg = config();
        cfg.mt5_bridge_token = None;
        let link = Mt5BridgeLink::new(&cfg);
        assert!(!link.configured());
        assert!(link.control_enabled());
        assert_eq!(link.volume_lots(), crate::config::DEFAULT_MT5_VOLUME_LOTS);
    }

    #[tokio::test]
    async fn hello_authorization_requires_a_matching_server_side_token() {
        let link = Mt5BridgeLink::new(&config());
        assert!(link
            .authorize_hello(Some("bridge-token"), Some("bridge-token"))
            .await
            .is_ok());
        assert!(link
            .authorize_hello(Some("wrong"), Some("bridge-token"))
            .await
            .is_err());
        assert!(link.authorize_hello(None, Some("bridge-token")).await.is_err());
        // No server-side token configured: nothing can authenticate.
        assert!(link.authorize_hello(Some("anything"), None).await.is_err());
    }

    #[tokio::test]
    async fn commands_without_a_session_fail_closed() {
        let link = Mt5BridgeLink::new(&config());
        assert!(!link.connected().await);
        let err = link.ping(500).await.unwrap_err();
        assert!(matches!(err, Mt5LinkError::NotConnected));

        let err = link
            .place_order(
                Mt5OrderIntent {
                    idempotency_key: "n3-xauusd-1-buy-pw-poc".into(),
                    strategy_id: "vp_break_retest_v1".into(),
                    symbol: "XAUUSD.a".into(),
                    side: "buy".into(),
                    volume: 0.01,
                    sl: 2648.0,
                    tp: 2656.0,
                    level_name: "PW PoC".into(),
                    entry_ref: 2650.0,
                },
                Some(500),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Mt5LinkError::NotConnected));
        // The message must point at the fix.
        assert!(err.to_string().contains("dials out"));
    }

    #[tokio::test]
    async fn the_order_path_refuses_everything_without_a_healthy_bridge() {
        let cfg = config();
        let link = Arc::new(Mt5BridgeLink::new(&cfg));
        let execution = Mt5Execution::new(&cfg, link);

        let refusal = execution.venue_health().await.unwrap_err();
        assert_eq!(refusal.code, "mt5_bridge_not_connected");

        let refusal = execution
            .execute(&intent("buy"), "XAUUSD.a")
            .await
            .unwrap_err();
        assert_eq!(refusal.code, "mt5_bridge_not_connected");

        // Unconfigured link (no MT5_BRIDGE_TOKEN) is refused even earlier.
        let mut bare = cfg.clone();
        bare.mt5_bridge_token = None;
        let bare_link = Arc::new(Mt5BridgeLink::new(&bare));
        let bare_execution = Mt5Execution::new(&bare, bare_link);
        assert_eq!(
            bare_execution.venue_health().await.unwrap_err().code,
            "mt5_not_configured"
        );
    }

    #[tokio::test]
    async fn account_frames_update_the_snapshot_cache() {
        let link = Mt5BridgeLink::new(&config());
        let account = Mt5AccountSnapshot {
            configured: true,
            connected: true,
            authorized: true,
            account_type: "demo".into(),
            login: Some(123_456),
            balance: Some(10_000.0),
            requested_symbol: "XAUUSD".into(),
            ..Default::default()
        };

        let rebroadcast = link
            .apply_frame(WsFrame::Mt5Account {
                data: account.clone(),
            })
            .await;
        assert!(matches!(rebroadcast, Some(WsFrame::Mt5Account { .. })));
        let cached = link.account_snapshot().await;
        assert_eq!(cached.account_type, "demo");
        assert_eq!(cached.login, Some(123_456));
        assert!(cached.configured);

        // A positions frame is cached separately from Node 4's own trades.
        let positions = Mt5PositionsSnapshot {
            positions: vec![Mt5Position {
                ticket: 111,
                symbol: "XAUUSD".into(),
                side: "buy".into(),
                volume: 0.01,
                comment: "n3-xauusd-1-buy-pw-poc".into(),
                magic: 330_033,
                ..Default::default()
            }],
            count: 1,
            ..Default::default()
        };
        link.apply_frame(WsFrame::Mt5Positions {
            data: positions.clone(),
        })
        .await;
        assert_eq!(link.positions_snapshot().await.count, 1);
        assert_eq!(link.status_snapshot().await.positions_open, 1);
    }

    #[tokio::test]
    async fn bridge_acks_resolve_pending_requests() {
        let link = Mt5BridgeLink::new(&config());
        let info = BridgeSessionInfo {
            token_ok: true,
            protocol: 1,
            bridge_version: "test".into(),
            venue: "deriv_mt5_demo".into(),
            capabilities: vec!["order_send".into()],
            ..Default::default()
        };
        let (generation, mut rx) = link.register_session(info).await;
        assert!(link.connected().await);

        let link_clone = link.clone();
        let caller = tokio::spawn(async move {
            link_clone
                .request(WsFrame::Mt5Ping { req_id: "ping-1".into() }, 2_000)
                .await
        });

        // The bridge receives the command and answers it.
        let command = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("command should arrive")
            .expect("channel open");
        assert!(command.contains("mt5_ping"));
        link.apply_frame(WsFrame::BridgeAck {
            req_id: "ping-1".into(),
            ok: true,
            data: Some(serde_json::json!({ "ts": 1 })),
            error: None,
        })
        .await;

        let reply = caller.await.unwrap().expect("ack resolves the request");
        assert!(reply.ok);

        // Ending the session fails subsequent commands closed.
        link.unregister_session(generation).await;
        assert!(!link.connected().await);
        assert!(matches!(
            link.ping(200).await.unwrap_err(),
            Mt5LinkError::NotConnected
        ));
    }

    #[tokio::test]
    async fn a_command_without_an_ack_times_out_without_hanging() {
        let link = Mt5BridgeLink::new(&config());
        let (generation, _rx) = link
            .register_session(BridgeSessionInfo {
                token_ok: true,
                ..Default::default()
            })
            .await;
        let err = link
            .request(WsFrame::Mt5Ping { req_id: "ping-2".into() }, 150)
            .await
            .unwrap_err();
        assert!(matches!(err, Mt5LinkError::Timeout { .. }));
        link.unregister_session(generation).await;
    }

    #[test]
    fn reconciliation_matches_a_broker_comment_by_intent_id() {
        let intent_id = "n3-xauusd-1-buy-pw-poc";
        // Comments are truncated by the bridge (MT5 limit), so a prefix match
        // must resolve the intent the same way.
        assert!(comment_matches_intent("n3-xauusd-1-buy-pw-poc", intent_id));
        assert!(comment_matches_intent("n3-xauusd-1-buy", intent_id));
        assert!(!comment_matches_intent("", intent_id));
        assert!(!comment_matches_intent("someone-elses-comment", intent_id));

        // An open position is the strongest evidence.
        let snapshot = serde_json::json!({
            "positions": {
                "positions": [{
                    "ticket": 4321,
                    "comment": "n3-xauusd-1-buy",
                    "price_open": 2650.25,
                    "volume": 0.01
                }]
            },
            "history": { "deals": [] }
        });
        let reconciled = reconcile_from_snapshot(&snapshot, intent_id).expect("position match");
        assert_eq!(reconciled.status, ExecutionStatus::Filled);
        assert_eq!(reconciled.source, "mt5_positions");
        assert_eq!(reconciled.position_ticket, Some(4321));
        assert_eq!(reconciled.filled_price, Some(2650.25));
        assert_eq!(reconciled.quantity, Some(0.01));

        // A closed deal still proves the order reached the broker.
        let snapshot = serde_json::json!({
            "positions": { "positions": [] },
            "history": {
                "deals": [{
                    "ticket": 99,
                    "position_ticket": 4321,
                    "comment": "n3-xauusd-1-buy-pw-poc",
                    "entry": "out",
                    "price": 2655.0,
                    "volume": 0.01
                }]
            }
        });
        let reconciled = reconcile_from_snapshot(&snapshot, intent_id).expect("deal match");
        assert_eq!(reconciled.status, ExecutionStatus::Closed);
        assert_eq!(reconciled.source, "mt5_history");
        assert_eq!(reconciled.execution_id, Some("mt5-deal-99".into()));

        // Nothing at all: the outcome stays unknown and is never retried.
        let empty = serde_json::json!({
            "positions": { "positions": [] },
            "history": { "deals": [] }
        });
        assert!(reconcile_from_snapshot(&empty, intent_id).is_none());
    }
}
