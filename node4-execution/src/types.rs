//! Wire types for Node 4.
//!
//! Two separate contracts live here:
//!
//! 1. **Node 3 ↔ Node 4** (protocol version 1, see `docs/NODE3_NODE4_PROTOCOL.md`):
//!    `TradeIntent` (in), `ExecutionReport` (out), plus the hello/heartbeat
//!    frames of the authenticated `/execution` link.
//! 2. **Node 4 ↔ MT5 bridge** (bridge protocol 1, see
//!    `docs/mt5/EXECUTION_ARCHITECTURE.md`): the `WsFrame` variants that carry
//!    bridge snapshots/commands, which mirror `mt5-bridge/src/snapshot.rs`
//!    field-for-field. `ci/mt5/protocol_lint.py` fails the build if the two
//!    copies drift, which is why the names must stay identical.

use serde::{Deserialize, Serialize};

/// Intent schema version Node 4 executes. Unknown versions fail closed.
pub const TRADE_INTENT_SCHEMA_VERSION: u32 = 1;
/// Execution report schema version Node 4 emits.
pub const EXECUTION_REPORT_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Node 3 → Node 4: trade intent (schema version 1)
// ---------------------------------------------------------------------------

/// One trade intent from Node 3. The stop loss and take profit are **hard
/// constraints**: Node 4 may reject them, but it never re-derives or rewrites
/// the strategy's meaning. No venue, stake, or lot information travels here —
/// sizing is a Node 4 decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeIntent {
    pub schema_version: u32,
    /// The system-wide idempotency key.
    pub intent_id: String,
    #[serde(default)]
    pub strategy: String,
    pub symbol: String,
    /// Exactly `buy` or `sell`.
    pub side: String,
    /// Node 4 supports `market` intents; anything else is rejected.
    #[serde(default = "default_order_type")]
    pub order_type: String,
    pub reference_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub risk_reward: f64,
    #[serde(default)]
    pub level_name: String,
    #[serde(default)]
    pub source_candle_time: i64,
    #[serde(default)]
    pub created_at: i64,
    /// Epoch milliseconds; an intent is rejected when `now >= expires_at`.
    pub expires_at: i64,
}

fn default_order_type() -> String {
    "market".to_string()
}

// ---------------------------------------------------------------------------
// Node 4 → Node 3: execution report (schema version 1)
// ---------------------------------------------------------------------------

/// Allowed `status` values (protocol version 1).
pub mod report_status {
    pub const ACCEPTED: &str = "accepted";
    pub const FILLED: &str = "filled";
    pub const PARTIAL: &str = "partial";
    pub const REJECTED: &str = "rejected";
    pub const UNKNOWN: &str = "unknown";
    pub const CANCELLED: &str = "cancelled";
    pub const CLOSED: &str = "closed";
}

/// One execution report back to Node 3. Secrets, raw tokens, and broker
/// credentials are forbidden in this payload by contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub schema_version: u32,
    pub intent_id: String,
    /// One of `accepted` | `filled` | `partial` | `rejected` | `unknown` |
    /// `cancelled` | `closed`.
    pub status: String,
    /// The selected venue label (never empty).
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filled_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl ExecutionReport {
    pub fn new(status: &str, intent: &TradeIntent, venue: &str) -> Self {
        Self {
            schema_version: EXECUTION_REPORT_SCHEMA_VERSION,
            intent_id: intent.intent_id.clone(),
            status: status.to_string(),
            venue: venue.to_string(),
            symbol: intent.symbol.clone(),
            side: intent.side.clone(),
            timestamp: now_ms(),
            execution_id: None,
            filled_price: None,
            quantity: None,
            quantity_unit: None,
            error_code: None,
            error_message: None,
        }
    }

    pub fn with_error(mut self, code: impl Into<String>, message: impl Into<String>) -> Self {
        self.error_code = Some(code.into());
        self.error_message = Some(message.into());
        self
    }
}

/// Frame Node 4 sends as its **first** message on the `/execution` socket.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionHello {
    pub r#type: &'static str,
    pub token: String,
    pub service: &'static str,
    pub protocol_version: u32,
}

/// A frame received from Node 3 on the authenticated execution link.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum Node3Frame {
    #[serde(rename = "execution_hello_ack")]
    ExecutionHelloAck {
        accepted: bool,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        protocol_version: Option<u32>,
    },
    /// `data` is kept as a raw value so a malformed intent body can be
    /// rejected with a precise diagnostic instead of a serde panic.
    #[serde(rename = "trade_intent")]
    TradeIntent { data: serde_json::Value },
    #[serde(rename = "execution_report_ack")]
    ExecutionReportAck {
        #[serde(default)]
        intent_id: String,
        #[serde(default)]
        accepted: bool,
    },
    #[serde(rename = "heartbeat")]
    Heartbeat,
    #[serde(other)]
    Unknown,
}

/// A frame Node 4 sends on the execution link.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Node4Frame {
    ExecutionHello {
        token: String,
        service: String,
        protocol_version: u32,
    },
    ExecutionReport { data: ExecutionReport },
    #[serde(rename = "heartbeat")]
    Heartbeat,
}

// ---------------------------------------------------------------------------
// Broker account / position state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    pub trade_id: String,
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry: f64,
    pub sl: f64,
    pub tp: f64,
    pub status: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rr: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivOpenContract {
    pub contract_id: String,
    pub symbol: String,
    pub display_symbol: String,
    pub contract_type: String,
    pub side: String,
    pub buy_price: f64,
    pub bid_price: f64,
    pub payout: f64,
    pub entry_spot: Option<f64>,
    pub current_spot: Option<f64>,
    pub barrier: Option<String>,
    pub profit: f64,
    pub profit_pct: f64,
    pub currency: String,
    pub date_start: i64,
    pub date_expiry: Option<i64>,
    pub status: String,
    pub longcode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivAccountSnapshot {
    pub configured: bool,
    pub connected: bool,
    pub authorized: bool,
    pub account_id: Option<String>,
    pub account_type: String,
    pub balance: Option<f64>,
    pub currency: String,
    pub open_trades_count: usize,
    pub total_open_stake: f64,
    pub total_unrealized_pnl: f64,
    pub open_trades: Vec<DerivOpenContract>,
    pub last_updated: Option<i64>,
    pub error: Option<String>,
    /// True when `DERIV_APP_ID` is set — required for PAT (`pat_...`) tokens,
    /// which Deriv rejects on REST without a `Deriv-App-ID` header.
    #[serde(default)]
    pub app_id_configured: bool,
    /// Detected token shape: `"pat"`, `"legacy"` or `"none"`.
    #[serde(default)]
    pub token_kind: String,
    /// Actionable configuration hint naming the env var to fix.
    #[serde(default)]
    pub setup_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenTradesSnapshot {
    /// Trades Node 4 itself executed from Node 3 intents (intent-driven,
    /// idempotency-ledgered). Kept separate from the Deriv options contracts
    /// and the MT5 bridge positions, which are broker views of possibly the
    /// same trades.
    pub node4_open_trades: Vec<TradeEvent>,
    pub deriv_open_trades: Vec<DerivOpenContract>,
    /// Broker positions reported by the MT5 demo bridge.
    #[serde(default)]
    pub mt5_open_positions: Vec<Mt5Position>,
    pub recent_trades: Vec<TradeEvent>,
    pub total_open_count: usize,
    pub mt5_open_count: usize,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub timestamp: i64,
    pub level: String,
    pub category: String,
    pub message: String,
}

/// Node 3 link health — reported **separately** from broker/bridge health so
/// operators can tell "strategy handoff is down" from "broker is down".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Node3LinkDiagnostics {
    pub configured: bool,
    pub url: String,
    pub connected: bool,
    /// `not_configured` | `connecting` | `auth_rejected` | `connected` |
    /// `reconnecting` | `disconnected` | `stale`
    pub state: String,
    pub protocol_version: u32,
    pub reconnect_count: u64,
    pub last_msg_ts: Option<i64>,
    pub last_error: Option<String>,
    /// Reports queued while the link was down (bounded).
    pub reports_pending: usize,
    pub reports_sent: u64,
    pub reports_acked: u64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineWorkDiagnostics {
    pub service: String,
    pub version: String,
    /// Always `execution_only`: Node 4 never scans, levels, or triggers.
    pub role: String,
    pub venue: String,
    pub started_at: i64,
    pub uptime_secs: u64,
    pub intents_received: u64,
    pub intents_accepted: u64,
    pub intents_rejected: u64,
    pub intents_expired: u64,
    pub intents_duplicate: u64,
    pub trades_executed: u64,
    pub trades_failed: u64,
    pub ledger_entries: usize,
    pub ledger_file: String,
    pub last_error: Option<String>,
    pub ws_clients_connected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub timestamp: i64,
    /// Node 3 (strategy handoff) link health — separate from broker health.
    pub node3_link: Node3LinkDiagnostics,
    pub open_trades: OpenTradesSnapshot,
    pub deriv_account: DerivAccountSnapshot,
    /// MT5 demo bridge view (broker/bridge health).
    #[serde(default)]
    pub mt5: Mt5Diagnostics,
    pub work: EngineWorkDiagnostics,
    pub recent_events: Vec<ActivityLogEntry>,
}

// ---------------------------------------------------------------------------
// Public + bridge WebSocket frames
//
// The `bridge_*` / `mt5_*` variants are the wire contract shared with
// `mt5-bridge/src/snapshot.rs` (kept in sync by `ci/mt5/protocol_lint.py`).
// The public variants feed the read-only `/ws` diagnostics stream.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    #[serde(rename = "diagnostics")]
    Diagnostics { data: DiagnosticsSnapshot },

    #[serde(rename = "open_trades")]
    OpenTrades { data: OpenTradesSnapshot },

    #[serde(rename = "deriv_account")]
    DerivAccount { data: DerivAccountSnapshot },

    #[serde(rename = "trades")]
    Trades { data: TradeEvent },

    /// Non-secret execution report metadata, for operator observability.
    #[serde(rename = "execution_report")]
    ExecutionReport { data: ExecutionReport },

    #[serde(rename = "diagnostic_event")]
    DiagnosticEvent { data: ActivityLogEntry },

    // ---- MT5 demo bridge (see docs/mt5/EXECUTION_ARCHITECTURE.md) ----------
    /// First frame the bridge sends on `/mt5/bridge`; Node 4 validates
    /// `token` against `MT5_BRIDGE_TOKEN` before the connection becomes the
    /// bridge session.
    #[serde(rename = "bridge_hello")]
    BridgeHello {
        #[serde(default)]
        token: Option<String>,
        #[serde(default)]
        protocol: Option<u32>,
        #[serde(default)]
        bridge: Option<String>,
        #[serde(default)]
        venue: Option<String>,
        #[serde(default)]
        capabilities: Option<Vec<String>>,
        #[serde(default)]
        account: Option<serde_json::Value>,
    },

    /// Node 4's answer to `bridge_hello`.
    #[serde(rename = "bridge_hello_ack")]
    BridgeHelloAck {
        ok: bool,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        protocol: Option<u32>,
    },

    #[serde(rename = "bridge_ack")]
    BridgeAck {
        req_id: String,
        ok: bool,
        #[serde(default)]
        data: Option<serde_json::Value>,
        #[serde(default)]
        error: Option<BridgeErrorPayload>,
    },

    #[serde(rename = "bridge_event")]
    BridgeEvent {
        event: String,
        #[serde(default)]
        data: serde_json::Value,
    },

    #[serde(rename = "mt5_account")]
    Mt5Account { data: Mt5AccountSnapshot },

    #[serde(rename = "mt5_positions")]
    Mt5Positions { data: Mt5PositionsSnapshot },

    #[serde(rename = "mt5_history")]
    Mt5History { data: Mt5HistorySnapshot },

    #[serde(rename = "bridge_status")]
    BridgeStatus { data: Mt5BridgeStatus },

    // ---- commands Node 4 sends to the bridge -----------------------------
    #[serde(rename = "mt5_order")]
    Mt5Order {
        req_id: String,
        idempotency_key: String,
        #[serde(default)]
        strategy_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        symbol: Option<String>,
        side: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sl: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tp: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entry_ref: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },

    #[serde(rename = "mt5_modify")]
    Mt5Modify {
        req_id: String,
        position_ticket: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sl: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tp: Option<f64>,
    },

    #[serde(rename = "mt5_close")]
    Mt5Close {
        req_id: String,
        position_ticket: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<f64>,
    },

    #[serde(rename = "mt5_close_all")]
    Mt5CloseAll {
        req_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    #[serde(rename = "mt5_halt")]
    Mt5Halt {
        req_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flatten: Option<bool>,
    },

    #[serde(rename = "mt5_resume")]
    Mt5Resume { req_id: String },

    #[serde(rename = "mt5_snapshot_request")]
    Mt5SnapshotRequest {
        req_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        what: Option<Vec<String>>,
    },

    #[serde(rename = "mt5_ping")]
    Mt5Ping { req_id: String },

    #[serde(rename = "subscribe")]
    Subscribe {
        #[serde(default)]
        topics: Vec<String>,
    },

    #[serde(rename = "snapshot")]
    Snapshot,

    #[serde(rename = "heartbeat")]
    Heartbeat,

    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// MT5 demo bridge schema
//
// These mirror `mt5-bridge/src/snapshot.rs` field-for-field; the bridge is the
// source of truth and `ci/mt5/protocol_lint.py` fails the build when the two
// copies drift. Node 4 only ever *displays* them and reconciles them against
// its own ledger — it never invents broker state.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5AccountSnapshot {
    pub configured: bool,
    pub connected: bool,
    pub authorized: bool,
    pub account_type: String,
    pub login: Option<i64>,
    pub server: Option<String>,
    pub company: Option<String>,
    pub currency: String,
    pub balance: Option<f64>,
    pub equity: Option<f64>,
    pub margin: Option<f64>,
    pub margin_free: Option<f64>,
    pub leverage: Option<i64>,
    pub trade_allowed: bool,
    pub halted: bool,
    pub halt_reason: Option<String>,
    pub requested_symbol: String,
    pub broker_symbol: Option<String>,
    pub symbol_digits: Option<i32>,
    pub symbol_volume_min: Option<f64>,
    pub symbol_volume_max: Option<f64>,
    pub symbol_volume_step: Option<f64>,
    pub symbol_contract_size: Option<f64>,
    pub terminal_build: Option<i64>,
    pub ea_version: Option<String>,
    pub bridge_version: String,
    pub latency_ms: Option<i64>,
    pub last_heartbeat: Option<i64>,
    pub last_updated: Option<i64>,
    pub error: Option<String>,
    pub setup_hint: Option<String>,
}

impl Default for Mt5AccountSnapshot {
    fn default() -> Self {
        Self {
            configured: false,
            connected: false,
            authorized: false,
            account_type: "unconfigured".into(),
            login: None,
            server: None,
            company: None,
            currency: "USD".into(),
            balance: None,
            equity: None,
            margin: None,
            margin_free: None,
            leverage: None,
            trade_allowed: false,
            halted: false,
            halt_reason: None,
            requested_symbol: "XAUUSD".into(),
            broker_symbol: None,
            symbol_digits: None,
            symbol_volume_min: None,
            symbol_volume_max: None,
            symbol_volume_step: None,
            symbol_contract_size: None,
            terminal_build: None,
            ea_version: None,
            bridge_version: String::new(),
            latency_ms: None,
            last_heartbeat: None,
            last_updated: None,
            error: None,
            setup_hint: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Mt5Position {
    pub ticket: i64,
    pub symbol: String,
    pub side: String,
    pub volume: f64,
    pub price_open: f64,
    pub sl: f64,
    pub tp: f64,
    pub profit: f64,
    pub swap: f64,
    pub comment: String,
    pub magic: i64,
    pub time_ms: i64,
    #[serde(default)]
    pub current_price: Option<f64>,
    #[serde(default)]
    pub unrealized_pnl: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5PositionsSnapshot {
    pub positions: Vec<Mt5Position>,
    pub count: usize,
    pub total_volume: f64,
    pub total_unrealized_pnl: f64,
    pub halted: bool,
    pub halt_reason: Option<String>,
    pub account_login: Option<i64>,
    pub account_type: String,
    pub source: String,
    pub timestamp: i64,
}

impl Default for Mt5PositionsSnapshot {
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            count: 0,
            total_volume: 0.0,
            total_unrealized_pnl: 0.0,
            halted: false,
            halt_reason: None,
            account_login: None,
            account_type: "unconfigured".into(),
            source: "bridge".into(),
            timestamp: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mt5Deal {
    pub ticket: i64,
    pub order_ticket: i64,
    pub position_ticket: i64,
    pub symbol: String,
    pub side: String,
    pub volume: f64,
    pub price: f64,
    pub profit: f64,
    pub swap: f64,
    pub commission: f64,
    pub comment: String,
    pub magic: i64,
    pub time_ms: i64,
    pub entry: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5HistorySnapshot {
    pub deals: Vec<Mt5Deal>,
    pub count: usize,
    pub cursor: Option<String>,
    pub complete: bool,
    pub total_realized_pnl: f64,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
    pub source: String,
    pub timestamp: i64,
}

impl Default for Mt5HistorySnapshot {
    fn default() -> Self {
        Self {
            deals: Vec::new(),
            count: 0,
            cursor: None,
            complete: true,
            total_realized_pnl: 0.0,
            first_ms: None,
            last_ms: None,
            source: "bridge".into(),
            timestamp: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5BridgeStatus {
    pub configured: bool,
    pub connected: bool,
    pub authorized: bool,
    pub protocol: u32,
    pub bridge_version: String,
    pub node4_url: String,
    pub ea_connected: bool,
    pub ea_write_enabled: bool,
    pub ea_mode: Option<String>,
    pub ea_login: Option<i64>,
    pub ea_server: Option<String>,
    pub ea_last_heartbeat: Option<i64>,
    pub halted: bool,
    pub halt_reason: Option<String>,
    pub trading_enabled: bool,
    pub orders_sent: u64,
    pub orders_filled: u64,
    pub orders_rejected: u64,
    pub orders_unknown: u64,
    pub positions_open: usize,
    pub history_deals: usize,
    pub last_error: Option<String>,
    pub uptime_secs: u64,
    pub timestamp: i64,
}

impl Default for Mt5BridgeStatus {
    fn default() -> Self {
        Self {
            configured: false,
            connected: false,
            authorized: false,
            protocol: 1,
            bridge_version: String::new(),
            node4_url: String::new(),
            ea_connected: false,
            ea_write_enabled: false,
            ea_mode: None,
            ea_login: None,
            ea_server: None,
            ea_last_heartbeat: None,
            halted: false,
            halt_reason: None,
            trading_enabled: false,
            orders_sent: 0,
            orders_filled: 0,
            orders_rejected: 0,
            orders_unknown: 0,
            positions_open: 0,
            history_deals: 0,
            last_error: None,
            uptime_secs: 0,
            timestamp: 0,
        }
    }
}

/// Aggregated MT5 bridge view used by `/diagnostics`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Mt5Diagnostics {
    pub account: Mt5AccountSnapshot,
    pub positions: Mt5PositionsSnapshot,
    pub history: Mt5HistorySnapshot,
    pub status: Mt5BridgeStatus,
}

/// Broker outcome of one order intent (`bridge_ack.data` for `mt5_order`).
/// Mirrors `mt5-bridge/src/snapshot.rs::OrderOutcome`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5OrderOutcome {
    pub status: String,
    pub idempotency_key: String,
    pub intent_id: String,
    pub requested_symbol: String,
    pub broker_symbol: String,
    pub side: String,
    pub requested_volume: f64,
    pub filled_volume: f64,
    pub price: Option<f64>,
    pub sl: Option<f64>,
    pub tp: Option<f64>,
    pub order_ticket: Option<i64>,
    pub deal_ticket: Option<i64>,
    pub position_ticket: Option<i64>,
    pub retcode: i64,
    pub retcode_desc: String,
    pub risk_amount: Option<f64>,
    pub risk_currency: String,
    pub notional: Option<f64>,
    pub reconciled: bool,
    pub latency_ms: i64,
    pub ts: i64,
    pub error: Option<String>,
}

impl Default for Mt5OrderOutcome {
    fn default() -> Self {
        Self {
            status: "unknown".into(),
            idempotency_key: String::new(),
            intent_id: String::new(),
            requested_symbol: String::new(),
            broker_symbol: String::new(),
            side: String::new(),
            requested_volume: 0.0,
            filled_volume: 0.0,
            price: None,
            sl: None,
            tp: None,
            order_ticket: None,
            deal_ticket: None,
            position_ticket: None,
            retcode: 0,
            retcode_desc: String::new(),
            risk_amount: None,
            risk_currency: "USD".into(),
            notional: None,
            reconciled: false,
            latency_ms: 0,
            ts: 0,
            error: None,
        }
    }
}

impl Mt5OrderOutcome {
    /// A trade may only be marked opened on a broker-confirmed fill.
    pub fn is_confirmed_fill(&self) -> bool {
        matches!(self.status.as_str(), "filled" | "partial") && self.filled_volume > 0.0
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_intent() -> TradeIntent {
        TradeIntent {
            schema_version: 1,
            intent_id: "n3-xauusd-1700000000000-buy-pw-poc".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2650.25,
            stop_loss: 2647.45,
            take_profit: 2656.25,
            risk_reward: 2.142857,
            level_name: "PW PoC".into(),
            source_candle_time: 1_700_000_000_000,
            created_at: 1_700_000_000_100,
            expires_at: 1_700_000_120_100,
        }
    }

    #[test]
    fn trade_intent_round_trips_the_protocol_example() {
        let raw = r#"{
            "schema_version": 1,
            "intent_id": "n3-xauusd-1700000000000-buy-pw-poc",
            "strategy": "vp_break_retest_v1",
            "symbol": "XAUUSD",
            "side": "buy",
            "order_type": "market",
            "reference_price": 2650.25,
            "stop_loss": 2647.45,
            "take_profit": 2656.25,
            "risk_reward": 2.142857,
            "level_name": "PW PoC",
            "source_candle_time": 1700000000000,
            "created_at": 1700000000100,
            "expires_at": 1700000120100
        }"#;
        let intent: TradeIntent = serde_json::from_str(raw).unwrap();
        assert_eq!(intent.intent_id, "n3-xauusd-1700000000000-buy-pw-poc");
        assert_eq!(intent.reference_price, 2650.25);

        // A missing schema_version defaults to 0 and must fail closed in
        // validation, not silently pass.
        let raw_no_version = r#"{"intent_id":"x","symbol":"XAUUSD","side":"buy","reference_price":1,"stop_loss":0.5,"take_profit":1.5,"risk_reward":1,"expires_at":2}"#;
        let parsed: TradeIntent = serde_json::from_str(raw_no_version).unwrap();
        assert_eq!(parsed.schema_version, 0);
        assert_eq!(parsed.order_type, "market");
    }

    #[test]
    fn execution_report_serializes_with_optional_fields_skipped() {
        let intent = sample_intent();
        let report = ExecutionReport::new(report_status::ACCEPTED, &intent, "none");
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["status"], "accepted");
        assert!(value.get("execution_id").is_none());
        assert!(value.get("error_code").is_none());

        let mut filled = ExecutionReport::new(report_status::FILLED, &intent, "deriv_mt5_demo");
        filled.execution_id = Some("mt5-deal-456".into());
        filled.filled_price = Some(2650.28);
        filled.quantity = Some(0.01);
        filled.quantity_unit = Some("lots".into());
        let value = serde_json::to_value(&filled).unwrap();
        assert_eq!(value["execution_id"], "mt5-deal-456");
        assert_eq!(value["quantity_unit"], "lots");
    }

    #[test]
    fn node3_frames_parse_the_protocol_wire_names() {
        let ack: Node3Frame =
            serde_json::from_str(r#"{"type":"execution_hello_ack","accepted":true,"protocol_version":1}"#)
                .unwrap();
        assert!(matches!(
            ack,
            Node3Frame::ExecutionHelloAck {
                accepted: true,
                ..
            }
        ));

        let intent: Node3Frame = serde_json::from_str(
            r#"{"type":"trade_intent","data":{"intent_id":"x","schema_version":1}}"#,
        )
        .unwrap();
        assert!(matches!(intent, Node3Frame::TradeIntent { .. }));

        let report_ack: Node3Frame = serde_json::from_str(
            r#"{"type":"execution_report_ack","intent_id":"x","accepted":true}"#,
        )
        .unwrap();
        assert!(matches!(
            report_ack,
            Node3Frame::ExecutionReportAck { accepted: true, .. }
        ));

        let heartbeat: Node3Frame =
            serde_json::from_str(r#"{"type":"heartbeat"}"#).unwrap();
        assert!(matches!(heartbeat, Node3Frame::Heartbeat));

        let unknown: Node3Frame =
            serde_json::from_str(r#"{"type":"something_else"}"#).unwrap();
        assert!(matches!(unknown, Node3Frame::Unknown));
    }

    #[test]
    fn node4_hello_frame_has_the_contract_shape() {
        let frame = Node4Frame::ExecutionHello {
            token: "secret".into(),
            service: crate::config::SERVICE_NAME.into(),
            protocol_version: crate::config::EXECUTION_PROTOCOL_VERSION,
        };
        let value = serde_json::to_value(&frame).unwrap();
        assert_eq!(value["type"], "execution_hello");
        assert_eq!(value["token"], "secret");
        assert_eq!(value["service"], "xauusd-node4-execution");
        assert_eq!(value["protocol_version"], 1);
    }

    #[test]
    fn open_trades_snapshot_uses_the_node4_field_name() {
        let snap = OpenTradesSnapshot {
            node4_open_trades: Vec::new(),
            deriv_open_trades: Vec::new(),
            mt5_open_positions: Vec::new(),
            recent_trades: Vec::new(),
            total_open_count: 0,
            mt5_open_count: 0,
            timestamp: 0,
        };
        let value = serde_json::to_value(&snap).unwrap();
        assert!(value.get("node4_open_trades").is_some());
        assert!(value.get("node3_open_trades").is_none());
    }

    #[test]
    fn trade_event_serialization_keeps_backward_compatible_fields() {
        let trade = TradeEvent {
            trade_id: "t-1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            size: 0.01,
            entry: 2650.0,
            sl: 2647.5,
            tp: 2656.0,
            status: "open".into(),
            timestamp: 1_700_000_000_000,
            level_name: Some("PW PoC".into()),
            venue: Some("DerivDemo".into()),
            rr: Some(2.4),
            current_price: Some(2651.0),
            unrealized_pnl: Some(1.0),
            closed_at: None,
            intent_id: Some("n3-1".into()),
        };
        let frame = WsFrame::Trades { data: trade };
        let val = serde_json::to_value(&frame).unwrap();
        assert_eq!(val["type"], "trades");
        assert_eq!(val["data"]["trade_id"], "t-1");
        assert_eq!(val["data"]["intent_id"], "n3-1");
        assert!(val["data"].get("closed_at").is_none());
    }
}
