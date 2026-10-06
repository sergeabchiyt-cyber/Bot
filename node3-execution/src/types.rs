use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpLevels {
    pub window: String,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    #[serde(default)]
    pub start: i64,
    #[serde(default)]
    pub end: i64,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub direction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swing_high: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swing_low: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunday_open: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpCandle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    #[serde(default)]
    pub source: String,
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedOrder {
    pub side: String,
    pub entry: f64,
    pub sl: f64,
    pub tp: f64,
    pub sl_pips: f64,
    pub tp_pips: f64,
    pub rr: f64,
    pub size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedLevelSetup {
    pub id: String,
    pub name: String,
    pub window: String,
    pub level_price: f64,
    /// `"scanning_break"` | `"broken_above"` | `"broken_below"`
    pub state: String,
    pub state_label: String,
    /// `Some("buy")` when `broken_above`, `Some("sell")` when `broken_below`, `None` while waiting for break.
    pub pending_side: Option<String>,
    pub current_price: Option<f64>,
    pub distance_dollars: Option<f64>,
    pub distance_pips: Option<f64>,
    pub retest_zone_low: f64,
    pub retest_zone_high: f64,
    pub invalidation_price: Option<f64>,
    pub volume_required: f64,
    pub current_volume: Option<f64>,
    pub volume_confirmed: bool,
    pub projected_buy: ProjectedOrder,
    pub projected_sell: ProjectedOrder,
    pub active_order: Option<ProjectedOrder>,
    pub broken_at: Option<i64>,
    pub broken_price: Option<f64>,
    pub last_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanningSnapshot {
    pub active_setups_count: usize,
    pub total_levels_tracked: usize,
    pub proximity_dollars: f64,
    pub invalidation_dollars: f64,
    pub volume_threshold: f64,
    pub current_volume: Option<f64>,
    pub volume_confirmed: bool,
    pub last_price: Option<f64>,
    pub atr: f64,
    pub atr_pips: f64,
    pub setups: Vec<ScannedLevelSetup>,
    pub timestamp: i64,
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
    /// Actionable configuration hint naming the env var to fix, e.g.
    /// "DERIV_DEMO_API looks like a PAT (pat_...) but DERIV_APP_ID is not set".
    /// `None` when the Deriv venue looks correctly configured.
    #[serde(default)]
    pub setup_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenTradesSnapshot {
    pub node3_open_trades: Vec<TradeEvent>,
    pub deriv_open_trades: Vec<DerivOpenContract>,
    /// Broker positions reported by the MT5 demo bridge. Kept separate from
    /// both Node 3's own trade ledger and the Deriv options contracts, because
    /// a strategy-side candle simulation is **not** a broker position.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineWorkDiagnostics {
    pub service: String,
    pub version: String,
    pub venue: String,
    pub started_at: i64,
    pub uptime_secs: u64,
    pub node1_ws_url: String,
    pub node1_connected: bool,
    pub node1_state: String,
    pub node1_reconnect_count: u64,
    pub last_node1_msg_ts: Option<i64>,
    pub last_candle_ts: Option<i64>,
    pub last_levels_ts: Option<i64>,
    pub candles_received: u64,
    pub levels_received: u64,
    pub ws_messages_received: u64,
    pub candle_buffer_len: usize,
    pub candle_buffer_capacity: usize,
    pub last_price: Option<f64>,
    pub last_candle: Option<VpCandle>,
    pub atr: f64,
    pub atr_pips: f64,
    pub current_sl_pips: f64,
    pub current_tp_pips: f64,
    pub current_rr: f64,
    pub volume_threshold: f64,
    pub last_candle_volume: Option<f64>,
    pub volume_ratio: Option<f64>,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub order_size: f64,
    pub breaks_detected: u64,
    pub breaks_invalidated: u64,
    pub retests_rejected_low_volume: u64,
    pub signals_confirmed: u64,
    pub trades_executed: u64,
    pub trades_failed: u64,
    pub last_signal_ts: Option<i64>,
    pub last_error: Option<String>,
    pub ws_clients_connected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub timestamp: i64,
    pub scanning: ScanningSnapshot,
    pub open_trades: OpenTradesSnapshot,
    pub deriv_account: DerivAccountSnapshot,
    /// MT5 demo bridge view. Kept as its own resource *and* embedded here so a
    /// single `/diagnostics` call shows where orders are actually going.
    #[serde(default)]
    pub mt5: Mt5Diagnostics,
    pub work: EngineWorkDiagnostics,
    pub recent_events: Vec<ActivityLogEntry>,
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

impl Default for Mt5BridgeStatus {
    fn default() -> Self {
        Self {
            configured: false,
            connected: false,
            authorized: false,
            protocol: 1,
            bridge_version: String::new(),
            node3_url: String::new(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    #[serde(rename = "levels")]
    Levels { data: VpLevels },

    #[serde(rename = "candle")]
    Candle { data: VpCandle },

    #[serde(rename = "trades")]
    Trades { data: TradeEvent },

    #[serde(rename = "diagnostics")]
    Diagnostics { data: DiagnosticsSnapshot },

    #[serde(rename = "scanning")]
    Scanning { data: ScanningSnapshot },

    #[serde(rename = "open_trades")]
    OpenTrades { data: OpenTradesSnapshot },

    #[serde(rename = "deriv_account")]
    DerivAccount { data: DerivAccountSnapshot },

    #[serde(rename = "diagnostic_event")]
    DiagnosticEvent { data: ActivityLogEntry },

    // ---- MT5 demo bridge (see docs/mt5/EXECUTION_ARCHITECTURE.md) ----------
    /// First frame the bridge sends on `/ws`; Node 3 validates `token` against
    /// `MT5_BRIDGE_TOKEN` before the connection becomes the bridge session.
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

    /// Node 3's answer to `bridge_hello`.
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

    // ---- commands Node 3 sends to the bridge -----------------------------
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
// copies drift. Node 3 only ever *displays* them and reconciles them against its
// own trade ledger — it never invents broker state.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mt5BridgeStatus {
    pub configured: bool,
    pub connected: bool,
    pub authorized: bool,
    pub protocol: u32,
    pub bridge_version: String,
    pub node3_url: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_event_backwards_compatible_serialization() {
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
        };
        let frame = WsFrame::Trades { data: trade };
        let val = serde_json::to_value(&frame).unwrap();
        assert_eq!(val["type"], "trades");
        assert_eq!(val["data"]["trade_id"], "t-1");
        assert_eq!(val["data"]["level_name"], "PW PoC");
        assert!(val["data"].get("closed_at").is_none());
    }
}
