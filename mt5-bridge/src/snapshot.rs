//! Snapshot + frame schema shared with Node 3.
//!
//! These structs are the wire contract between the bridge and Node 3, and Node 3
//! mirrors them field-for-field in `node3-execution/src/types.rs`
//! (`Mt5AccountSnapshot`, `Mt5PositionsSnapshot`, `Mt5HistorySnapshot`,
//! `Mt5BridgeStatus`). `ci/mt5/protocol_lint.py` fails the build if the two
//! copies drift apart, which is why the names must stay identical.

use serde::{Deserialize, Serialize};

pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;
pub const BRIDGE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Account view for the frontend. `configured`/`connected`/`authorized`/
/// `account_type`/`error` are the fields required by the design document's
/// `/mt5/account` contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
            bridge_version: BRIDGE_VERSION.to_string(),
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
    pub current_price: Option<f64>,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
    /// `in` | `out` | `inout` (MT5 deal entry).
    pub entry: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mt5HistorySnapshot {
    pub deals: Vec<Mt5Deal>,
    pub count: usize,
    /// Opaque cursor for the next page (line offset of the append-only store).
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl Default for Mt5BridgeStatus {
    fn default() -> Self {
        Self {
            configured: false,
            connected: false,
            authorized: false,
            protocol: BRIDGE_PROTOCOL_VERSION,
            bridge_version: BRIDGE_VERSION.to_string(),
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

/// Outcome of one order intent, returned to Node 3 inside `bridge_ack.data`.
///
/// Node 3 marks a trade opened **only** when `status` is `filled` or `partial`
/// (the latter carrying a broker-confirmed `filled_volume`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderOutcome {
    /// `filled` | `partial` | `rejected` | `unknown` | `duplicate`
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
    /// True when the outcome was resolved by an idempotent broker lookup after
    /// a timeout instead of by the original `ORDER_SEND` response.
    pub reconciled: bool,
    pub latency_ms: i64,
    pub ts: i64,
    pub error: Option<String>,
}

impl Default for OrderOutcome {
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

impl OrderOutcome {
    /// A trade may only be marked opened on a broker-confirmed fill.
    pub fn is_confirmed_fill(&self) -> bool {
        matches!(self.status.as_str(), "filled" | "partial") && self.filled_volume > 0.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BridgeErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl BridgeErrorPayload {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Bridge → Node 3 frames.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeToNode3 {
    /// First frame on every (re)connection; Node 3 validates `token`.
    BridgeHello {
        token: String,
        protocol: u32,
        bridge: String,
        venue: String,
        capabilities: Vec<String>,
        account: Mt5AccountSnapshot,
    },
    BridgeStatus {
        data: Mt5BridgeStatus,
    },
    Mt5Account {
        data: Mt5AccountSnapshot,
    },
    Mt5Positions {
        data: Mt5PositionsSnapshot,
    },
    Mt5History {
        data: Mt5HistorySnapshot,
    },
    BridgeAck {
        req_id: String,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<BridgeErrorPayload>,
    },
    BridgeEvent {
        event: String,
        data: serde_json::Value,
    },
    Heartbeat,
}

/// Node 3 → bridge frames.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node3ToBridge {
    /// Node 3's answer to `bridge_hello`.
    BridgeHelloAck {
        ok: bool,
        #[serde(default)]
        error: Option<String>,
    },
    Mt5Order {
        req_id: String,
        idempotency_key: String,
        #[serde(default)]
        strategy_id: String,
        #[serde(default)]
        symbol: Option<String>,
        side: String,
        #[serde(default)]
        volume: Option<f64>,
        #[serde(default)]
        sl: Option<f64>,
        #[serde(default)]
        tp: Option<f64>,
        #[serde(default)]
        level_name: Option<String>,
        #[serde(default)]
        entry_ref: Option<f64>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    Mt5Modify {
        req_id: String,
        position_ticket: i64,
        #[serde(default)]
        sl: Option<f64>,
        #[serde(default)]
        tp: Option<f64>,
    },
    Mt5Close {
        req_id: String,
        position_ticket: i64,
        #[serde(default)]
        volume: Option<f64>,
    },
    Mt5CloseAll {
        req_id: String,
        #[serde(default)]
        reason: Option<String>,
    },
    Mt5Halt {
        req_id: String,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        flatten: Option<bool>,
    },
    Mt5Resume {
        req_id: String,
    },
    Mt5SnapshotRequest {
        req_id: String,
        #[serde(default)]
        what: Option<Vec<String>>,
    },
    Mt5Ping {
        req_id: String,
    },
    #[serde(other)]
    Unknown,
}

/// Capabilities advertised in `bridge_hello`.
pub fn capabilities() -> Vec<String> {
    vec![
        "order_send".into(),
        "position_modify".into(),
        "position_close".into(),
        "close_all".into(),
        "halt".into(),
        "resume".into(),
        "account".into(),
        "positions".into(),
        "history".into(),
        "reconcile".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_frame_serialises_with_a_type_tag() {
        let frame = BridgeToNode3::BridgeHello {
            token: "secret".into(),
            protocol: BRIDGE_PROTOCOL_VERSION,
            bridge: format!("mt5-bridge/{BRIDGE_VERSION}"),
            venue: "deriv_mt5_demo".into(),
            capabilities: capabilities(),
            account: Mt5AccountSnapshot::default(),
        };
        let value = serde_json::to_value(&frame).unwrap();
        assert_eq!(value["type"], "bridge_hello");
        assert_eq!(value["venue"], "deriv_mt5_demo");
        assert_eq!(value["account"]["account_type"], "unconfigured");
        assert_eq!(value["protocol"], 1);
    }

    #[test]
    fn node3_commands_round_trip_through_the_tagged_enum() {
        let raw = r#"{"type":"mt5_order","req_id":"r1","idempotency_key":"k1","side":"buy","volume":0.01,"sl":2647.5,"tp":2656.0}"#;
        let parsed: Node3ToBridge = serde_json::from_str(raw).unwrap();
        match parsed {
            Node3ToBridge::Mt5Order {
                req_id,
                idempotency_key,
                side,
                volume,
                ..
            } => {
                assert_eq!(req_id, "r1");
                assert_eq!(idempotency_key, "k1");
                assert_eq!(side, "buy");
                assert_eq!(volume, Some(0.01));
            }
            other => panic!("unexpected frame: {other:?}"),
        }

        // Unknown/forward-compatible frames must not break the loop.
        let unknown: Node3ToBridge =
            serde_json::from_str(r#"{"type":"mt5_something_new","req_id":"r9"}"#).unwrap();
        assert_eq!(unknown, Node3ToBridge::Unknown);
    }

    #[test]
    fn confirmed_fill_requires_a_broker_reported_volume() {
        let mut outcome = OrderOutcome {
            status: "filled".into(),
            filled_volume: 0.01,
            ..Default::default()
        };
        assert!(outcome.is_confirmed_fill());

        // A `filled` status without a volume is not a confirmation.
        outcome.filled_volume = 0.0;
        assert!(!outcome.is_confirmed_fill());

        outcome.status = "unknown".into();
        outcome.filled_volume = 0.01;
        assert!(!outcome.is_confirmed_fill());
    }
}
