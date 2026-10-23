//! Node 4 execution configuration.
//!
//! Ownership boundary (see `docs/NODE3_NODE4_PROTOCOL.md` and
//! `docs/BRANCH_ARCHITECTURE.md`): **Node 4** owns every account-aware setting —
//! venue selection, stake, lot size, risk cap, deviation, symbol mapping,
//! timeouts — while Node 3 sends prices and side only. Node 4 never receives,
//! stores, or forwards a broker password: MT5 credentials live with the
//! terminal-side bridge (`mt5-bridge/**`).

use std::env;

/// Default for `DERIV_MIN_STAKE`: the smallest stake Deriv prices for
/// `frxXAUUSD` on a USD options account. Deriv answers a smaller stake with
/// `InvalidMinStake` (`Please enter a stake amount that's at least 0.50.`), so
/// `EXECUTION_STAKE` below it is clamped up in the Deriv flow.
pub const DEFAULT_DERIV_MIN_STAKE: f64 = 0.50;

/// Default lot size for the MT5 demo venue. **Not** `EXECUTION_STAKE`: that
/// value is a USD options stake for the Deriv options venue and has nothing to
/// do with a MetaTrader lot size (1 lot of XAUUSD = 100 oz on Deriv MT5).
pub const DEFAULT_MT5_VOLUME_LOTS: f64 = 0.01;

/// Default durable idempotency ledger. Relative paths resolve against the
/// service working directory; hosting platforms inject `PORT` and expect the
/// process to keep its state under a writable path (e.g. `data/`).
pub const DEFAULT_EXECUTION_LEDGER_FILE: &str = "data/execution_ledger.jsonl";

/// The execution protocol version Node 4 announces in `execution_hello`.
pub const EXECUTION_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionVenue {
    /// Deriv MT5 **demo** account, driven through the `mt5-bridge` service that
    /// runs next to the terminal. See `docs/mt5/EXECUTION_ARCHITECTURE.md`.
    DerivMt5Demo,
    /// Deriv options (Rise/Fall contracts) — the existing `DERIV_DEMO_API`
    /// integration. Never used for MT5 orders.
    DerivDemo,
    ChelseaLive,
    /// No venue: intents are validated, durably recorded and reported, but no
    /// order is ever sent (explicit dry-run / signal mode).
    None,
}

impl ExecutionVenue {
    pub fn label(&self) -> &'static str {
        match self {
            ExecutionVenue::DerivMt5Demo => "deriv_mt5_demo",
            ExecutionVenue::DerivDemo => "deriv_demo",
            ExecutionVenue::ChelseaLive => "chelsea_live",
            ExecutionVenue::None => "none",
        }
    }

    pub fn parse(value: &str) -> Option<ExecutionVenue> {
        match value.trim().to_ascii_lowercase().as_str() {
            "deriv_mt5_demo" | "deriv_mt5" | "mt5" | "mt5_demo" => {
                Some(ExecutionVenue::DerivMt5Demo)
            }
            "deriv_demo" | "deriv" | "deriv_options" => Some(ExecutionVenue::DerivDemo),
            "chelsea" | "chelsea_live" | "chelsea_mcp" => Some(ExecutionVenue::ChelseaLive),
            "none" | "signal_only" | "signal" | "dry_run" => Some(ExecutionVenue::None),
            _ => None,
        }
    }

    /// True when the venue can place real broker orders. `none` is the only
    /// venue that cannot, and it is the only one allowed to run without a
    /// durable ledger.
    pub fn is_real(&self) -> bool {
        !matches!(self, ExecutionVenue::None)
    }
}

/// Where a trade intent comes from.
///
/// `Node3` is the original, execution-only topology: Node 4 dials Node 3's
/// authenticated `/execution` socket and never looks at market data.
///
/// `Node1Strategy` is the opt-in local path: Node 4 consumes the versioned
/// Node 1 market feed, runs the ported `vp_break_retest_v1` strategy itself, and
/// hands the resulting immutable intent to the *same* execution pipeline.
/// Selecting it replaces the Node 3 link entirely; the two sources are never
/// run at the same time, because two strategy inputs could open two positions
/// for the same market event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentSource {
    Node3,
    Node1Strategy,
}

impl IntentSource {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "node3" | "node_3" | "remote" => Some(IntentSource::Node3),
            "node1_strategy" | "node1" | "local" | "local_strategy" => {
                Some(IntentSource::Node1Strategy)
            }
            _ => None,
        }
    }

    /// True when Node 4 itself owns the strategy decision.
    pub fn is_local(&self) -> bool {
        matches!(self, IntentSource::Node1Strategy)
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    // ---- Intent source ----------------------------------------------------
    /// `NODE4_INTENT_SOURCE`: `node3` (default) or `node1_strategy` (opt-in).
    pub intent_source: IntentSource,
    /// Set when `NODE4_INTENT_SOURCE` was not one of the accepted values; a
    /// malformed value must fail startup rather than silently select Node 3.
    pub intent_source_error: Option<String>,

    // ---- Node 3 intent link (outbound WebSocket client) --------------------
    /// `NODE3_WS_URL`: Node 3's authenticated execution endpoint, for example
    /// `wss://<node3-host>/execution`. Node 4 dials out; Node 3 never connects
    /// to a broker host.
    pub node3_ws_url: String,
    /// `NODE4_SHARED_TOKEN`: the shared secret Node 4 presents as the first
    /// frame (`execution_hello`). Never logged, never sent to Node 2.
    pub node4_shared_token: Option<String>,
    /// Protocol version Node 4 announces; only version 1 is supported.
    pub protocol_version: u32,
    /// Local HTTP + WebSocket port (`GET /health`, `GET /diagnostics`,
    /// `GET /open-trades`, `GET /account`, `GET /mt5/*`, `WS /ws`,
    /// `WS /mt5/bridge`, `POST /mt5/control`). 10000 by default.
    pub port: u16,
    /// Seconds to wait for `execution_hello_ack` before dropping the socket.
    pub node3_hello_timeout_ms: u64,
    /// Reconnect backoff bounds for the Node 3 link.
    pub node3_reconnect_min_ms: u64,
    pub node3_reconnect_max_ms: u64,
    /// A Node 4 session with no frame from Node 3 for this long is stale: no
    /// new broker write is allowed, but monitoring/reconciliation continue.
    pub node3_stale_ms: i64,

    // ---- Durable idempotency ledger ---------------------------------------
    /// `EXECUTION_LEDGER_FILE`: append-only JSONL ledger holding every intent
    /// id Node 4 has accepted responsibility for (plus broker commands,
    /// outcomes and reconciliation results).
    pub execution_ledger_file: String,

    // ---- Venue credentials (mutually exclusive) ---------------------------
    pub mcp_chelsea_url: Option<String>,
    pub deriv_demo_api: Option<String>,
    pub deriv_app_id: Option<String>,
    pub deriv_api_url: String,
    /// Minimum stake accepted by the Deriv venue (USD).
    pub deriv_min_stake: f64,
    /// `EXECUTION_STAKE`: USD options stake for the Deriv options venue. The
    /// MT5 venue sizes in lots from `MT5_VOLUME_LOTS` instead.
    pub execution_stake: f64,

    // ---- MT5 demo bridge (venue `deriv_mt5_demo`) -------------------------
    /// `MT5_BRIDGE_TOKEN`: the shared secret the bridge must present in
    /// `bridge_hello` on `WS /mt5/bridge`. It is a **service** credential, not
    /// the MT5 account password, and the same value must exist in the bridge
    /// environment.
    pub mt5_bridge_token: Option<String>,
    /// `MT5_CONTROL_TOKEN`: bearer token for `POST /mt5/control`
    /// (halt/resume/flatten/close). Unset ⇒ the endpoint answers 403.
    pub mt5_control_token: Option<String>,
    /// `MT5_SYMBOL`: the instrument Node 3 trades (`XAUUSD`).
    pub mt5_symbol: String,
    /// `MT5_SYMBOL_MAP`: explicit requested→broker mapping (`XAUUSD=XAUUSD.a`).
    /// Broker suffixes are never guessed; an unmapped symbol fails closed.
    pub mt5_symbol_map: Vec<(String, String)>,
    /// `MT5_VOLUME_LOTS`: lot size used for MT5 market orders. Validated
    /// against the broker's min/max/step by the bridge before any send.
    pub mt5_volume_lots: f64,
    /// `MT5_ORDER_TIMEOUT_MS`: how long Node 4 waits for a broker-confirmed
    /// outcome before treating the order as unknown (never retried blindly).
    pub mt5_order_timeout_ms: u64,
    /// `MT5_HISTORY_PAGE_SIZE`: deals per `/mt5/history` page.
    pub mt5_history_page_size: usize,
    /// `MT5_MAX_RISK_PER_TRADE`: optional hard cap in account currency; an
    /// order whose stop distance implies more risk is refused before sending.
    pub mt5_max_risk_per_trade: Option<f64>,
    /// `MT5_ALLOW_OTHER_SYMBOL=1` allows `MT5_SYMBOL` to be something other
    /// than XAUUSD (the strategy is XAUUSD-only, so this is off by default).
    pub mt5_allow_other_symbol: bool,

    // ---- Resolved venue ---------------------------------------------------
    /// Explicit `EXECUTION_VENUE` selection, when set.
    pub execution_venue_override: Option<String>,
    /// The validated venue this process runs, as stored by [`Config::resolve_venue`].
    /// `ExecutionVenue::None` whenever the configuration is unusable, so the
    /// dedicated getter is [`Config::venue_selection_error`].
    pub venue: ExecutionVenue,
    /// Malformed `MT5_SYMBOL_MAP` entries collected at load time; surfaced as a
    /// hard error when the MT5 venue is selected.
    pub symbol_map_errors: Vec<String>,

    // ---- Node 1 market-data feed (contract v1, local strategy mode) --------
    /// `NODE1_FEED_ENABLED`: run the Node 1 consumer. It defaults on in
    /// `node1_strategy` mode and off in `node3` mode. In `node3` mode it is a
    /// read-only monitor: the strategy never emits an intent.
    pub node1_feed_enabled: bool,
    /// `NODE1_WS_URL`: `wss://<node1-host>/ws` — the market feed.
    pub node1_ws_url: String,
    /// `NODE1_FEED_TOKEN`: optional bearer token sent in the WebSocket
    /// handshake. Contract v1 defines **no** read authentication, so this is
    /// forward-compatible plumbing, not a security control by itself; set
    /// `NODE1_FEED_REQUIRE_AUTH=1` to make its absence fatal.
    pub node1_feed_token: Option<String>,
    /// `NODE1_FEED_REQUIRE_AUTH`: refuse to run without a feed token.
    pub node1_feed_require_auth: bool,
    /// `NODE1_REQUIRE_TLS`: refuse a plaintext `ws://` feed unless the host is
    /// loopback. Default on.
    pub node1_require_tls: bool,
    /// `NODE1_CONTRACT_VERSION`: the exact `contract_version` Node 1 must
    /// report. Any other value (including a missing one) keeps the feed
    /// non-trading.
    pub node1_contract_version: String,
    /// `NODE1_SYMBOL` and `NODE1_INTERVAL`: the feed identity Node 4 accepts.
    /// Both are part of every v1 candle/tick-volume frame.
    pub node1_symbol: String,
    pub node1_interval: String,
    /// Silence longer than this (default 75 s = 2.5 × the 30 s heartbeat)
    /// closes the socket and reconnects.
    pub node1_heartbeat_timeout_ms: i64,
    /// Node 1 `freshness.state` older than this is not tradable (default 60 s).
    pub node1_stale_ms: i64,
    /// A closed candle older than this (measured from its close) is never a
    /// trigger (default 30 min = 2 × the 15m interval).
    pub node1_max_candle_age_ms: i64,
    /// Reconnect backoff for the Node 1 feed.
    pub node1_reconnect_min_ms: u64,
    pub node1_reconnect_max_ms: u64,
    /// Closed candles that must be observed in a session before the feed is
    /// allowed to trade (the v1 subscribe replay carries 15).
    pub node1_min_closed_candles: u64,

    // ---- Local strategy ---------------------------------------------------
    /// `NODE4_STRATEGY_APPROVED`: the operator asserts that the Node 1 feed
    /// contract and this strategy specification are signed off for this
    /// deployment. Required in `node1_strategy` mode — a draft contract must
    /// never become tradable by accident.
    pub strategy_approved: bool,
    /// `NODE4_STRATEGY_VOLUME_THRESHOLD`: candle tick-volume gate (default
    /// 10,500, exactly Node 3's shipped value).
    pub strategy_volume_threshold: f64,
    /// Node 3's price projection: SL/TP in pips (0.01 price units for XAUUSD)
    /// and the hard risk/reward clamp.
    pub strategy_sl_min_pips: f64,
    pub strategy_sl_max_pips: f64,
    pub strategy_tp_min_pips: f64,
    pub strategy_tp_max_pips: f64,
    pub strategy_rr_min: f64,
    pub strategy_rr_max: f64,
    /// `NODE4_STRATEGY_TTL_SECS`: intent lifetime (default 120 s).
    pub strategy_ttl_secs: u64,
    /// `NODE4_STRATEGY_ATR_FALLBACK`: ATR used before enough candles exist.
    pub strategy_atr_fallback: f64,

    // ---- Local-path execution gates ---------------------------------------
    /// `NODE4_DEMO_INTEGRATION_PASSED`: required before a *real* venue may be
    /// driven by the local strategy. The MT5 demo gate in
    /// `docs/mt5/EXECUTION_ARCHITECTURE.md` must have passed first.
    pub demo_integration_passed: bool,
    /// `NODE4_LIVE_TRADING_APPROVED`: required for `chelsea_live` when the
    /// local strategy is the intent source. Live remains opt-in.
    pub live_trading_approved: bool,
}

fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_u16(key: &str, default: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_truthy(key: &str) -> bool {
    match env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

/// Parse `KEY=VALUE,KEY=VALUE` into pairs, reporting malformed entries.
fn env_pairs(key: &str) -> (Vec<(String, String)>, Vec<String>) {
    let mut pairs = Vec::new();
    let mut errors = Vec::new();
    let Some(raw) = env_opt(key) else {
        return (pairs, errors);
    };
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        match entry.split_once('=') {
            Some((from, to)) if !from.trim().is_empty() && !to.trim().is_empty() => {
                pairs.push((from.trim().to_string(), to.trim().to_string()));
            }
            _ => errors.push(format!(
                "{key} entry '{entry}' is not a `requested=broker` pair"
            )),
        }
    }
    (pairs, errors)
}

impl Default for Config {
    /// Test/dev defaults: **no venue credentials at all** (`ExecutionVenue::None`),
    /// XAUUSD requested, and the same numeric fallbacks `from_env()` uses when a
    /// variable is absent. Production code must go through `from_env()` so the
    /// environment stays the single source of truth.
    fn default() -> Self {
        Self {
            intent_source: IntentSource::Node3,
            intent_source_error: None,
            node3_ws_url: String::new(),
            node4_shared_token: None,
            protocol_version: EXECUTION_PROTOCOL_VERSION,
            port: 10_000,
            node3_hello_timeout_ms: 10_000,
            node3_reconnect_min_ms: 1_000,
            node3_reconnect_max_ms: 30_000,
            node3_stale_ms: 45_000,

            execution_ledger_file: DEFAULT_EXECUTION_LEDGER_FILE.into(),

            mcp_chelsea_url: None,
            deriv_demo_api: None,
            deriv_app_id: None,
            deriv_api_url: "https://api.derivws.com".into(),
            deriv_min_stake: DEFAULT_DERIV_MIN_STAKE,
            execution_stake: DEFAULT_DERIV_MIN_STAKE,

            mt5_bridge_token: None,
            mt5_control_token: None,
            mt5_symbol: "XAUUSD".into(),
            mt5_symbol_map: Vec::new(),
            mt5_volume_lots: DEFAULT_MT5_VOLUME_LOTS,
            mt5_order_timeout_ms: 15_000,
            mt5_history_page_size: 100,
            mt5_max_risk_per_trade: None,
            mt5_allow_other_symbol: false,

            execution_venue_override: None,
            venue: ExecutionVenue::None,
            symbol_map_errors: Vec::new(),

            node1_feed_enabled: false,
            node1_ws_url: String::new(),
            node1_feed_token: None,
            node1_feed_require_auth: false,
            node1_require_tls: true,
            node1_contract_version: "1.0".into(),
            node1_symbol: "XAUUSD".into(),
            node1_interval: "15m".into(),
            node1_heartbeat_timeout_ms: 75_000,
            node1_stale_ms: 60_000,
            node1_max_candle_age_ms: 1_800_000,
            node1_reconnect_min_ms: 1_000,
            node1_reconnect_max_ms: 30_000,
            node1_min_closed_candles: 15,

            strategy_approved: false,
            strategy_volume_threshold: 10_500.0,
            strategy_sl_min_pips: 200.0,
            strategy_sl_max_pips: 300.0,
            strategy_tp_min_pips: 600.0,
            strategy_tp_max_pips: 800.0,
            strategy_rr_min: 2.0,
            strategy_rr_max: 3.0,
            strategy_ttl_secs: 120,
            strategy_atr_fallback: 3.0,

            demo_integration_passed: false,
            live_trading_approved: false,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let (mt5_symbol_map, symbol_map_errors) = env_pairs("MT5_SYMBOL_MAP");

        let intent_source_raw = env_str("NODE4_INTENT_SOURCE", "node3");
        let (intent_source, intent_source_error) = match IntentSource::parse(&intent_source_raw) {
            Some(source) => (source, None),
            None => (
                IntentSource::Node3,
                Some(format!(
                    "NODE4_INTENT_SOURCE='{intent_source_raw}' is not recognised \
                         (expected node3 | node1_strategy)"
                )),
            ),
        };

        let mut config = Self {
            intent_source,
            intent_source_error,
            node3_ws_url: env_str("NODE3_WS_URL", ""),
            node4_shared_token: env_opt("NODE4_SHARED_TOKEN"),
            protocol_version: EXECUTION_PROTOCOL_VERSION,
            // Hosting platforms inject PORT; we default to 10000 so the health
            // and WebSocket endpoints are reachable out of the box.
            port: env_u16("PORT", 10_000),
            node3_hello_timeout_ms: env_u64("NODE3_HELLO_TIMEOUT_MS", 10_000),
            node3_reconnect_min_ms: env_u64("NODE3_RECONNECT_MIN_MS", 1_000),
            node3_reconnect_max_ms: env_u64("NODE3_RECONNECT_MAX_MS", 30_000),
            node3_stale_ms: env_u64("NODE3_STALE_MS", 45_000) as i64,

            execution_ledger_file: env_str("EXECUTION_LEDGER_FILE", DEFAULT_EXECUTION_LEDGER_FILE),

            mcp_chelsea_url: env_opt("MCP_CHELSEA_URL"),
            deriv_demo_api: env_opt("DERIV_DEMO_API"),
            deriv_app_id: env_opt("DERIV_APP_ID"),
            // Default to Deriv's current API: REST (accounts + one-time-password
            // WebSocket URLs) at https://api.derivws.com. The legacy
            // `wss://ws.derivws.com/websockets/v3` endpoint frequently answers
            // HTTP 520 from cloud/VPS networks, so it is now only a fallback
            // (or an explicit override via DERIV_API_URL=wss://...).
            deriv_api_url: env_str("DERIV_API_URL", "https://api.derivws.com"),
            deriv_min_stake: env_f64("DERIV_MIN_STAKE", DEFAULT_DERIV_MIN_STAKE),
            // Node 4-owned stake for the Deriv options venue. `ORDER_SIZE` is
            // accepted as a deprecated alias so an existing deployment does not
            // silently change size mid-migration.
            execution_stake: env::var("EXECUTION_STAKE")
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .or_else(|| {
                    env::var("ORDER_SIZE")
                        .ok()
                        .and_then(|v| v.trim().parse().ok())
                })
                .unwrap_or(DEFAULT_DERIV_MIN_STAKE),

            mt5_bridge_token: env_opt("MT5_BRIDGE_TOKEN"),
            mt5_control_token: env_opt("MT5_CONTROL_TOKEN"),
            mt5_symbol: env_str("MT5_SYMBOL", "XAUUSD"),
            mt5_symbol_map,
            mt5_volume_lots: env_f64("MT5_VOLUME_LOTS", DEFAULT_MT5_VOLUME_LOTS),
            mt5_order_timeout_ms: env_u64("MT5_ORDER_TIMEOUT_MS", 15_000),
            mt5_history_page_size: env_u64("MT5_HISTORY_PAGE_SIZE", 100) as usize,
            mt5_max_risk_per_trade: env::var("MT5_MAX_RISK_PER_TRADE")
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| *v > 0.0),
            mt5_allow_other_symbol: env_truthy("MT5_ALLOW_OTHER_SYMBOL"),

            execution_venue_override: env_opt("EXECUTION_VENUE"),
            venue: ExecutionVenue::None,
            symbol_map_errors: Vec::new(),

            // The feed is mandatory in local-strategy mode and a read-only
            // monitor otherwise; an explicit NODE1_FEED_ENABLED=1 turns the
            // monitor on in Node 3 mode as well.
            node1_feed_enabled: env_truthy("NODE1_FEED_ENABLED")
                || intent_source == IntentSource::Node1Strategy,
            node1_ws_url: env_str("NODE1_WS_URL", ""),
            node1_feed_token: env_opt("NODE1_FEED_TOKEN"),
            node1_feed_require_auth: env_truthy("NODE1_FEED_REQUIRE_AUTH"),
            node1_require_tls: match env::var("NODE1_REQUIRE_TLS") {
                Ok(value) => !matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                ),
                Err(_) => true,
            },
            node1_contract_version: env_str("NODE1_CONTRACT_VERSION", "1.0"),
            node1_symbol: env_str("NODE1_SYMBOL", "XAUUSD"),
            node1_interval: env_str("NODE1_INTERVAL", "15m"),
            node1_heartbeat_timeout_ms: env_u64("NODE1_HEARTBEAT_TIMEOUT_MS", 75_000) as i64,
            node1_stale_ms: env_u64("NODE1_STALE_MS", 60_000) as i64,
            node1_max_candle_age_ms: env_u64("NODE1_MAX_CANDLE_AGE_MS", 1_800_000) as i64,
            node1_reconnect_min_ms: env_u64("NODE1_RECONNECT_MIN_MS", 1_000),
            node1_reconnect_max_ms: env_u64("NODE1_RECONNECT_MAX_MS", 30_000),
            node1_min_closed_candles: env_u64("NODE1_MIN_CLOSED_CANDLES", 15),

            strategy_approved: env_truthy("NODE4_STRATEGY_APPROVED"),
            strategy_volume_threshold: env_f64("NODE4_STRATEGY_VOLUME_THRESHOLD", 10_500.0),
            strategy_sl_min_pips: env_f64("NODE4_STRATEGY_SL_MIN_PIPS", 200.0),
            strategy_sl_max_pips: env_f64("NODE4_STRATEGY_SL_MAX_PIPS", 300.0),
            strategy_tp_min_pips: env_f64("NODE4_STRATEGY_TP_MIN_PIPS", 600.0),
            strategy_tp_max_pips: env_f64("NODE4_STRATEGY_TP_MAX_PIPS", 800.0),
            strategy_rr_min: env_f64("NODE4_STRATEGY_RR_MIN", 2.0),
            strategy_rr_max: env_f64("NODE4_STRATEGY_RR_MAX", 3.0),
            strategy_ttl_secs: env_u64("NODE4_STRATEGY_TTL_SECS", 120),
            strategy_atr_fallback: env_f64("NODE4_STRATEGY_ATR_FALLBACK", 3.0),

            demo_integration_passed: env_truthy("NODE4_DEMO_INTEGRATION_PASSED"),
            live_trading_approved: env_truthy("NODE4_LIVE_TRADING_APPROVED"),
        };
        config.symbol_map_errors = symbol_map_errors;
        config.normalise_strategy();
        config.resolve_venue();
        config
    }

    /// Clamp strategy parameters exactly like Node 3's `Config::from_env` did,
    /// so a malformed environment cannot produce unsafe prices.
    fn normalise_strategy(&mut self) {
        self.strategy_ttl_secs = self.strategy_ttl_secs.clamp(5, 3_600);
        self.strategy_volume_threshold = self.strategy_volume_threshold.max(0.0);
        self.strategy_sl_min_pips = self.strategy_sl_min_pips.max(1.0);
        self.strategy_sl_max_pips = self.strategy_sl_max_pips.max(self.strategy_sl_min_pips);
        self.strategy_tp_min_pips = self.strategy_tp_min_pips.max(1.0);
        self.strategy_tp_max_pips = self.strategy_tp_max_pips.max(self.strategy_tp_min_pips);
        self.strategy_rr_min = self.strategy_rr_min.max(0.1);
        self.strategy_rr_max = self.strategy_rr_max.max(self.strategy_rr_min);
        self.strategy_atr_fallback =
            if self.strategy_atr_fallback.is_finite() && self.strategy_atr_fallback > 0.0 {
                self.strategy_atr_fallback
            } else {
                3.0
            };
        self.node1_heartbeat_timeout_ms = self.node1_heartbeat_timeout_ms.max(1_000);
        self.node1_stale_ms = self.node1_stale_ms.max(1_000);
        self.node1_max_candle_age_ms = self.node1_max_candle_age_ms.max(1_000);
        self.node1_min_closed_candles = self.node1_min_closed_candles.max(1);
    }

    /// Store the venue decision in `venue`, leaving the fail-closed
    /// `ExecutionVenue::None` state when the configuration is contradictory.
    /// Idempotent: the decision is always recomputed from the current fields.
    pub fn resolve_venue(&mut self) {
        self.venue = self.venue_decision().unwrap_or(ExecutionVenue::None);
    }

    /// The validated venue (see `venue_selection_error` for why it may be
    /// `None`, which is the fail-closed state).
    pub fn execution_venue(&self) -> ExecutionVenue {
        self.venue
    }

    /// Venue selection failure, if any. A configured-but-unusable venue is a
    /// hard error: there is deliberately no fallback to another venue, and no
    /// fallback to signal-only execution.
    ///
    /// Recomputed from the current configuration every time, so a `Config` that
    /// was mutated after construction fails closed just like one built by
    /// [`Config::from_env`].
    pub fn venue_selection_error(&self) -> Option<String> {
        self.venue_decision().err()
    }

    /// Parsed feed interval in milliseconds ("15m", "1h", "1d").
    pub fn node1_interval_ms(&self) -> Result<i64, String> {
        parse_interval_ms(&self.node1_interval)
    }

    /// True when a feed token is configured (sent as a bearer header).
    pub fn node1_auth_configured(&self) -> bool {
        self.node1_feed_token.is_some()
    }

    /// The Node 1 URL may be used with the configured transport policy.
    ///
    /// A plaintext `ws://` URL is only accepted for loopback development; every
    /// remote feed must be `wss://` unless `NODE1_REQUIRE_TLS=0` explicitly
    /// overrides it (for a trusted private network).
    pub fn node1_url_allowed(&self) -> Result<(), String> {
        let url = self.node1_ws_url.trim();
        if url.is_empty() {
            return Err(
                "NODE1_WS_URL is not set — the local strategy needs Node 1's market feed".into(),
            );
        }
        if !(url.starts_with("ws://") || url.starts_with("wss://")) {
            return Err(format!(
                "NODE1_WS_URL must be a ws:// or wss:// URL, got '{url}'"
            ));
        }
        if url.starts_with("ws://") && self.node1_require_tls {
            let host = url_host(url).unwrap_or("");
            let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
            if !loopback {
                return Err(format!(
                    "NODE1_WS_URL='{url}' is plaintext ws:// for a non-loopback host; the \
                     market feed must be wss:// (set NODE1_REQUIRE_TLS=0 only on a trusted \
                     private network)"
                ));
            }
        }
        Ok(())
    }

    /// Startup errors that must stop the process entirely. A Node 4 that cannot
    /// authenticate to Node 3, cannot validate its feed contract, or whose venue
    /// configuration is contradictory must not run: it could otherwise appear
    /// healthy while executing nothing (or something unexpected).
    pub fn startup_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if let Some(err) = &self.intent_source_error {
            errors.push(err.clone());
        }

        match self.intent_source {
            IntentSource::Node3 => {
                if self.node3_ws_url.trim().is_empty() {
                    errors.push(
                        "NODE3_WS_URL is not set — in `node3` mode Node 4 is an outbound \
                         client of the strategy service and cannot accept intents without it"
                            .into(),
                    );
                } else if !(self.node3_ws_url.starts_with("ws://")
                    || self.node3_ws_url.starts_with("wss://"))
                {
                    errors.push(format!(
                        "NODE3_WS_URL must be a ws:// or wss:// URL, got '{}'",
                        self.node3_ws_url
                    ));
                }
                if self.node4_shared_token.is_none() {
                    errors.push(
                        "NODE4_SHARED_TOKEN is not set — the Node 3 handshake would be rejected \
                         (set the same value on Node 3 and Node 4)"
                            .into(),
                    );
                }
            }
            IntentSource::Node1Strategy => {
                if !self.node1_feed_enabled {
                    errors.push(
                        "NODE1_FEED_ENABLED must be 1 when NODE4_INTENT_SOURCE=node1_strategy"
                            .into(),
                    );
                }
                if !self.strategy_approved {
                    errors.push(
                        "NODE4_STRATEGY_APPROVED is not set — `node1_strategy` mode runs a \
                         ported strategy against a draft feed contract; set it only after the \
                         Node 1 contract and this strategy specification are signed off"
                            .into(),
                    );
                }
            }
        }

        if self.node1_feed_enabled {
            if let Err(err) = self.node1_url_allowed() {
                errors.push(err);
            }
            if let Err(err) = self.node1_interval_ms() {
                errors.push(err);
            }
            if self.node1_feed_require_auth && self.node1_feed_token.is_none() {
                errors.push("NODE1_FEED_REQUIRE_AUTH=1 but NODE1_FEED_TOKEN is not set".into());
            }
            if self.node1_symbol.trim().is_empty() {
                errors.push("NODE1_SYMBOL must not be empty".into());
            }
            if self.node1_contract_version.trim().is_empty() {
                errors.push("NODE1_CONTRACT_VERSION must not be empty".into());
            }
        }

        if let Some(err) = self.venue_selection_error() {
            errors.push(format!("execution venue configuration error: {err}"));
        }
        if self.venue.is_real() {
            if let Some(err) = self.symbol_map_errors.first() {
                errors.push(err.clone());
            }
            if self.validation_stake().is_err() {
                errors.push(self.validation_stake().unwrap_err());
            }
            if self.intent_source.is_local() {
                if self.venue == ExecutionVenue::ChelseaLive && !self.live_trading_approved {
                    errors.push(
                        "EXECUTION_VENUE=chelsea_live is a live venue; set \
                         NODE4_LIVE_TRADING_APPROVED=1 only after the demo integration gate has \
                         passed and live trading is explicitly approved"
                            .into(),
                    );
                }
                if !self.demo_integration_passed {
                    errors.push(
                        "NODE4_DEMO_INTEGRATION_PASSED is not set — the local strategy must \
                         complete the demo integration gate before it may drive a real venue"
                            .into(),
                    );
                }
            }
        }
        errors
    }

    fn validation_stake(&self) -> Result<(), String> {
        if !self.execution_stake.is_finite() || self.execution_stake <= 0.0 {
            return Err(format!(
                "EXECUTION_STAKE must be a positive USD amount, got {}",
                self.execution_stake
            ));
        }
        Ok(())
    }

    /// True when `DERIV_APP_ID` is set to a non-empty value.
    ///
    /// Required for PAT (`pat_...`) tokens: Deriv rejects REST calls from a PAT
    /// without a `Deriv-App-ID` header (`HTTP 401: Deriv-App-ID header is
    /// required for PAT tokens`). Legacy `a1-...` tokens do not need it.
    pub fn deriv_app_id_configured(&self) -> bool {
        self.deriv_app_id
            .as_deref()
            .map(|id| !id.trim().is_empty())
            .unwrap_or(false)
    }

    /// Build the Deriv WebSocket endpoint URL.
    ///
    /// An `app_id` query parameter is appended only when `DERIV_APP_ID` is
    /// explicitly configured (or already embedded in `DERIV_API_URL`). The
    /// current Deriv API (REST + OTP WebSocket URLs) does not use an `app_id`
    /// query parameter at all, and the legacy-flow failover in
    /// `execution_deriv` automatically retries the official test id `1089`
    /// when no app id is configured.
    pub fn deriv_ws_url(&self) -> String {
        let base = self.deriv_api_url.trim();
        if base.contains("app_id=") {
            return base.to_string();
        }
        let Some(app_id) = self
            .deriv_app_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        else {
            return base.to_string();
        };
        if base.contains('?') {
            format!("{base}&app_id={app_id}")
        } else {
            format!("{base}?app_id={app_id}")
        }
    }

    // ---- Venue helpers ----------------------------------------------------

    pub fn deriv_configured(&self) -> bool {
        self.deriv_demo_api.is_some()
    }

    pub fn chelsea_configured(&self) -> bool {
        self.mcp_chelsea_url.is_some()
    }

    pub fn mt5_configured(&self) -> bool {
        self.mt5_bridge_token.is_some()
    }

    /// True when `POST /mt5/control` is usable (a control token is configured).
    pub fn mt5_control_enabled(&self) -> bool {
        self.mt5_control_token.is_some()
    }

    /// Resolve the requested→broker symbol mapping for a symbol name.
    pub fn mt5_broker_symbol(&self, requested: &str) -> Option<&str> {
        self.mt5_symbol_map
            .iter()
            .find(|(from, _)| from == requested)
            .map(|(_, to)| to.as_str())
    }

    /// Resolve an intent symbol to the broker symbol, refusing anything that is
    /// not supported or not explicitly mapped.
    ///
    /// An exact match may pass without a mapping (the broker's symbol is the
    /// requested symbol); anything else — `XAUUSD.a`, `GOLD`, ... — requires an
    /// explicit `MT5_SYMBOL_MAP` entry. Broker suffixes are never guessed.
    pub fn resolve_symbol(&self, requested: &str) -> Result<String, String> {
        let requested = requested.trim();
        if requested.is_empty() {
            return Err("intent symbol is empty".into());
        }
        if requested.eq_ignore_ascii_case(&self.mt5_symbol) {
            return Ok(self.mt5_symbol.clone());
        }
        if let Some(mapped) = self.mt5_broker_symbol(requested) {
            return Ok(mapped.to_string());
        }
        Err(format!(
            "symbol '{requested}' is not supported: this service is configured for '{}' and \
             only an explicit MT5_SYMBOL_MAP entry can map another symbol to a broker symbol",
            self.mt5_symbol
        ))
    }

    /// The venue to run, or the reason the configuration is unusable.
    ///
    /// Mutual exclusivity is enforced: two configured venues without an
    /// explicit `EXECUTION_VENUE` is an error rather than a silent precedence
    /// rule — Node 4 must never be able to trade two accounts at once.
    pub fn venue_decision(&self) -> Result<ExecutionVenue, String> {
        let configured = [
            (ExecutionVenue::DerivMt5Demo, self.mt5_configured()),
            (ExecutionVenue::DerivDemo, self.deriv_configured()),
            (ExecutionVenue::ChelseaLive, self.chelsea_configured()),
        ];
        let configured_names: Vec<&str> = configured
            .iter()
            .filter(|(_, is_set)| *is_set)
            .map(|(venue, _)| venue.label())
            .collect();

        let venue = match self.execution_venue_override.as_deref() {
            Some(raw) => {
                let Some(venue) = ExecutionVenue::parse(raw) else {
                    return Err(format!(
                        "EXECUTION_VENUE='{raw}' is not recognised \
                         (expected deriv_mt5_demo | deriv_demo | chelsea_live | none)"
                    ));
                };
                if venue != ExecutionVenue::None {
                    let is_configured = match venue {
                        ExecutionVenue::DerivMt5Demo => self.mt5_configured(),
                        ExecutionVenue::DerivDemo => self.deriv_configured(),
                        ExecutionVenue::ChelseaLive => self.chelsea_configured(),
                        ExecutionVenue::None => true,
                    };
                    if !is_configured {
                        return Err(format!(
                            "EXECUTION_VENUE={} is selected but its credentials are missing \
                             ({})",
                            venue.label(),
                            match venue {
                                ExecutionVenue::DerivMt5Demo => "MT5_BRIDGE_TOKEN",
                                ExecutionVenue::DerivDemo => "DERIV_DEMO_API",
                                ExecutionVenue::ChelseaLive => "MCP_CHELSEA_URL",
                                ExecutionVenue::None => "",
                            }
                        ));
                    }
                    for (other, is_set) in configured {
                        if is_set && other != venue {
                            return Err(format!(
                                "EXECUTION_VENUE={} is selected but {} is also configured — \
                                 venues are mutually exclusive; unset one of them",
                                venue.label(),
                                other.label()
                            ));
                        }
                    }
                }
                venue
            }
            None => match configured_names.len() {
                0 => ExecutionVenue::None,
                1 => configured
                    .iter()
                    .find(|(_, is_set)| *is_set)
                    .map(|(venue, _)| *venue)
                    .unwrap_or(ExecutionVenue::None),
                _ => {
                    return Err(format!(
                        "multiple execution venues are configured ({}); set EXECUTION_VENUE to \
                         exactly one of them",
                        configured_names.join(", ")
                    ))
                }
            },
        };

        if venue == ExecutionVenue::DerivMt5Demo {
            if !self.mt5_symbol.eq_ignore_ascii_case("XAUUSD") && !self.mt5_allow_other_symbol {
                return Err(format!(
                    "MT5_SYMBOL='{}' but this service is XAUUSD-only; set \
                     MT5_ALLOW_OTHER_SYMBOL=1 only if a different instrument is intended",
                    self.mt5_symbol
                ));
            }
            if !self.mt5_volume_lots.is_finite() || self.mt5_volume_lots <= 0.0 {
                return Err(format!(
                    "MT5_VOLUME_LOTS must be a positive number of lots, got {}",
                    self.mt5_volume_lots
                ));
            }
            if let Some(err) = self.symbol_map_errors.first() {
                return Err(err.clone());
            }
        }

        Ok(venue)
    }
}

/// Parse a feed interval (`15m`, `1h`, `1d`) into milliseconds.
///
/// Only integer values with a lowercase `m`/`h`/`d` unit are accepted; the
/// contract names intervals in exactly that form, and a guessed unit would
/// silently misalign every candle bucket.
pub fn parse_interval_ms(value: &str) -> Result<i64, String> {
    let value = value.trim();
    if value.len() < 2 {
        return Err(format!(
            "interval '{value}' is not `<number><m|h|d>` (for example 15m)"
        ));
    }
    let (number, unit) = value.split_at(value.len() - 1);
    let amount: u64 = number
        .parse()
        .map_err(|_| format!("interval '{value}' has no integer number"))?;
    if amount == 0 {
        return Err(format!("interval '{value}' must be greater than zero"));
    }
    let unit_ms: u64 = match unit {
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => {
            return Err(format!(
                "interval '{value}' has unit '{unit}'; expected m, h or d"
            ))
        }
    };
    amount
        .checked_mul(unit_ms)
        .map(|ms| ms as i64)
        .ok_or_else(|| format!("interval '{value}' is too large"))
}

/// Host component of a `ws(s)://` URL, without userinfo, port or path.
fn url_host(url: &str) -> Option<&str> {
    let rest = url.split("://").nth(1)?;
    let authority = rest.split('/').next()?;
    let host_port = authority.rsplit('@').next()?;
    if host_port.starts_with('[') {
        // Bracketed IPv6 literal: keep the brackets, drop any port.
        return host_port.split(']').next().map(|host| {
            // Re-add the bracket so callers can compare with "[::1]".
            if host.is_empty() {
                host_port
            } else {
                &host_port[..host.len() + 1]
            }
        });
    }
    Some(host_port.split(':').next().unwrap_or(host_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Config {
        Config {
            node3_ws_url: "wss://strategy.example/execution".into(),
            node4_shared_token: Some("shared-token".into()),
            deriv_demo_api: Some("test-token".into()),
            venue: ExecutionVenue::DerivDemo,
            ..Default::default()
        }
    }

    #[test]
    fn deriv_ws_url_does_not_force_app_id() {
        let config = base();
        // Current REST base is returned untouched.
        assert_eq!(config.deriv_ws_url(), "https://api.derivws.com");

        // Legacy wss base is also returned untouched when no app id is set.
        let mut legacy = base();
        legacy.deriv_api_url = "wss://ws.derivws.com/websockets/v3".into();
        assert_eq!(legacy.deriv_ws_url(), "wss://ws.derivws.com/websockets/v3");
    }

    #[test]
    fn deriv_ws_url_appends_configured_app_id() {
        let mut config = base();
        config.deriv_api_url = "wss://ws.derivws.com/websockets/v3".into();
        config.deriv_app_id = Some("12345".into());
        assert_eq!(
            config.deriv_ws_url(),
            "wss://ws.derivws.com/websockets/v3?app_id=12345"
        );

        // An app id already embedded in the URL wins (no duplication).
        let mut embedded = base();
        embedded.deriv_api_url = "wss://ws.derivws.com/websockets/v3?app_id=999".into();
        embedded.deriv_app_id = Some("12345".into());
        assert_eq!(
            embedded.deriv_ws_url(),
            "wss://ws.derivws.com/websockets/v3?app_id=999"
        );
    }

    #[test]
    fn detects_whether_deriv_app_id_is_configured() {
        let mut config = base();
        assert!(!config.deriv_app_id_configured());

        config.deriv_app_id = Some("1089".into());
        assert!(config.deriv_app_id_configured());

        // Blank / whitespace-only values count as "not set".
        config.deriv_app_id = Some("   ".into());
        assert!(!config.deriv_app_id_configured());
        config.deriv_app_id = Some(String::new());
        assert!(!config.deriv_app_id_configured());
    }

    #[test]
    fn venue_labels_and_parsing_round_trip() {
        for venue in [
            ExecutionVenue::DerivMt5Demo,
            ExecutionVenue::DerivDemo,
            ExecutionVenue::ChelseaLive,
            ExecutionVenue::None,
        ] {
            assert_eq!(ExecutionVenue::parse(venue.label()), Some(venue));
        }
        assert_eq!(
            ExecutionVenue::parse("MT5"),
            Some(ExecutionVenue::DerivMt5Demo)
        );
        assert_eq!(ExecutionVenue::parse("nonsense"), None);
        assert!(ExecutionVenue::DerivDemo.is_real());
        assert!(!ExecutionVenue::None.is_real());
    }

    #[test]
    fn legacy_single_venue_configuration_still_resolves() {
        let config = base();
        assert_eq!(config.venue_decision().unwrap(), ExecutionVenue::DerivDemo);
    }

    #[test]
    fn mt5_token_alone_selects_the_mt5_venue() {
        let mut config = base();
        config.deriv_demo_api = None;
        config.mt5_bridge_token = Some("bridge-token".into());
        assert_eq!(
            config.venue_decision().unwrap(),
            ExecutionVenue::DerivMt5Demo
        );
    }

    #[test]
    fn venues_are_mutually_exclusive_and_fail_closed() {
        let mut config = base();
        config.mt5_bridge_token = Some("bridge-token".into());
        // DERIV_DEMO_API from `base()` plus MT5 without an explicit choice.
        let err = config.venue_decision().unwrap_err();
        assert!(err.contains("multiple execution venues"));
        assert!(err.contains("EXECUTION_VENUE"));

        // An explicit selection does NOT paper over a second configured venue:
        // the operator must unset one, so Node 4 can never trade the wrong
        // account by accident.
        config.execution_venue_override = Some("deriv_mt5_demo".into());
        let err = config.venue_decision().unwrap_err();
        assert!(err.contains("mutually exclusive"));

        // With the other venue's credential gone, the explicit choice resolves.
        config.deriv_demo_api = None;
        assert_eq!(
            config.venue_decision().unwrap(),
            ExecutionVenue::DerivMt5Demo
        );

        // Selecting a venue without its credentials is refused.
        let mut missing = base();
        missing.deriv_demo_api = None;
        missing.execution_venue_override = Some("deriv_mt5_demo".into());
        let err = missing.venue_decision().unwrap_err();
        assert!(err.contains("MT5_BRIDGE_TOKEN"));
    }

    #[test]
    fn unknown_venue_override_is_refused() {
        let mut config = base();
        config.execution_venue_override = Some("iq_option".into());
        assert!(config.venue_decision().is_err());
    }

    #[test]
    fn mt5_symbol_and_volume_are_validated() {
        let mut config = base();
        config.deriv_demo_api = None;
        config.mt5_bridge_token = Some("t".into());

        config.mt5_symbol = "EURUSD".into();
        assert!(config.venue_decision().is_err());
        config.mt5_allow_other_symbol = true;
        assert!(config.venue_decision().is_ok());

        config.mt5_allow_other_symbol = false;
        config.mt5_symbol = "XAUUSD".into();
        config.mt5_volume_lots = 0.0;
        assert!(config.venue_decision().is_err());
        config.mt5_volume_lots = DEFAULT_MT5_VOLUME_LOTS;
        assert!(config.venue_decision().is_ok());

        // A malformed MT5_SYMBOL_MAP entry is a hard error.
        config.symbol_map_errors = vec!["MT5_SYMBOL_MAP entry 'XAUUSD' is not a pair".into()];
        assert!(config.venue_decision().is_err());
    }

    #[test]
    fn mt5_broker_symbol_mapping_is_explicit() {
        let mut config = base();
        config.mt5_symbol_map = vec![("XAUUSD".into(), "XAUUSD.a".into())];
        assert_eq!(config.mt5_broker_symbol("XAUUSD"), Some("XAUUSD.a"));
        assert_eq!(config.mt5_broker_symbol("XAUUSD.a"), None);
    }

    #[test]
    fn symbol_resolution_is_exact_or_explicitly_mapped() {
        let mut config = base();
        // Exact match (case-insensitive) passes without a mapping.
        assert_eq!(config.resolve_symbol("XAUUSD").unwrap(), "XAUUSD");
        assert_eq!(config.resolve_symbol("xauusd").unwrap(), "XAUUSD");
        // Unmapped suffix fails closed.
        assert!(config.resolve_symbol("XAUUSD.a").is_err());
        assert!(config.resolve_symbol("GOLD").is_err());
        assert!(config.resolve_symbol("  ").is_err());
        // An explicit mapping resolves.
        config.mt5_symbol_map = vec![("XAUUSD.a".into(), "XAUUSD.a".into())];
        assert_eq!(config.resolve_symbol("XAUUSD.a").unwrap(), "XAUUSD.a");
    }

    #[test]
    fn mt5_volume_default_is_not_the_options_stake() {
        let config = base();
        assert_eq!(config.execution_stake, DEFAULT_DERIV_MIN_STAKE);
        assert_eq!(config.mt5_volume_lots, DEFAULT_MT5_VOLUME_LOTS);
        // The two knobs are independent on purpose.
        assert_ne!(DEFAULT_MT5_VOLUME_LOTS, DEFAULT_DERIV_MIN_STAKE);
    }

    #[test]
    fn stored_venue_follows_the_decision_and_reports_errors() {
        let mut config = base();
        assert_eq!(config.execution_venue(), ExecutionVenue::DerivDemo);
        assert!(config.venue_selection_error().is_none());

        // A failure leaves the venue as None *and* explains why.
        config.mt5_bridge_token = Some("t".into());
        assert!(config
            .venue_selection_error()
            .unwrap()
            .contains("multiple execution venues"));
        config.resolve_venue();
        assert_eq!(config.execution_venue(), ExecutionVenue::None);
        assert!(config
            .venue_selection_error()
            .unwrap()
            .contains("multiple execution venues"));
    }

    #[test]
    fn mt5_control_requires_a_token() {
        let mut config = base();
        assert!(!config.mt5_control_enabled());
        config.mt5_control_token = Some("control-token".into());
        assert!(config.mt5_control_enabled());
    }

    #[test]
    fn intervals_parse_only_the_contract_units() {
        assert_eq!(parse_interval_ms("15m").unwrap(), 900_000);
        assert_eq!(parse_interval_ms("1h").unwrap(), 3_600_000);
        assert_eq!(parse_interval_ms("1d").unwrap(), 86_400_000);
        for bad in ["", "15", "15x", "0m", "m", "1.5h"] {
            assert!(parse_interval_ms(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn intent_source_parsing_is_explicit_and_fail_closed() {
        assert_eq!(IntentSource::parse("node3"), Some(IntentSource::Node3));
        assert_eq!(
            IntentSource::parse("node1_strategy"),
            Some(IntentSource::Node1Strategy)
        );
        assert_eq!(
            IntentSource::parse("node1"),
            Some(IntentSource::Node1Strategy)
        );
        assert_eq!(IntentSource::parse("nonsense"), None);
        assert!(!IntentSource::Node3.is_local());
        assert!(IntentSource::Node1Strategy.is_local());
    }

    #[test]
    fn local_strategy_mode_requires_the_feed_the_gate_and_the_approval() {
        let mut config = Config {
            intent_source: IntentSource::Node1Strategy,
            node1_feed_enabled: true,
            node1_ws_url: "wss://engine.example/ws".into(),
            ..Default::default()
        };
        let errors = config.startup_errors();
        assert!(errors.iter().any(|e| e.contains("NODE4_STRATEGY_APPROVED")));
        assert!(
            !errors.iter().any(|e| e.contains("NODE3_WS_URL")),
            "node3 variables must not be required in local mode"
        );

        config.strategy_approved = true;
        assert!(config.startup_errors().is_empty());

        // A real venue additionally needs the demo gate.
        config.deriv_demo_api = Some("token".into());
        config.resolve_venue();
        let errors = config.startup_errors();
        assert!(errors
            .iter()
            .any(|e| e.contains("NODE4_DEMO_INTEGRATION_PASSED")));
        config.demo_integration_passed = true;
        assert!(config.startup_errors().is_empty());
    }

    #[test]
    fn a_plaintext_feed_is_only_allowed_for_loopback() {
        let mut config = Config {
            node1_feed_enabled: true,
            node1_ws_url: "ws://engine.example/ws".into(),
            ..Default::default()
        };
        assert!(config.node1_url_allowed().is_err());
        config.node1_ws_url = "ws://localhost:3000/ws".into();
        assert!(config.node1_url_allowed().is_ok());
        config.node1_ws_url = "wss://engine.example/ws".into();
        assert!(config.node1_url_allowed().is_ok());
        config.node1_ws_url = "https://engine.example/ws".into();
        assert!(config.node1_url_allowed().is_err());
    }

    #[test]
    fn an_unknown_intent_source_is_a_startup_error() {
        let config = Config {
            intent_source_error: Some("NODE4_INTENT_SOURCE='nope'".into()),
            node3_ws_url: "wss://strategy.example/execution".into(),
            node4_shared_token: Some("t".into()),
            ..Default::default()
        };
        assert!(config
            .startup_errors()
            .iter()
            .any(|e| e.contains("NODE4_INTENT_SOURCE")));
    }

    #[test]
    fn startup_requires_the_node3_link_and_a_token() {
        let mut config = Config::default();
        let errors = config.startup_errors();
        assert!(errors.iter().any(|e| e.contains("NODE3_WS_URL")));
        assert!(errors.iter().any(|e| e.contains("NODE4_SHARED_TOKEN")));

        config.node3_ws_url = "https://not-a-websocket/execution".into();
        config.node4_shared_token = Some("t".into());
        let errors = config.startup_errors();
        assert!(errors.iter().any(|e| e.contains("ws:// or wss://")));

        config.node3_ws_url = "wss://strategy.example/execution".into();
        assert!(config.startup_errors().is_empty());
    }

    #[test]
    fn startup_refuses_a_contradictory_venue() {
        let mut config = base();
        config.mt5_bridge_token = Some("t".into());
        // Two credentials and no explicit choice.
        let errors = config.startup_errors();
        assert!(errors.iter().any(|e| e.contains("venue")));

        // A non-positive stake for a real venue is refused.
        let mut config = base();
        config.execution_stake = 0.0;
        let errors = config.startup_errors();
        assert!(errors.iter().any(|e| e.contains("EXECUTION_STAKE")));
    }
}
