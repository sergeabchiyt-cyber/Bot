use std::env;

/// Default for `DERIV_MIN_STAKE`: the smallest stake Deriv prices for
/// `frxXAUUSD` on a USD options account. Deriv answers a smaller stake with
/// `InvalidMinStake` (`Please enter a stake amount that's at least 0.50.`), so
/// `ORDER_SIZE` below it is clamped up in the Deriv flow.
pub const DEFAULT_DERIV_MIN_STAKE: f64 = 0.50;

/// Default lot size for the MT5 demo venue. **Not** `ORDER_SIZE`: that value is
/// a USD options stake for the Deriv options venue and has nothing to do with a
/// MetaTrader lot size (1 lot of XAUUSD = 100 oz on Deriv MT5).
pub const DEFAULT_MT5_VOLUME_LOTS: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionVenue {
    /// Deriv MT5 **demo** account, driven through the `mt5-bridge` service that
    /// runs next to the terminal. See `docs/mt5/EXECUTION_ARCHITECTURE.md`.
    DerivMt5Demo,
    /// Deriv options (Rise/Fall contracts) — the existing `DERIV_DEMO_API`
    /// integration. Never used for MT5 orders.
    DerivDemo,
    ChelseaLive,
    /// No venue: signals are recorded and broadcast, no orders are sent.
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
            "none" | "signal_only" | "signal" => Some(ExecutionVenue::None),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub node1_ws_url: String,
    /// Port for the local HTTP + WebSocket server (`GET /health`, `GET /diagnostics`, `WS /ws`). 10000 by default.
    pub port: u16,
    pub mcp_chelsea_url: Option<String>,
    pub deriv_demo_api: Option<String>,
    pub deriv_app_id: Option<String>,
    pub deriv_api_url: String,
    /// Minimum stake accepted by the Deriv venue (USD). `ORDER_SIZE` used to
    /// default to 0.01, which Deriv rejects with `InvalidMinStake:
    /// Please enter a stake amount that's at least 0.50.`
    pub deriv_min_stake: f64,

    // ---- MT5 demo bridge (venue `deriv_mt5_demo`) -------------------------
    /// `MT5_BRIDGE_TOKEN`: the shared secret the bridge must present in
    /// `bridge_hello`. It is a **service** credential, not the MT5 account
    /// password, and the same value must exist in the bridge environment.
    pub mt5_bridge_token: Option<String>,
    /// `MT5_CONTROL_TOKEN`: bearer token for `POST /mt5/control`
    /// (halt/resume/close). Unset ⇒ the endpoint answers 403 (fail closed).
    pub mt5_control_token: Option<String>,
    /// `MT5_SYMBOL`: the instrument the strategy trades (`XAUUSD`).
    pub mt5_symbol: String,
    /// `MT5_SYMBOL_MAP`: explicit requested→broker mapping (`XAUUSD=XAUUSD.a`).
    /// Broker suffixes are never guessed; an unmapped symbol fails closed.
    pub mt5_symbol_map: Vec<(String, String)>,
    /// `MT5_VOLUME_LOTS`: lot size used for MT5 market orders. Validated
    /// against the broker's min/max/step by the bridge before any send.
    pub mt5_volume_lots: f64,
    /// `MT5_ORDER_TIMEOUT_MS`: how long Node 3 waits for a broker-confirmed
    /// outcome before treating the order as unknown (never retried blindly).
    pub mt5_order_timeout_ms: u64,
    /// `MT5_HISTORY_PAGE_SIZE`: deals per `/mt5/history` page.
    pub mt5_history_page_size: usize,
    /// `MT5_MAX_RISK_PER_TRADE`: optional extra guard in account currency; an
    /// order whose stop distance implies more risk is refused before sending.
    pub mt5_max_risk_per_trade: Option<f64>,
    /// `MT5_ALLOW_OTHER_SYMBOL=1` allows `MT5_SYMBOL` to be something other than
    /// XAUUSD (the strategy is XAUUSD-only, so this is off by default).
    pub mt5_allow_other_symbol: bool,
    /// Explicit `EXECUTION_VENUE` selection, when set.
    pub execution_venue_override: Option<String>,
    /// The validated venue this process runs. `ExecutionVenue::None` when the
    /// configuration is unusable — see `venue_error`.
    pub venue: ExecutionVenue,
    /// Set when venue selection failed; the service must refuse to trade (and
    /// should refuse to start) rather than silently falling back to another
    /// venue or to signal-only mode.
    pub venue_error: Option<String>,
    /// Malformed `MT5_SYMBOL_MAP` entries collected at load time; surfaced as a
    /// hard error when the MT5 venue is selected.
    pub symbol_map_errors: Vec<String>,

    // Strategy Parameters
    pub volume_threshold: f64,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub order_size: f64,
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

impl Config {
    pub fn from_env() -> Self {
        let (mt5_symbol_map, symbol_map_errors) = env_pairs("MT5_SYMBOL_MAP");
        let mut config = Self {
            node1_ws_url: env_str("NODE1_WS_URL", "wss://engine-southeastasia-sng-main.onrender.com/ws"),
            // Hosting platforms inject PORT; we default to 10000 so the
            // health and WebSocket endpoints are reachable out of the box.
            port: env_u16("PORT", 10_000),
            mcp_chelsea_url: env_opt("MCP_CHELSEA_URL"),
            deriv_demo_api: env_opt("DERIV_DEMO_API"),
            deriv_app_id: env_opt("DERIV_APP_ID"),
            // Default to Deriv's current API: REST (accounts + one-time-password
            // WebSocket URLs) at https://api.derivws.com. The legacy
            // `wss://ws.derivws.com/websockets/v3` endpoint frequently answers
            // HTTP 520 from cloud/VPS networks, so it is now only a fallback
            // (or an explicit override via DERIV_API_URL=wss://...).
            deriv_api_url: env_str("DERIV_API_URL", "https://api.derivws.com"),
            // Deriv refuses stakes below the contract minimum ("Please enter
            // a stake amount that's at least 0.50."), so ORDER_SIZE below it
            // is clamped up rather than rejected.
            deriv_min_stake: env_f64("DERIV_MIN_STAKE", DEFAULT_DERIV_MIN_STAKE),

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

            volume_threshold: env_f64("VOLUME_THRESHOLD", 10_500.0),
            sl_min_pips: env_f64("SL_MIN_PIPS", 200.0),
            sl_max_pips: env_f64("SL_MAX_PIPS", 300.0),
            tp_min_pips: env_f64("TP_MIN_PIPS", 600.0),
            tp_max_pips: env_f64("TP_MAX_PIPS", 800.0),
            rr_min: env_f64("RR_MIN", 2.0),
            rr_max: env_f64("RR_MAX", 3.0),
            order_size: env_f64("ORDER_SIZE", 0.01),
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

    /// The validated venue (see `venue_error` for why it may be `None`).
    pub fn execution_venue(&self) -> ExecutionVenue {
        self.venue
    }

    /// Venue selection failure, if any. A configured-but-unusable venue is a
    /// hard error: there is deliberately no fallback to another venue, and no
    /// fallback to signal-only execution.
    pub fn venue_selection_error(&self) -> Option<&str> {
        self.venue_error.as_deref()
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

    // ---- MT5 venue --------------------------------------------------------

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

    /// The venue to run, or the reason the configuration is unusable.
    ///
    /// Mutual exclusivity is enforced: two configured venues without an
    /// explicit `EXECUTION_VENUE` is an error rather than a silent precedence
    /// rule — a strategy must never be able to trade two accounts at once.
    pub fn venue_decision(&self) -> Result<ExecutionVenue, String> {
        let configured = [
            (ExecutionVenue::DerivMt5Demo, self.mt5_configured()),
            (ExecutionVenue::DerivDemo, self.deriv_demo_api.is_some()),
            (ExecutionVenue::ChelseaLive, self.mcp_chelsea_url.is_some()),
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
                        ExecutionVenue::DerivDemo => self.deriv_demo_api.is_some(),
                        ExecutionVenue::ChelseaLive => self.mcp_chelsea_url.is_some(),
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
                    "MT5_SYMBOL='{}' but this strategy is XAUUSD-only; set \
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
            node1_ws_url: "wss://example.invalid/ws".into(),
            port: 10_000,
            mcp_chelsea_url: None,
            deriv_demo_api: Some("test-token".into()),
            deriv_app_id: None,
            deriv_api_url: "https://api.derivws.com".into(),
            deriv_min_stake: DEFAULT_DERIV_MIN_STAKE,
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
            venue: ExecutionVenue::DerivDemo,
            venue_error: None,
            symbol_map_errors: Vec::new(),
            volume_threshold: 10_500.0,
            sl_min_pips: 200.0,
            sl_max_pips: 300.0,
            tp_min_pips: 600.0,
            tp_max_pips: 800.0,
            rr_min: 2.0,
            rr_max: 3.0,
            order_size: 0.01,
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

        // An explicit selection resolves it.
        config.execution_venue_override = Some("deriv_mt5_demo".into());
        assert_eq!(config.venue_decision().unwrap(), ExecutionVenue::DerivMt5Demo);

        // …but not if the other venue is still configured.
        let mut conflicting = base();
        conflicting.mt5_bridge_token = Some("bridge-token".into());
        conflicting.execution_venue_override = Some("deriv_mt5_demo".into());
        let err = conflicting.venue_decision().unwrap_err();
        assert!(err.contains("mutually exclusive"));

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
        config.mt5_symbol_map = vec![("XAUUSD".into(), String::new())];
        // `env_pairs` filters those out; emulate the parsed error directly.
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
    fn mt5_volume_default_is_not_the_options_stake() {
        let config = base();
        assert_eq!(config.order_size, 0.01);
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
        // (the struct stores what `from_env` resolved; emulate its behaviour)
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

}
