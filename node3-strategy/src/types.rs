use serde::{Deserialize, Serialize};

use crate::vp::VpAudit;

/// Version of the private Node 3 <-> Node 4 execution protocol.
pub const EXECUTION_PROTOCOL_VERSION: u32 = 1;
/// Version of the `trade_intent` schema. v2 changes are additive optional fields only.
pub const INTENT_SCHEMA_VERSION: u32 = 1;
/// Strategy identifier carried on every intent. Kept unchanged from v1 so the
/// additive v2 schema stays compatible. The rule version is `STRATEGY_VERSION`.
pub const STRATEGY_NAME: &str = "vp_break_retest_v1";
/// Bump on every rule change so Node 4 can tell intents apart by logic.
pub const STRATEGY_VERSION: &str = "2.0.0";

/// Metadata describing how a profile was built. Published with every level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub row_mode: String,
    pub row_height: f64,
    pub rows: usize,
    pub range_low: f64,
    pub range_high: f64,
    pub va_pct: f64,
    pub total_volume: f64,
    pub input_interval: String,
    pub input_bars: usize,
}

/// A published volume-profile level (C2 `/levels`).
///
/// `window` is `PW`, `PS`, or `CW`. PS publishes PoC only. `vah` and `val` are
/// absent for PS. `sunday_open` is present on PW only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpLevels {
    pub window: String,
    pub poc: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vah: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub val: Option<f64>,
    pub start: i64,
    pub end: i64,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunday_open: Option<f64>,
    pub meta: ProfileMeta,
}

/// A stored closed bar. `volume` is Node 1's tick volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// A window that is not yet publishable, with the reason and the time it will be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingWindow {
    pub window: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_at_ms: Option<i64>,
}

/// One drawing for the chart: a level ray, a sunday-open marker, or an FRVP rectangle.
///
/// The identity of a level is `(window, kind, start, end)`. `id` is that tuple
/// rendered as `WINDOW:kind:start:end`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingItem {
    pub id: String,
    /// `level` | `sunday_open` | `frvp`.
    pub kind: String,
    pub window: String,
    pub label: String,
    /// `1H` (intraday) or `daily`. FRVP rectangles are `1H`.
    pub timeframe: String,
    pub color: String,
    /// `solid` | `dashed` | `dotted` | `rectangle`.
    pub line_style: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    pub start: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ray_from_time: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<crate::vp::ProfileRow>>,
}

/// Full drawing set (C2 `/drawings`). `pending` lists windows not yet publishable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingsPayload {
    pub timestamp: i64,
    pub items: Vec<DrawingItem>,
    pub pending: Vec<PendingWindow>,
}

/// Per-resolution summary of the bar store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesStat {
    pub interval: String,
    pub bars: usize,
    pub oldest_ms: Option<i64>,
    pub newest_ms: Option<i64>,
}

/// Publish status of one window in diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowStatus {
    pub window: String,
    pub published: bool,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub input_interval: Option<String>,
    pub note: Option<String>,
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
    /// Identity of the level. Equals the drawing id `(window, kind, start, end)`.
    pub drawing_id: String,
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
    /// Present only when the volume gate is enabled.
    pub volume_required: Option<f64>,
    pub volume_confirmed: Option<bool>,
    pub projected_buy: Option<ProjectedOrder>,
    pub projected_sell: Option<ProjectedOrder>,
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
    pub volume_filter_enabled: bool,
    pub volume_threshold: f64,
    pub current_volume: Option<f64>,
    pub volume_confirmed: Option<bool>,
    pub last_price: Option<f64>,
    pub strategy_interval: String,
    pub tradeable: bool,
    pub freshness_state: String,
    pub atr: Option<f64>,
    pub atr_pips: Option<f64>,
    pub sl_pips: Option<f64>,
    pub tp_pips: Option<f64>,
    pub rr: Option<f64>,
    pub setups: Vec<ScannedLevelSetup>,
    pub timestamp: i64,
}

/// Immutable strategy output sent from Node 3 to Node 4.
///
/// `intent_id` is also the cross-service idempotency key. Node 4 must persist
/// it before touching a broker and must never execute it twice. Fields added
/// after v1 are optional, so existing consumers keep working.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atr: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atr_pips: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_confirmed: Option<bool>,
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
    pub contract_version: String,
    pub last_node1_msg_ts: Option<i64>,
    pub last_candle_ts: Option<i64>,
    pub last_tick_volume_ts: Option<i64>,
    pub last_calendar_ts: Option<i64>,
    pub ws_messages_received: u64,
    pub closed_bars_stored: u64,
    pub duplicate_bars_ignored: u64,
    pub open_bars_ignored: u64,
    pub replayed_bars: u64,
    pub tick_volume_frames: u64,
    pub last_tick_volume: Option<u64>,
    pub calendar_items: usize,
    pub contract_violations: u64,
    pub last_violation: Option<String>,
    pub last_violation_ts: Option<i64>,
    pub feed_state: String,
    pub freshness_state: String,
    pub freshness_age_ms: Option<i64>,
    pub market_open: Option<bool>,
    pub tradeable: bool,
    pub store_series: Vec<SeriesStat>,
    pub retention_days: i64,
    pub week_anchor: String,
    pub vp_ready: bool,
    pub windows: Vec<WindowStatus>,
    pub va_pct: f64,
    pub vp_rows: usize,
    pub vp_row_mode: String,
    pub vp_bin_size: f64,
    pub vp_lower_tf: String,
    pub ps_mode: String,
    pub cw_first_publish: String,
    pub strategy_interval: String,
    pub last_price: Option<f64>,
    pub last_candle: Option<VpCandle>,
    pub atr: Option<f64>,
    pub atr_pips: Option<f64>,
    pub current_sl_pips: Option<f64>,
    pub current_tp_pips: Option<f64>,
    pub current_rr: Option<f64>,
    pub volume_filter_enabled: bool,
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

/// Compact health view for `GET /health`. The probe always returns HTTP 200
/// while the process is alive. Upstream outages show in `status`, not as a
/// process failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    /// `ok` | `degraded` | `disconnected`.
    pub status: String,
    pub ok: bool,
    pub service: String,
    pub version: String,
    pub timestamp: i64,
    pub uptime_secs: u64,
    pub contract_version: String,
    pub node1_connected: bool,
    pub node1_state: String,
    pub freshness_state: String,
    pub tradeable: bool,
    pub vp_ready: bool,
    pub contract_violations: u64,
    pub intents_pending: usize,
    pub node4_connected: bool,
    pub node4_token_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    // Public Node 3 market contract (C2).
    #[serde(rename = "levels")]
    Levels { data: Vec<VpLevels> },
    #[serde(rename = "drawing")]
    Drawing { data: DrawingsPayload },
    #[serde(rename = "vp")]
    Vp { data: Box<VpAudit> },

    // Public Node 3 diagnostics contract.
    #[serde(rename = "diagnostics")]
    Diagnostics { data: Box<DiagnosticsSnapshot> },
    #[serde(rename = "scanning")]
    Scanning { data: ScanningSnapshot },
    #[serde(rename = "signals")]
    Signals { data: SignalSnapshot },
    #[serde(rename = "diagnostic_event")]
    DiagnosticEvent { data: ActivityLogEntry },

    // Private Node 3 <-> Node 4 execution contract. Never sent on the public socket.
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
