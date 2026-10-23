//! Wire and snapshot types owned by Node 4.
//!
//! Two contracts live here:
//!
//! * the **bridge** contract (`Mt5*` structs and the `bridge_*`/`mt5_*`
//!   frames) mirrored field-for-field from `mt5-bridge/src/snapshot.rs` — the
//!   bridge is the source of truth and `ci/mt5/protocol_lint.py` fails the build
//!   when the two copies drift;
//! * the **operator** contract (diagnostics, open trades, account snapshots)
//!   served on the read-only public resources.
//!
//! There is no market-data or strategy state in this service: Node 4 consumes
//! trade intents and reports broker facts only.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Execution-owned trade records
// ---------------------------------------------------------------------------

/// A trade Node 4 placed (or adopted) on a venue.
///
/// `intent_id` links the record back to the Node 3 intent that caused it; the
/// system-wide idempotency key is the same value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionTrade {
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
    pub intent_id: Option<String>,
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenTradesSnapshot {
    /// Trades this service placed on the selected venue.
    pub node4_open_trades: Vec<ExecutionTrade>,
    /// Deriv options contracts from the live account monitor.
    pub deriv_open_trades: Vec<DerivOpenContract>,
    /// Positions the MT5 bridge reports for this service's magic number.
    pub mt5_open_positions: Vec<Mt5Position>,
    pub recent_trades: Vec<ExecutionTrade>,
    pub total_open_count: usize,
    pub mt5_open_count: usize,
    pub timestamp: i64,
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityLogEntry {
    pub timestamp: i64,
    pub level: String,
    pub category: String,
    pub message: String,
}

/// Health of the **Node 3 intent link**, reported separately from broker and
/// bridge health: a dead strategy link must never be confused with a dead
/// broker connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node3LinkStatus {
    pub url: String,
    pub token_configured: bool,
    pub protocol_version: u32,
    pub connected: bool,
    pub authenticated: bool,
    /// `disconnected` | `connecting` | `awaiting_hello_ack` | `connected` | ...
    pub state: String,
    pub session_age_secs: Option<u64>,
    pub last_message_ts: Option<i64>,
    pub last_hello_ack_ts: Option<i64>,
    pub last_report_ts: Option<i64>,
    pub reconnect_count: u64,
    pub frames_received: u64,
    pub reports_sent: u64,
    pub reports_acked: u64,
    pub last_error: Option<String>,
}

impl Default for Node3LinkStatus {
    fn default() -> Self {
        Self {
            url: String::new(),
            token_configured: false,
            protocol_version: crate::config::EXECUTION_PROTOCOL_VERSION,
            connected: false,
            authenticated: false,
            state: "starting".into(),
            session_age_secs: None,
            last_message_ts: None,
            last_hello_ack_ts: None,
            last_report_ts: None,
            reconnect_count: 0,
            frames_received: 0,
            reports_sent: 0,
            reports_acked: 0,
            last_error: None,
        }
    }
}

/// Node 4's own work counters. Execution-only: no strategy or market counters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionWorkDiagnostics {
    pub service: String,
    pub version: String,
    /// Always `execution_only` — this service never makes strategy decisions.
    pub role: String,
    pub venue: String,
    pub started_at: i64,
    pub uptime_secs: u64,
    pub execution_protocol_version: u32,
    pub ledger_path: String,
    pub ledger_available: bool,
    pub ledger_records: u64,
    pub ledger_intents: usize,
    pub intents_received: u64,
    pub intents_accepted: u64,
    pub intents_rejected: u64,
    pub intents_expired: u64,
    pub intents_duplicate: u64,
    pub orders_placed: u64,
    pub orders_filled: u64,
    pub orders_partial: u64,
    pub orders_rejected: u64,
    pub orders_unknown: u64,
    pub orders_reconciled: u64,
    pub last_intent_ts: Option<i64>,
    pub last_report_ts: Option<i64>,
    pub last_error: Option<String>,
    pub ws_clients_connected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSnapshot {
    pub timestamp: i64,
    pub node3: Node3LinkStatus,
    /// Node 1 market-data consumer health. Additive: always present, with
    /// `enabled: false` when the service runs in `node3` mode.
    #[serde(default)]
    pub market_feed: FeedSnapshot,
    /// Local strategy state. Additive: disabled in `node3` mode.
    #[serde(default)]
    pub strategy: StrategySnapshot,
    pub work: ExecutionWorkDiagnostics,
    pub open_trades: OpenTradesSnapshot,
    pub deriv_account: DerivAccountSnapshot,
    pub mt5: Mt5Diagnostics,
    pub recent_events: Vec<ActivityLogEntry>,
}

/// Health of the versioned Node 1 market-data feed.
///
/// `trading_ready` is the only field the execution gate reads; it is computed
/// when the snapshot is taken, so a feed that merely *was* live is never
/// mistaken for a feed that still is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSnapshot {
    pub enabled: bool,
    /// `disabled | connecting | subscribing | syncing | live | stale |
    /// market_closed | resyncing | contract_mismatch | contract_violation |
    /// offline`.
    pub state: String,
    pub trading_ready: bool,
    pub not_ready_reason: Option<String>,
    pub url: String,
    /// `none (contract v1)` or `bearer`.
    pub auth: String,
    pub contract_version: Option<String>,
    pub contract_required: String,
    pub symbol: String,
    pub interval: String,
    pub connected: bool,
    pub last_frame_ms: Option<i64>,
    pub last_status_ms: Option<i64>,
    pub last_candle_time: Option<i64>,
    pub last_closed_candle_time: Option<i64>,
    pub last_levels_ts: Option<i64>,
    /// The last live envelope `seq`; replay frames do not advance it.
    pub last_seq: Option<u64>,
    /// Closed candles observed since the current socket connected (the v1
    /// subscribe replay seeds 15).
    pub session_closed_candles: u64,
    /// A live (non-replay) frame has arrived after the subscribe replay.
    pub sync_complete: bool,
    /// `contract_version` matches the pin.
    pub contract_ok: bool,
    pub freshness_state: Option<String>,
    pub freshness_age_ms: Option<u64>,
    pub market_open: Option<bool>,
    pub reconnect_count: u64,
    pub frames_received: u64,
    pub candles_received: u64,
    pub closed_candles: u64,
    pub levels_received: u64,
    pub status_frames: u64,
    pub duplicates: u64,
    pub out_of_order: u64,
    pub gaps: u64,
    pub contract_violations: u64,
    pub resyncs: u64,
    pub last_error: Option<String>,
    pub last_event_ms: Option<i64>,
}

impl Default for FeedSnapshot {
    fn default() -> Self {
        Self {
            enabled: false,
            state: "disabled".into(),
            trading_ready: false,
            not_ready_reason: Some("NODE1_FEED_ENABLED is not set".into()),
            url: String::new(),
            auth: "none".into(),
            contract_version: None,
            contract_required: "1.0".into(),
            symbol: "XAUUSD".into(),
            interval: "15m".into(),
            connected: false,
            last_frame_ms: None,
            last_status_ms: None,
            last_candle_time: None,
            last_closed_candle_time: None,
            last_levels_ts: None,
            last_seq: None,
            session_closed_candles: 0,
            sync_complete: false,
            contract_ok: false,
            freshness_state: None,
            freshness_age_ms: None,
            market_open: None,
            reconnect_count: 0,
            frames_received: 0,
            candles_received: 0,
            closed_candles: 0,
            levels_received: 0,
            status_frames: 0,
            duplicates: 0,
            out_of_order: 0,
            gaps: 0,
            contract_violations: 0,
            resyncs: 0,
            last_error: None,
            last_event_ms: None,
        }
    }
}

/// State of the local strategy runner. Disabled (all zeros) when Node 3 is the
/// intent source, so `/diagnostics` has one stable shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySnapshot {
    pub enabled: bool,
    pub mode: String,
    pub strategy_version: String,
    pub symbol: String,
    pub interval: String,
    pub volume_threshold: f64,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub ttl_secs: u64,
    pub levels_tracked: usize,
    pub candles_observed: u64,
    pub last_candle_time: Option<i64>,
    pub atr: f64,
    pub last_decision_at_ms: Option<i64>,
    pub last_action: String,
    pub last_trigger_level: Option<String>,
    pub last_intent_id: Option<String>,
    pub intents_emitted: u64,
    pub intents_suppressed: u64,
    pub last_suppression_reason: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: Option<i64>,
    pub breaks_detected: u64,
    pub breaks_invalidated: u64,
    pub retests_rejected_low_volume: u64,
    pub signals_confirmed: u64,
}

impl Default for StrategySnapshot {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "node3".into(),
            strategy_version: String::new(),
            symbol: String::new(),
            interval: String::new(),
            volume_threshold: 0.0,
            sl_min_pips: 0.0,
            sl_max_pips: 0.0,
            tp_min_pips: 0.0,
            tp_max_pips: 0.0,
            rr_min: 0.0,
            rr_max: 0.0,
            ttl_secs: 0,
            levels_tracked: 0,
            candles_observed: 0,
            last_candle_time: None,
            atr: 0.0,
            last_decision_at_ms: None,
            last_action: "flat".into(),
            last_trigger_level: None,
            last_intent_id: None,
            intents_emitted: 0,
            intents_suppressed: 0,
            last_suppression_reason: None,
            last_error: None,
            updated_at: None,
            breaks_detected: 0,
            breaks_invalidated: 0,
            retests_rejected_low_volume: 0,
            signals_confirmed: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5Diagnostics {
    pub account: Mt5AccountSnapshot,
    pub positions: Mt5PositionsSnapshot,
    pub history: Mt5HistorySnapshot,
    pub status: Mt5BridgeStatus,
}

// ---------------------------------------------------------------------------
// Deriv options account (venue `deriv_demo`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub app_id_configured: bool,
    pub token_kind: String,
    pub setup_hint: Option<String>,
}

impl Default for DerivAccountSnapshot {
    fn default() -> Self {
        Self {
            configured: false,
            connected: false,
            authorized: false,
            account_id: None,
            account_type: "unconfigured".into(),
            balance: None,
            currency: "USD".into(),
            open_trades_count: 0,
            total_open_stake: 0.0,
            total_unrealized_pnl: 0.0,
            open_trades: Vec::new(),
            last_updated: None,
            error: None,
            app_id_configured: false,
            token_kind: "none".into(),
            setup_hint: None,
        }
    }
}

// ---------------------------------------------------------------------------
// MT5 demo bridge schema (mirrors mt5-bridge/src/snapshot.rs)
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
            account_type: "unknown".into(),
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
            requested_symbol: String::new(),
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
            account_type: "unknown".into(),
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
    /// The Node 4 endpoint the terminal-side bridge dials out to.
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

// ---------------------------------------------------------------------------
// Venue-level trade events (adapter output, before the intent is attached)
// ---------------------------------------------------------------------------

/// A venue's own record of an order it accepted.
///
/// Venue adapters (Deriv options, Chelsea MCP) produce this; the execution
/// manager attaches the `intent_id` when it turns the event into an
/// [`ExecutionTrade`]. Broker-authoritative MT5 fills come from
/// [`Mt5OrderOutcome`] instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
}

// ---------------------------------------------------------------------------
// MT5 bridge session view (internal; rendered through `Mt5Diagnostics`)
// ---------------------------------------------------------------------------

/// The latest broker view the bridge pushed, cached between frames so the
/// `/mt5/*` resources answer instantly and the venue health checks have one
/// authoritative source.
#[derive(Debug, Clone, Default)]
pub struct Mt5SnapshotState {
    pub account: Mt5AccountSnapshot,
    pub positions: Mt5PositionsSnapshot,
    pub history: Mt5HistorySnapshot,
    pub status: Mt5BridgeStatus,
    /// Timestamp of the last frame of any kind: proof the bridge is alive.
    pub last_bridge_frame: Option<i64>,
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)] // `Diagnostics` carries the full snapshot
pub enum WsFrame {
    // ---- operator/public frames -------------------------------------------
    #[serde(rename = "diagnostics")]
    Diagnostics { data: ExecutionSnapshot },

    #[serde(rename = "open_trades")]
    OpenTrades { data: OpenTradesSnapshot },

    #[serde(rename = "deriv_account")]
    DerivAccount { data: DerivAccountSnapshot },

    #[serde(rename = "trade")]
    Trades { data: ExecutionTrade },

    #[serde(rename = "diagnostic_event")]
    DiagnosticEvent { data: ActivityLogEntry },

    // ---- MT5 demo bridge resources (mirrored from the bridge contract) ----
    #[serde(rename = "mt5_account")]
    Mt5Account { data: Mt5AccountSnapshot },

    #[serde(rename = "mt5_positions")]
    Mt5Positions { data: Mt5PositionsSnapshot },

    #[serde(rename = "mt5_history")]
    Mt5History { data: Mt5HistorySnapshot },

    #[serde(rename = "bridge_status")]
    BridgeStatus { data: Mt5BridgeStatus },

    #[serde(rename = "bridge_event")]
    BridgeEvent {
        event: String,
        #[serde(default)]
        data: serde_json::Value,
    },

    // ---- handshake --------------------------------------------------------
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
    },

    #[serde(rename = "bridge_hello_ack")]
    BridgeHelloAck {
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol: Option<u32>,
    },

    #[serde(rename = "bridge_ack")]
    BridgeAck {
        req_id: String,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<BridgeErrorPayload>,
    },

    // ---- commands Node 4 sends to the bridge ------------------------------
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_trade_serialisation_keeps_optional_fields_absent() {
        let trade = ExecutionTrade {
            trade_id: "t-1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            size: 0.01,
            entry: 2650.0,
            sl: 2647.5,
            tp: 2656.0,
            status: "open".into(),
            timestamp: 1_700_000_000_000,
            intent_id: Some("n3-xauusd-1-buy-pw-poc".into()),
            level_name: Some("PW PoC".into()),
            venue: Some("deriv_mt5_demo".into()),
            rr: Some(2.4),
            current_price: Some(2651.0),
            unrealized_pnl: Some(1.0),
            closed_at: None,
        };
        let frame = WsFrame::Trades { data: trade };
        let val = serde_json::to_value(&frame).unwrap();
        assert_eq!(val["type"], "trade");
        assert_eq!(val["data"]["trade_id"], "t-1");
        assert_eq!(val["data"]["intent_id"], "n3-xauusd-1-buy-pw-poc");
        assert!(val["data"].get("closed_at").is_none());
    }

    #[test]
    fn open_trades_uses_the_node4_field_name() {
        let snapshot = OpenTradesSnapshot::default();
        let val = serde_json::to_value(&snapshot).unwrap();
        assert!(val.get("node4_open_trades").is_some());
        assert!(val.get("node3_open_trades").is_none());
    }

    #[test]
    fn bridge_frames_round_trip_through_the_tagged_enum() {
        let raw = r#"{"type":"mt5_halt","req_id":"r-1","reason":"operator","flatten":true}"#;
        let frame: WsFrame = serde_json::from_str(raw).unwrap();
        match frame {
            WsFrame::Mt5Halt {
                req_id,
                reason,
                flatten,
            } => {
                assert_eq!(req_id, "r-1");
                assert_eq!(reason.as_deref(), Some("operator"));
                assert_eq!(flatten, Some(true));
            }
            other => panic!("unexpected frame: {other:?}"),
        }

        let ack = WsFrame::BridgeAck {
            req_id: "r-2".into(),
            ok: true,
            data: Some(serde_json::json!({ "halted": true })),
            error: None,
        };
        let value = serde_json::to_value(&ack).unwrap();
        assert_eq!(value["type"], "bridge_ack");
        assert_eq!(value["ok"], true);
        assert!(value.get("error").is_none());
    }

    #[test]
    fn mt5_order_outcome_requires_a_confirmed_fill() {
        let mut outcome = Mt5OrderOutcome::default();
        assert!(!outcome.is_confirmed_fill());
        outcome.status = "filled".into();
        assert!(!outcome.is_confirmed_fill(), "a fill needs volume");
        outcome.filled_volume = 0.01;
        assert!(outcome.is_confirmed_fill());
        outcome.status = "timeout".into();
        assert!(!outcome.is_confirmed_fill());
        outcome.status = "partial".into();
        assert!(outcome.is_confirmed_fill());
    }
}
