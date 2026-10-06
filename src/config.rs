use std::env;

/// The Node2 static dashboard, deployed separately from the engine. Override
/// with `CORS_ALLOWED_ORIGIN` when the site moves (or to disable browser CORS
/// entirely by setting it to an origin that can never match, e.g. empty).
const DEFAULT_CORS_ALLOWED_ORIGIN: &str = "https://static-dash-frontend.onrender.com";

/// An `Origin` header never carries whitespace or a trailing slash, so a
/// configured `https://host/` could never match a browser request. Normalising
/// here keeps a stray slash in the Render dashboard from silently disabling CORS.
fn normalize_origin(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
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

    // ---- Browser API ----
    /// The only origin allowed to call the REST API from a browser (the Node2
    /// static site). Compared against the request's `Origin` header
    /// byte-for-byte, so it carries no trailing slash.
    pub cors_allowed_origin: String,

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

    // ---- Econ news audio monitor (engine -> Node3 as `audio_chunk`) ----
    /// Master switch for the automated econ-news audio monitor. When an
    /// upcoming/active calendar event window opens, the monitor discovers
    /// live coverage (MCP browser + configured watchlist), captures the
    /// audio and streams it to Node3 over `/ws` topic `audio_chunk`.
    pub econ_monitor: bool,
    /// How often (seconds) the calendar is scanned for event windows.
    pub econ_scan_secs: u64,
    /// Start capturing this many seconds BEFORE an event's scheduled time.
    pub econ_before_secs: i64,
    /// Keep capturing this many seconds AFTER an event's scheduled time.
    pub econ_after_secs: i64,
    /// Minimum impact to arm on: "High" | "Medium" | "Low".
    pub econ_min_impact: String,
    /// Currencies to arm on (comma list, e.g. "USD,ALL").
    pub econ_currencies: Vec<String>,
    /// Always-on stream watchlist (comma list of page or direct media URLs,
    /// e.g. `https://www.youtube.com/@federalreserve/live`).
    pub econ_stream_sources: Vec<String>,
    /// PCM chunk duration streamed per `audio_chunk` frame (ms).
    pub econ_chunk_ms: u64,
    /// Use the browser MCP server to browse active events and discover live
    /// coverage links (the same 31-tool browser server as the calendar).
    pub econ_use_mcp: bool,
    /// CI/offline plumbing test: stream a synthetic 16kHz tone as
    /// `audio_chunk` frames without any event, stream, ffmpeg or yt-dlp.
    pub econ_test_tone: bool,
    /// Safety cap per captured stream (seconds).
    pub econ_max_stream_secs: u64,
    /// Binary used to resolve live stream URLs (yt-dlp).
    pub econ_ytdlp: String,
    /// Binary used to decode streams to 16kHz mono f32le PCM (ffmpeg).
    pub econ_ffmpeg: String,

    /// CI/offline only: if no historical candles could be fetched from any
    /// upstream, seed a deterministic synthetic series so the volume-profile
    /// endpoints are exercisable. Never enabled by default.
    pub seed_synthetic_candles: bool,

    // ---- Volume-profile histogram model (TradingView parity) ----
    /// `rows` (TradingView "Number Of Rows" layout, default) or `price`
    /// (constant-height rows).
    pub vp_row_mode: String,
    /// Row count for the `rows` mode — TradingView's "Row Size" input, 128 in
    /// the reference charts.
    pub vp_rows: usize,
    /// Row height in price for the `price` mode (the legacy $0.50 grid).
    pub vp_bin_size: f64,
    /// Symbol tick: row heights are rounded to whole ticks like TradingView.
    pub vp_tick_size: f64,
    /// TradingView "Value Area Volume" percentage.
    pub vp_va_pct: f64,
    /// Profile input resolution: `tv` (default) replays TradingView's
    /// 5,000-bar lower-timeframe ladder per window, or pin one (`1m`, `5m`).
    pub vp_lower_tf: String,
    /// Second-chance interval if the 1m profile history cannot cover PW.
    pub vp_fallback_interval: String,
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

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Comma-separated list env, trimmed and stripped of empties.
fn env_list(key: &str, default: &str) -> Vec<String> {
    env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
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

            cors_allowed_origin: normalize_origin(&env_str(
                "CORS_ALLOWED_ORIGIN",
                DEFAULT_CORS_ALLOWED_ORIGIN,
            )),

            mcp_browser_url: env_str("MCP_BROWSER_URL", "http://localhost:3001"),
            mcp_browser_token: env_opt("MCP_BROWSER_TOKEN"),
            mcp_scrape_secs: env_u64("MCP_SCRAPE_SECS", 900),

            calendar_url: env_opt("CALENDAR_URL"),
            calendar_use_mcp: env_bool("CALENDAR_USE_MCP", true),

            econ_monitor: env_bool("ECON_MONITOR", true),
            econ_scan_secs: env_u64("ECON_SCAN_SECS", 60),
            econ_before_secs: env_i64("ECON_WINDOW_BEFORE_SECS", 900),
            econ_after_secs: env_i64("ECON_WINDOW_AFTER_SECS", 3600),
            econ_min_impact: env_str("ECON_MIN_IMPACT", "High"),
            econ_currencies: env_list("ECON_CURRENCIES", "USD"),
            econ_stream_sources: env_list("ECON_STREAM_SOURCES", ""),
            econ_chunk_ms: env_u64("ECON_CHUNK_MS", 1000).clamp(100, 10_000),
            econ_use_mcp: env_bool("ECON_USE_MCP", true),
            econ_test_tone: env_bool("ECON_TEST_TONE", false),
            econ_max_stream_secs: env_u64("ECON_MAX_STREAM_SECS", 7200),
            econ_ytdlp: env_str("ECON_YTDLP", "yt-dlp"),
            econ_ffmpeg: env_str("ECON_FFMPEG", "ffmpeg"),

            seed_synthetic_candles: env_bool("SEED_SYNTHETIC_CANDLES", false),

            vp_row_mode: env_str("VP_ROW_MODE", "rows").to_ascii_lowercase(),
            vp_rows: env_u64("VP_ROWS", 128).clamp(2, 5_000) as usize,
            vp_bin_size: env_f64("VP_BIN_SIZE", 0.50),
            vp_tick_size: env_f64("VP_TICK_SIZE", 0.01),
            vp_va_pct: env_f64("VP_VA_PCT", 70.0),
            vp_lower_tf: env_str("VP_LOWER_TF", "tv").to_ascii_lowercase(),
            vp_fallback_interval: env_str("VP_FALLBACK_INTERVAL", "5m").to_ascii_lowercase(),
        }
    }

    /// Numeric impact rank for the econ monitor: High=2, Medium=1, Low/other=0.
    pub fn econ_min_impact_rank(&self) -> u8 {
        match self.econ_min_impact.trim().to_ascii_lowercase().as_str() {
            "high" | "red" => 2,
            "medium" | "orange" => 1,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origin_is_normalized_for_header_comparison() {
        // A browser `Origin` header has no trailing slash and no padding, so
        // anything configured has to be reduced to that same form.
        assert_eq!(
            normalize_origin("https://static-dash-frontend.onrender.com"),
            "https://static-dash-frontend.onrender.com"
        );
        assert_eq!(
            normalize_origin("  https://static-dash-frontend.onrender.com/  "),
            "https://static-dash-frontend.onrender.com"
        );
        assert_eq!(normalize_origin(""), "");
    }

    #[test]
    fn default_origin_is_already_in_header_form() {
        // The comparison with the browser's `Origin` is byte-for-byte, so the
        // built-in default must not need normalising at all.
        assert_eq!(
            normalize_origin(DEFAULT_CORS_ALLOWED_ORIGIN),
            DEFAULT_CORS_ALLOWED_ORIGIN
        );
    }

    #[test]
    fn from_env_falls_back_to_the_node2_site() {
        if std::env::var_os("CORS_ALLOWED_ORIGIN").is_some() {
            print!("CORS_ALLOWED_ORIGIN is set in the environment; skipping");
            return;
        }
        assert_eq!(Config::from_env().cors_allowed_origin, DEFAULT_CORS_ALLOWED_ORIGIN);
    }
}
