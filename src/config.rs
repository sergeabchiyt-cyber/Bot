use std::env;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionVenue {
    DerivDemo,
    ChelseaLive,
    None,
}

#[derive(Clone)]
pub struct Config {
    pub port: u16,

    // ---- Chart / candles ----
    /// Binance is retained for order flow only; price history/live candles
    /// come from SiftingIO so VP calculations use one price domain.
    pub binance_ws_url: String,
    pub sifting_ws_url: String,
    pub sifting_hist_url: String,
    pub sifting_api_key: String,
    pub sifting_symbol: String,

    // ---- Order flow feeds ----
    pub feed_binance: bool,
    pub feed_bybit: bool,
    pub feed_okx: bool,
    pub feed_bitget: bool,
    pub feed_gate: bool,
    pub feed_kraken: bool,
    pub feed_alltick: bool,
    pub feed_itick: bool,
    pub feed_sifting: bool,

    pub bitget_symbol: String,
    pub gate_symbol: String,
    pub kraken_product: String,
    pub alltick_ws_url: String,
    pub alltick_code: String,
    pub itick_ws_url: String,
    pub itick_symbol: String,

    // ---- Strategy ----
    pub level_proximity_pips: f64,

    // ---- Browser MCP ----
    pub mcp_browser_url: String,
    pub mcp_browser_token: Option<String>,
    pub mcp_scrape_secs: u64,

    // ---- Economic calendar ----
    /// Override the direct calendar feed URL (defaults to the ForexFactory
    /// weekly export). Set empty to rely on the defaults.
    pub calendar_url: Option<String>,
    /// Use the browser MCP server as a fallback when the direct feed fails.
    pub calendar_use_mcp: bool,

    /// CI/offline only: if no historical candles could be fetched from any
    /// upstream, seed a deterministic synthetic series so the volume-profile
    /// endpoints are exercisable. Never enabled by default.
    pub seed_synthetic_candles: bool,

    // ---- Execution ----
    pub mcp_chelsea_url: Option<String>,
    pub deriv_api_url: String,
    pub deriv_app_id: Option<String>,
    pub deriv_demo_api: Option<String>,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
}

fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// tri-state feed switch: FEED_X=on|off|auto (auto = on only when its
/// credentials exist, or always-on for keyless public feeds).
fn env_switch(key: &str, has_credentials: bool) -> bool {
    match env::var(key).unwrap_or_default().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => true,
        "0" | "false" | "off" | "no" => false,
        _ => has_credentials, // auto
    }
}

impl Config {
    pub fn from_env() -> Self {
        let sifting_api_key = env_opt("SIFTING_API_KEY").unwrap_or_default();
        let has_sifting_key = !sifting_api_key.is_empty();
        let alltick_token = env_opt("ALLTICK_TOKEN");
        let itick_token = env_opt("ITICK_TOKEN");

        let binance_symbol = env_str("BINANCE_SYMBOL", "XAUUSDT").to_lowercase();

        Self {
            port: env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(3000),

            binance_ws_url: env::var("BINANCE_WS_URL").unwrap_or_else(|_| {
                format!("wss://fstream.binance.com/ws/{binance_symbol}@aggTrade")
            }),
            sifting_ws_url: env_str("SIFTING_WS_URL", "wss://stream.sifting.io/ws/v1"),
            sifting_hist_url: env_str("SIFTING_HIST_URL", "https://api.sifting.io"),
            sifting_api_key,
            sifting_symbol: env_str("SIFTING_SYMBOL", "XAUUSD"),

            feed_binance: env_switch("FEED_BINANCE", true),
            feed_bybit: env_switch("FEED_BYBIT", true),
            feed_okx: env_switch("FEED_OKX", true),
            feed_bitget: env_switch("FEED_BITGET", true),
            feed_gate: env_switch("FEED_GATE", true),
            feed_kraken: env_switch("FEED_KRAKEN", true),
            feed_alltick: env_switch("FEED_ALLTICK", alltick_token.is_some()),
            feed_itick: env_switch("FEED_ITICK", itick_token.is_some()),
            feed_sifting: env_switch("FEED_SIFTING", has_sifting_key),

            bitget_symbol: env_str("BITGET_SYMBOL", "XAUTUSDT"),
            gate_symbol: env_str("GATE_SYMBOL", "XAUT_USDT"),
            kraken_product: env_str("KRAKEN_PRODUCT", "PF_XAUTUSD"),
            alltick_ws_url: env_str("ALLTICK_WS_URL", "wss://quote.alltick.co/quote-b-ws-api"),
            alltick_code: env_str("ALLTICK_CODE", "XAUUSD"),
            itick_ws_url: env_str("ITICK_WS_URL", "wss://api-free.itick.org/forex"),
            itick_symbol: env_str("ITICK_SYMBOL", "XAUUSD"),

            level_proximity_pips: env_f64("LEVEL_PROXIMITY_PIPS", 5.0),

            mcp_browser_url: env_str("MCP_BROWSER_URL", "http://localhost:3001"),
            mcp_browser_token: env_opt("MCP_BROWSER_TOKEN"),
            mcp_scrape_secs: env_u64("MCP_SCRAPE_SECS", 900),

            calendar_url: env_opt("CALENDAR_URL"),
            calendar_use_mcp: env_bool("CALENDAR_USE_MCP", true),
            seed_synthetic_candles: env_bool("SEED_SYNTHETIC_CANDLES", false),

            mcp_chelsea_url: env_opt("MCP_CHELSEA_URL"),
            deriv_api_url: env_str("DERIV_API_URL", "wss://ws.derivws.com/websockets/v3"),
            deriv_app_id: env_opt("DERIV_APP_ID"),
            deriv_demo_api: env_opt("DERIV_DEMO_API"),
            sl_min_pips: env_f64("SL_MIN_PIPS", 10.0),
            sl_max_pips: env_f64("SL_MAX_PIPS", 50.0),
            tp_min_pips: env_f64("TP_MIN_PIPS", 15.0),
            tp_max_pips: env_f64("TP_MAX_PIPS", 100.0),
            rr_min: env_f64("RR_MIN", 1.0),
            rr_max: env_f64("RR_MAX", 3.0),
        }
    }

    pub fn execution_venue(&self) -> ExecutionVenue {
        if self.mcp_chelsea_url.is_some() {
            ExecutionVenue::ChelseaLive
        } else if self.deriv_demo_api.is_some() {
            ExecutionVenue::DerivDemo
        } else {
            ExecutionVenue::None
        }
    }
}
