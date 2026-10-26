//! Process configuration.
//!
//! Node 1 reads market-data, calendar and browser-MCP settings only. Every
//! execution-side variable (sizing, stops, targets, account credentials) lives
//! on Node 4 and is deliberately absent here.

use std::env;

/// The Node 2 static dashboard, deployed separately from the engine. Override
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

    // ---- Market data ----
    /// SiftingIO is the single price domain: candles, tick volume and feed
    /// freshness all come from its spot XAUUSD stream.
    pub sifting_ws_url: String,
    pub sifting_hist_url: String,
    pub sifting_api_key: String,
    pub sifting_symbol: String,
    pub feed_sifting: bool,

    // ---- Browser API ----
    /// The only origin allowed to call the REST API from a browser (the Node 2
    /// static site). Compared against the request's `Origin` header
    /// byte-for-byte, so it carries no trailing slash.
    pub cors_allowed_origin: String,

    // ---- Browser MCP (calendar fallback) ----
    pub mcp_browser_url: String,
    pub mcp_browser_token: Option<String>,
    pub mcp_scrape_secs: u64,

    // ---- Economic calendar ----
    /// Override the direct calendar feed URL (defaults to the ForexFactory
    /// weekly export). Set empty to rely on the defaults.
    pub calendar_url: Option<String>,
    /// Use the browser MCP server as a fallback when direct/API calendar feeds
    /// fail. Auto-enabled only when `MCP_BROWSER_URL` is configured (or when
    /// `CALENDAR_USE_MCP` explicitly turns it on).
    pub calendar_use_mcp: bool,

    /// CI/offline only: seed a deterministic synthetic series when no upstream
    /// history is reachable. Never enabled by default, and every synthetic bar
    /// is labelled `source: "synthetic"`.
    pub seed_synthetic_candles: bool,
}

fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn tri_state_switch(raw: Option<&str>, has_credentials: bool) -> bool {
    match raw.unwrap_or_default().trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => true,
        "0" | "false" | "off" | "no" => false,
        _ => has_credentials, // auto
    }
}

/// tri-state feed switch: FEED_X=on|off|auto (auto = on only when its
/// credentials exist, or always-on for keyless public feeds).
fn env_switch(key: &str, has_credentials: bool) -> bool {
    let value = env::var(key).ok();
    tri_state_switch(value.as_deref(), has_credentials)
}

impl Config {
    pub fn from_env() -> Self {
        let sifting_api_key = env_opt("SIFTING_API_KEY").unwrap_or_default();
        let has_sifting_key = !sifting_api_key.is_empty();
        let mcp_url = env_opt("MCP_BROWSER_URL");
        let has_mcp_url = mcp_url.is_some();

        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3000),

            sifting_ws_url: env_str("SIFTING_WS_URL", "wss://stream.sifting.io/ws/v1"),
            sifting_hist_url: env_str("SIFTING_HIST_URL", "https://api.sifting.io"),
            sifting_api_key,
            sifting_symbol: env_str("SIFTING_SYMBOL", "XAUUSD"),
            feed_sifting: env_switch("FEED_SIFTING", has_sifting_key),

            cors_allowed_origin: normalize_origin(&env_str(
                "CORS_ALLOWED_ORIGIN",
                DEFAULT_CORS_ALLOWED_ORIGIN,
            )),

            mcp_browser_url: mcp_url.unwrap_or_else(|| "http://localhost:3001".to_string()),
            mcp_browser_token: env_opt("MCP_BROWSER_TOKEN"),
            mcp_scrape_secs: env_u64("MCP_SCRAPE_SECS", 900).max(60),

            calendar_url: env_opt("CALENDAR_URL"),
            calendar_use_mcp: env_switch("CALENDAR_USE_MCP", has_mcp_url),

            seed_synthetic_candles: env_bool("SEED_SYNTHETIC_CANDLES", false),
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
        assert_eq!(
            Config::from_env().cors_allowed_origin,
            DEFAULT_CORS_ALLOWED_ORIGIN
        );
    }

    #[test]
    fn the_calendar_poll_cannot_be_configured_below_a_minute() {
        // A zero interval would busy-loop the ForexFactory feed into a ban.
        assert!(Config::from_env().mcp_scrape_secs >= 60);
    }

    #[test]
    fn mcp_fallback_is_auto_enabled_only_when_mcp_url_is_configured() {
        assert!(!tri_state_switch(None, false));
        assert!(tri_state_switch(None, true));
        assert!(tri_state_switch(Some("auto"), true));
        assert!(!tri_state_switch(Some("auto"), false));
        assert!(tri_state_switch(Some("1"), false));
        assert!(!tri_state_switch(Some("0"), true));

        if std::env::var_os("MCP_BROWSER_URL").is_none()
            && std::env::var_os("CALENDAR_USE_MCP").is_none()
        {
            assert!(
                !Config::from_env().calendar_use_mcp,
                "bare deployments without MCP_BROWSER_URL must not probe localhost:3001"
            );
        }
    }
}
