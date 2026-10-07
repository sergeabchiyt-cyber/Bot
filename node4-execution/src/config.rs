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

#[derive(Clone, Debug)]
pub struct Config {
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
    /// The validated venue this process runs. `ExecutionVenue::None` when the
    /// configuration is unusable — see `venue_error`.
    pub venue: ExecutionVenue,
    /// Set when venue selection failed; the service must refuse to trade (and
    /// refuses to start) rather than silently falling back to another venue.
    pub venue_error: Option<String>,
    /// Malformed `MT5_SYMBOL_MAP` entries collected at load time; surfaced as a
    /// hard error when the MT5 venue is selected.
    pub symbol_map_errors: Vec<String>,
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
    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
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
            venue_error: None,
            symbol_map_errors: Vec::new(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let (mt5_symbol_map, symbol_map_errors) = env_pairs("MT5_SYMBOL_MAP");
        let mut config = Self {
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

            execution_ledger_file: env_str(
                "EXECUTION_LEDGER_FILE",
                DEFAULT_EXECUTION_LEDGER_FILE,
            ),

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
                .or_else(|| env::var("ORDER_SIZE").ok().and_then(|v| v.trim().parse().ok()))
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
            venue_error: None,
            symbol_map_errors: Vec::new(),
        };
        config.symbol_map_errors = symbol_map_errors;
        match config.venue_decision() {
            Ok(venue) => config.venue = venue,
            Err(err) => {
                config.venue = ExecutionVenue::None;
                config.venue_error = Some(err);
            }
        }
        config
    }

    /// The validated venue (see `venue_selection_error` for why it may be
    /// `None`, which is the fail-closed state).
    pub fn execution_venue(&self) -> ExecutionVenue {
        self.venue
    }

    /// Venue selection failure, if any. A configured-but-unusable venue is a
    /// hard error: there is deliberately no fallback to another venue, and no
    /// fallback to signal-only execution.
    pub fn venue_selection_error(&self) -> Option<&str> {
        self.venue_error.as_deref()
    }

    /// Startup errors that must stop the process entirely. A Node 4 that cannot
    /// authenticate to Node 3, or whose venue configuration is contradictory,
    /// must not run: it could otherwise appear healthy while executing nothing
    /// (or something unexpected).
    pub fn startup_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.node3_ws_url.trim().is_empty() {
            errors.push(
                "NODE3_WS_URL is not set — Node 4 is an outbound client of the strategy \
                 service and cannot accept intents without it"
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
        assert_eq!(
            legacy.deriv_ws_url(),
            "wss://ws.derivws.com/websockets/v3"
        );
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
        embedded.deriv_api_url =
            "wss://ws.derivws.com/websockets/v3?app_id=999".into();
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
        assert_eq!(ExecutionVenue::parse("MT5"), Some(ExecutionVenue::DerivMt5Demo));
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
        assert_eq!(config.venue_decision().unwrap(), ExecutionVenue::DerivMt5Demo);
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
        assert_eq!(config.venue_decision().unwrap(), ExecutionVenue::DerivMt5Demo);

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
        match config.venue_decision() {
            Ok(venue) => config.venue = venue,
            Err(err) => {
                config.venue = ExecutionVenue::None;
                config.venue_error = Some(err);
            }
        }
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
