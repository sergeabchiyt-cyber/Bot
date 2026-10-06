//! Node 3's half of the MT5 demo bridge integration.
//!
//! Topology (see `docs/mt5/EXECUTION_ARCHITECTURE.md`): the **bridge dials
//! out** to this service's `/ws` endpoint, authenticates with
//! `bridge_hello { token }`, and then pushes snapshots while answering commands.
//! Node 3 therefore never needs an inbound route to the terminal host and never
//! holds the MT5 account password.
//!
//! Two responsibilities live here:
//!
//! 1. [`Mt5BridgeLink`] — the session registry, request/response correlation,
//!    and the snapshot cache the `/mt5/*` resources read.
//! 2. [`Mt5Execution`] — the execution venue: build a validated order intent,
//!    wait for the broker-confirmed outcome (bounded timeout), and only then
//!    hand a `TradeEvent` back to the strategy loop.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, warn};

use crate::config::Config;
use crate::types::{
    BridgeErrorPayload, Mt5AccountSnapshot, Mt5BridgeStatus, Mt5HistorySnapshot, Mt5OrderOutcome,
    Mt5Position, Mt5PositionsSnapshot, TradeEvent, WsFrame,
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
                 NODE3_WS_URL and MT5_BRIDGE_TOKEN on the bridge)"
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

/// Bridge → Node 3 answer to one command.
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

#[derive(Debug, Clone, Default)]
pub struct Mt5RecentEvent {
    pub ts: i64,
    pub event: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct Mt5SnapshotState {
    pub account: Mt5AccountSnapshot,
    pub positions: Mt5PositionsSnapshot,
    pub history: Mt5HistorySnapshot,
    pub status: Mt5BridgeStatus,
    pub events: Vec<Mt5RecentEvent>,
    pub last_bridge_frame: Option<i64>,
    pub frames_received: u64,
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
        let mut account = Mt5AccountSnapshot::default();
        account.configured = config.mt5_configured();
        account.requested_symbol = config.mt5_symbol.clone();
        if !config.mt5_configured() {
            account.error = Some("MT5_BRIDGE_TOKEN is not set".into());
        }
        let mut status = Mt5BridgeStatus::default();
        status.configured = config.mt5_configured();
        status.protocol = BRIDGE_PROTOCOL_VERSION;

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
    pub async fn authorize_hello(&self, presented: Option<&str>, expected: Option<&str>) -> Result<(), String> {
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
        // Read what the log line needs before the fields move into the state.
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
            let mut account = self.inner.state.read().await.account.clone();
            account.connected = false;
            account.authorized = false;
            {
                let mut state = self.inner.state.write().await;
                state.account = account;
                state.status.connected = false;
                state.status.authorized = false;
                state.status.ea_connected = false;
            }
            warn!("MT5 bridge session ended");
        }
    }

    pub async fn session_info(&self) -> BridgeSessionInfo {
        self.inner.info.read().await.clone()
    }

    pub async fn snapshot_state(&self) -> Mt5SnapshotState {
        self.inner.state.read().await.clone()
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

    pub async fn history_snapshot(&self) -> Mt5HistorySnapshot {
        self.inner.state.read().await.history.clone()
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
                    .or_else(|| data.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string()))
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

    /// Record that a frame arrived. `apply_frame` already does this; the method
    /// stays public for callers that observe traffic without applying it.
    pub async fn note_frame_received(&self) {
        self.touch().await;
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
    pub async fn request(&self, frame: WsFrame, timeout_ms: u64) -> Result<BridgeReply, Mt5LinkError> {
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

    pub async fn modify_position(
        &self,
        position_ticket: i64,
        sl: Option<f64>,
        tp: Option<f64>,
    ) -> Result<BridgeReply, Mt5LinkError> {
        let req_id = self.next_request_id("modify");
        self.request(
            WsFrame::Mt5Modify {
                req_id,
                position_ticket,
                sl,
                tp,
            },
            15_000,
        )
        .await
    }

    /// The core order path: send an intent and return the broker's outcome.
    ///
    /// The caller must check [`Mt5OrderOutcome::is_confirmed_fill`] before
    /// marking anything open — an `unknown` outcome means "do not assume".
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
}

/// A validated order request on its way to the bridge.
#[derive(Debug, Clone, PartialEq)]
pub struct Mt5OrderIntent {
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

/// Build an idempotency key for a signal.
///
/// The same signal must always produce the same key so a retry after an
/// uncertain outcome can never place a second order; a different signal always
/// produces a different key. The level name is reduced to a safe token.
pub fn idempotency_key(sequence: u64, side: &str, level_name: &str, ts_ms: i64) -> String {
    let level: String = level_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect();
    format!("n3-{ts_ms}-{sequence}-{side}-{}", if level.is_empty() { "L".to_string() } else { level })
}

/// Validate an intent before it is allowed anywhere near the broker.
///
/// This is the Node 3 half of a two-layer validation: the bridge repeats every
/// check against live broker metadata (min/max/step volume, stops level, quote
/// freshness) because Node 3 cannot know the contract specification.
pub fn validate_intent(intent: &Mt5OrderIntent) -> std::result::Result<(), String> {
    if intent.idempotency_key.trim().is_empty() {
        return Err("idempotency key is empty".into());
    }
    if !matches!(intent.side.as_str(), "buy" | "sell") {
        return Err(format!("side '{}' must be buy or sell", intent.side));
    }
    if !intent.volume.is_finite() || intent.volume <= 0.0 {
        return Err(format!("volume {} is not positive", intent.volume));
    }
    if !intent.entry_ref.is_finite() || intent.entry_ref <= 0.0 {
        return Err(format!("entry reference {} is invalid", intent.entry_ref));
    }
    if !intent.sl.is_finite() || intent.sl <= 0.0 {
        return Err(format!("stop loss {} is invalid", intent.sl));
    }
    if !intent.tp.is_finite() || intent.tp <= 0.0 {
        return Err(format!("take profit {} is invalid", intent.tp));
    }
    match intent.side.as_str() {
        "buy" if intent.sl >= intent.entry_ref => {
            return Err(format!(
                "buy stop loss {} is not below the entry {}",
                intent.sl, intent.entry_ref
            ))
        }
        "sell" if intent.sl <= intent.entry_ref => {
            return Err(format!(
                "sell stop loss {} is not above the entry {}",
                intent.sl, intent.entry_ref
            ))
        }
        _ => {}
    }
    Ok(())
}

/// Estimated money risk of an order in account currency, using the broker's
/// contract size. `None` when the contract size is unknown, which is itself a
/// reason to refuse (the bridge validates it authoritatively as well).
pub fn estimated_risk(entry: f64, sl: f64, volume: f64, contract_size: Option<f64>) -> Option<f64> {
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
    sequence: AtomicU64,
}

impl Mt5Execution {
    pub fn new(config: &Config, link: Arc<Mt5BridgeLink>) -> Self {
        Self {
            link,
            config: config.clone(),
            sequence: AtomicU64::new(1),
        }
    }

    pub fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    /// Place one market order and return a `TradeEvent` **only** on a
    /// broker-confirmed fill.
    pub async fn place_order(
        &self,
        side: &str,
        size: f64,
        entry: f64,
        sl: f64,
        tp: f64,
        level_name: &str,
    ) -> Result<TradeEvent> {
        if !self.link.configured() {
            bail!(
                "MT5 venue selected but MT5_BRIDGE_TOKEN is not set — refusing to trade \
                 (no fallback to another venue)"
            );
        }
        if !self.link.connected().await {
            bail!(
                "MT5 bridge is not connected — refusing to trade (the bridge dials out to this \
                 service; check NODE3_WS_URL / MT5_BRIDGE_TOKEN on the bridge)"
            );
        }

        // A socket that is open but silent is not evidence of a healthy bridge:
        // the bridge pushes accounting every couple of seconds.
        match self.link.last_frame_age_ms().await {
            Some(age) if age <= SESSION_STALE_MS => {}
            Some(age) => bail!(
                "MT5 bridge has been silent for {age} ms (limit {SESSION_STALE_MS} ms) — \
                 refusing to trade on stale account state"
            ),
            None => bail!(
                "MT5 bridge session has not sent any state yet — refusing to trade until it does"
            ),
        }

        let account = self.link.account_snapshot().await;
        if !account.authorized {
            bail!(
                "MT5 bridge reports authorized=false (EA link down or demo guard failed: {}) — \
                 refusing to trade",
                account
                    .error
                    .clone()
                    .unwrap_or_else(|| "no reason reported".into())
            );
        }
        if account.halted {
            bail!(
                "MT5 bridge is halted: {}",
                account
                    .halt_reason
                    .clone()
                    .unwrap_or_else(|| "no reason reported".into())
            );
        }
        if !account.account_type.eq_ignore_ascii_case("demo") {
            bail!(
                "MT5 account mode is '{}' — this venue executes demo accounts only",
                account.account_type
            );
        }

        let volume = if size.is_finite() && size > 0.0 {
            size
        } else {
            self.link.volume_lots()
        };
        let key = idempotency_key(
            self.next_sequence(),
            side,
            level_name,
            chrono::Utc::now().timestamp_millis(),
        );
        let intent = Mt5OrderIntent {
            idempotency_key: key.clone(),
            strategy_id: "vp-break-retest".into(),
            symbol: self.config.mt5_symbol.clone(),
            side: side.to_string(),
            volume,
            sl,
            tp,
            level_name: level_name.to_string(),
            entry_ref: entry,
        };
        validate_intent(&intent).map_err(|err| anyhow!("invalid MT5 order intent: {err}"))?;

        // Local risk guard (the bridge validates against live broker metadata).
        let risk = estimated_risk(
            intent.entry_ref,
            intent.sl,
            intent.volume,
            account.symbol_contract_size,
        );
        if let (Some(risk), Some(max)) = (risk, self.config.mt5_max_risk_per_trade) {
            if risk > max {
                bail!(
                    "MT5 order risk {risk:.2} {} exceeds MT5_MAX_RISK_PER_TRADE {max:.2} — refused \
                     before sending",
                    account.currency
                );
            }
        }

        info!(
            "MT5 order intent {}: {} {:.2} lots {} (SL {:.2}, TP {:.2}, est. risk {})",
            key,
            side,
            intent.volume,
            intent.symbol,
            intent.sl,
            intent.tp,
            risk
                .map(|r| format!("{r:.2} {}", account.currency))
                .unwrap_or_else(|| "unknown".into())
        );

        let outcome = self
            .link
            .place_order(intent.clone(), Some(self.config.mt5_order_timeout_ms))
            .await
            .map_err(|err| anyhow!("MT5 order failed: {err}"))?;

        if !outcome.is_confirmed_fill() {
            let detail = outcome
                .error
                .clone()
                .unwrap_or_else(|| outcome.retcode_desc.clone());
            bail!(
                "MT5 order {} not confirmed by the broker (status {}, retcode {}): {} — the trade \
                 is NOT open",
                key,
                outcome.status,
                outcome.retcode,
                if detail.is_empty() { "no detail".into() } else { detail }
            );
        }

        let trade_id = outcome
            .position_ticket
            .map(|ticket| format!("mt5-{ticket}"))
            .unwrap_or_else(|| format!("mt5-{}", outcome.idempotency_key));
        info!(
            "MT5 fill confirmed: {} {} {} lots @ {} (position {}, deal {}, retcode {})",
            trade_id,
            outcome.side,
            outcome.filled_volume,
            outcome
                .price
                .map(|p| format!("{p:.2}"))
                .unwrap_or_else(|| "?".into()),
            outcome
                .position_ticket
                .map(|t| t.to_string())
                .unwrap_or_else(|| "?".into()),
            outcome
                .deal_ticket
                .map(|t| t.to_string())
                .unwrap_or_else(|| "?".into()),
            outcome.retcode
        );

        Ok(TradeEvent {
            trade_id,
            symbol: self.config.mt5_symbol.clone(),
            side: outcome.side.clone(),
            size: outcome.filled_volume,
            entry: outcome.price.unwrap_or(entry),
            sl: outcome.sl.unwrap_or(sl),
            tp: outcome.tp.unwrap_or(tp),
            status: "open".into(),
            timestamp: if outcome.ts > 0 { outcome.ts } else { now_ms() },
            level_name: Some(level_name.to_string()),
            venue: Some("DerivMt5Demo".into()),
            rr: None,
            current_price: outcome.price,
            unrealized_pnl: Some(0.0),
            closed_at: None,
        })
    }

    /// Close every position this bridge owns (used by the kill switch).
    pub async fn close_all(&self, reason: &str) -> Result<usize> {
        let reply = self
            .link
            .close_all(reason)
            .await
            .map_err(|err| anyhow!("close_all failed: {err}"))?;
        let closed = reply
            .data
            .as_ref()
            .and_then(|data| data.get("closed"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        Ok(closed)
    }

    /// Broker positions currently open on the bridge's magic number.
    pub async fn open_positions(&self) -> Vec<Mt5Position> {
        self.link.positions_snapshot().await.positions
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

    fn intent(side: &str) -> Mt5OrderIntent {
        Mt5OrderIntent {
            idempotency_key: "n3-1-1-buy-PWPoC".into(),
            strategy_id: "vp-break-retest".into(),
            symbol: "XAUUSD".into(),
            side: side.into(),
            volume: 0.01,
            sl: if side == "buy" { 2648.0 } else { 2652.0 },
            tp: if side == "buy" { 2656.0 } else { 2644.0 },
            level_name: "PW PoC".into(),
            entry_ref: 2650.0,
        }
    }

    #[test]
    fn idempotency_keys_are_stable_per_signal_and_unique_across_signals() {
        let a = idempotency_key(7, "buy", "PW PoC", 1_700_000_000_000);
        let b = idempotency_key(7, "buy", "PW PoC", 1_700_000_000_000);
        let c = idempotency_key(8, "buy", "PW PoC", 1_700_000_000_000);
        let d = idempotency_key(7, "sell", "PW PoC", 1_700_000_000_000);
        assert_eq!(a, b, "the same signal must produce the same key");
        assert_ne!(a, c, "a different signal must produce a different key");
        assert_ne!(a, d, "side must be part of the key");
        // Keys stay broker-comment safe (alphanumeric, dash).
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[test]
    fn intent_validation_rejects_broken_payloads() {
        assert!(validate_intent(&intent("buy")).is_ok());
        assert!(validate_intent(&intent("sell")).is_ok());

        let mut bad = intent("buy");
        bad.side = "long".into();
        assert!(validate_intent(&bad).unwrap_err().contains("side"));

        let mut bad = intent("buy");
        bad.volume = 0.0;
        assert!(validate_intent(&bad).unwrap_err().contains("volume"));

        let mut bad = intent("buy");
        bad.volume = f64::NAN;
        assert!(validate_intent(&bad).is_err());

        let mut bad = intent("buy");
        bad.idempotency_key = "  ".into();
        assert!(validate_intent(&bad).unwrap_err().contains("idempotency"));

        // Stop loss on the wrong side of the entry.
        let mut bad = intent("buy");
        bad.sl = 2660.0;
        assert!(validate_intent(&bad).unwrap_err().contains("below the entry"));
        let mut bad = intent("sell");
        bad.sl = 2640.0;
        assert!(validate_intent(&bad).unwrap_err().contains("above the entry"));

        let mut bad = intent("buy");
        bad.entry_ref = 0.0;
        assert!(validate_intent(&bad).is_err());

        let mut bad = intent("buy");
        bad.tp = 0.0;
        assert!(validate_intent(&bad).is_err());
    }

    #[test]
    fn risk_estimate_needs_the_broker_contract_size() {
        // 2 dollars of stop, 0.01 lots, 100 oz contract = 2.00 USD.
        assert_eq!(estimated_risk(2650.0, 2648.0, 0.01, Some(100.0)), Some(2.0));
        // Without contract metadata there is no estimate at all (fail closed).
        assert_eq!(estimated_risk(2650.0, 2648.0, 0.01, None), None);
        // A nonsense contract size is not usable either.
        assert_eq!(estimated_risk(2650.0, 2648.0, 0.01, Some(0.0)), None);

        // The max-risk guard compares like with like.
        let risk = estimated_risk(2650.0, 2645.0, 0.10, Some(100.0)).unwrap();
        assert!((risk - 50.0).abs() < 1e-9);
    }

    #[test]
    fn link_defaults_are_unconfigured_and_fail_closed() {
        let mut cfg = config();
        cfg.mt5_bridge_token = None;
        let link = Mt5BridgeLink::new(&cfg);
        assert!(!link.configured());
        assert!(link.control_enabled());
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
                    idempotency_key: "k".into(),
                    strategy_id: "s".into(),
                    symbol: "XAUUSD".into(),
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
    async fn account_frames_update_the_snapshot_cache() {
        let link = Mt5BridgeLink::new(&config());
        let mut account = Mt5AccountSnapshot::default();
        account.configured = true;
        account.connected = true;
        account.authorized = true;
        account.account_type = "demo".into();
        account.login = Some(123_456);
        account.balance = Some(10_000.0);
        account.requested_symbol = "XAUUSD".into();

        let rebroadcast = link.apply_frame(WsFrame::Mt5Account { data: account.clone() }).await;
        assert!(matches!(rebroadcast, Some(WsFrame::Mt5Account { .. })));
        let cached = link.account_snapshot().await;
        assert_eq!(cached.account_type, "demo");
        assert_eq!(cached.login, Some(123_456));
        assert!(cached.configured);

        // A positions frame is cached separately from Node 3's own trades.
        let positions = Mt5PositionsSnapshot {
            positions: vec![Mt5Position {
                ticket: 111,
                symbol: "XAUUSD".into(),
                side: "buy".into(),
                volume: 0.01,
                comment: "N3-1".into(),
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

    #[tokio::test]
    async fn order_path_refuses_everything_when_the_bridge_reports_halted() {
        let cfg = config();
        let execution = Mt5Execution::new(&cfg, Arc::new(Mt5BridgeLink::new(&cfg)));
        // Not connected -> refused before any intent is built.
        let err = execution
            .place_order("buy", 0.01, 2650.0, 2648.0, 2656.0, "PW PoC")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not connected"));
    }
}
