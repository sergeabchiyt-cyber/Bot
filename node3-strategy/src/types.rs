use serde::{Deserialize, Serialize};

pub const EXECUTION_PROTOCOL_VERSION: u32 = 1;
pub const INTENT_SCHEMA_VERSION: u32 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectedOrder {
    pub side: String,
    pub entry: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub sl_pips: f64,
    pub tp_pips: f64,
    pub risk_reward: f64,
    /// Always `node4_managed`: account-aware quantity is not a Node 3 concern.
    pub sizing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedLevelSetup {
    pub id: String,
    pub name: String,
    pub window: String,
    pub level_price: f64,
    /// `scanning_break` | `broken_above` | `broken_below`.
    pub state: String,
    pub state_label: String,
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
    pub active_projection: Option<ProjectedOrder>,
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

/// Immutable strategy output sent from Node 3 to Node 4.
///
/// `intent_id` is also the cross-service idempotency key. Node 4 must persist
/// it before touching a broker and must never execute it twice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeIntent {
    pub schema_version: u32,
    pub intent_id: String,
    pub strategy: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub reference_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub risk_reward: f64,
    pub level_name: String,
    pub source_candle_time: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

impl TradeIntent {
    pub fn is_expired_at(&self, timestamp: i64) -> bool {
        self.expires_at <= timestamp
    }
}

/// Broker-authoritative response produced by Node 4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionReport {
    pub schema_version: u32,
    pub intent_id: String,
    pub status: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshot {
    pub pending: Vec<TradeIntent>,
    pub recent: Vec<TradeIntent>,
    pub execution_reports: Vec<ExecutionReport>,
    pub pending_count: usize,
    pub node4_connected: bool,
    pub node4_token_configured: bool,
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
pub struct StrategyWorkDiagnostics {
    pub service: String,
    pub version: String,
    pub role: String,
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
    pub breaks_detected: u64,
    pub breaks_invalidated: u64,
    pub retests_rejected_low_volume: u64,
    pub signals_confirmed: u64,
    pub intents_emitted: u64,
    pub intents_dropped: u64,
    pub execution_reports_received: u64,
    pub last_signal_ts: Option<i64>,
    pub last_execution_report_ts: Option<i64>,
    pub last_error: Option<String>,
    pub public_ws_clients_connected: usize,
    pub node4_connected: bool,
    pub node4_token_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub timestamp: i64,
    pub scanning: ScanningSnapshot,
    pub signals: SignalSnapshot,
    pub work: StrategyWorkDiagnostics,
    pub recent_events: Vec<ActivityLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    // Node 1 -> Node 3 market contract.
    #[serde(rename = "levels")]
    Levels { data: VpLevels },
    #[serde(rename = "candle")]
    Candle { data: VpCandle },

    // Public Node 3 diagnostics contract.
    #[serde(rename = "diagnostics")]
    Diagnostics { data: Box<DiagnosticsSnapshot> },
    #[serde(rename = "scanning")]
    Scanning { data: ScanningSnapshot },
    #[serde(rename = "signals")]
    Signals { data: SignalSnapshot },
    #[serde(rename = "diagnostic_event")]
    DiagnosticEvent { data: ActivityLogEntry },

    // Private Node 3 <-> Node 4 execution contract.
    #[serde(rename = "execution_hello")]
    ExecutionHello {
        token: String,
        #[serde(default)]
        service: String,
        #[serde(default)]
        protocol_version: u32,
    },
    #[serde(rename = "execution_hello_ack")]
    ExecutionHelloAck {
        accepted: bool,
        protocol_version: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    #[serde(rename = "trade_intent")]
    TradeIntent { data: TradeIntent },
    #[serde(rename = "execution_report")]
    ExecutionReport { data: ExecutionReport },
    #[serde(rename = "execution_report_ack")]
    ExecutionReportAck {
        intent_id: String,
        accepted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    #[serde(rename = "subscribe")]
    Subscribe {
        #[serde(default)]
        topics: Vec<String>,
    },
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "heartbeat")]
    Heartbeat,
}
