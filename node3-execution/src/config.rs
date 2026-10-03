use std::env;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionVenue {
    DerivDemo,
    ChelseaLive,
    None,
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

fn env_u16(key: &str, default: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

impl Config {
    pub fn from_env() -> Self {
        Self {
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

            volume_threshold: env_f64("VOLUME_THRESHOLD", 10_500.0),
            sl_min_pips: env_f64("SL_MIN_PIPS", 200.0),
            sl_max_pips: env_f64("SL_MAX_PIPS", 300.0),
            tp_min_pips: env_f64("TP_MIN_PIPS", 600.0),
            tp_max_pips: env_f64("TP_MAX_PIPS", 800.0),
            rr_min: env_f64("RR_MIN", 2.0),
            rr_max: env_f64("RR_MAX", 3.0),
            order_size: env_f64("ORDER_SIZE", 0.01),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            node1_ws_url: "wss://example.invalid/ws".into(),
            port: 10_000,
            mcp_chelsea_url: None,
            deriv_demo_api: Some("test-token".into()),
            deriv_app_id: None,
            deriv_api_url: "https://api.derivws.com".into(),
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
        let config = test_config();
        // Current REST base is returned untouched.
        assert_eq!(config.deriv_ws_url(), "https://api.derivws.com");

        // Legacy wss base is also returned untouched when no app id is set.
        let mut legacy = test_config();
        legacy.deriv_api_url = "wss://ws.derivws.com/websockets/v3".into();
        assert_eq!(
            legacy.deriv_ws_url(),
            "wss://ws.derivws.com/websockets/v3"
        );
    }

    #[test]
    fn deriv_ws_url_appends_configured_app_id() {
        let mut config = test_config();
        config.deriv_api_url = "wss://ws.derivws.com/websockets/v3".into();
        config.deriv_app_id = Some("12345".into());
        assert_eq!(
            config.deriv_ws_url(),
            "wss://ws.derivws.com/websockets/v3?app_id=12345"
        );

        // An app id already embedded in the URL wins (no duplication).
        let mut embedded = test_config();
        embedded.deriv_api_url =
            "wss://ws.derivws.com/websockets/v3?app_id=999".into();
        embedded.deriv_app_id = Some("12345".into());
        assert_eq!(
            embedded.deriv_ws_url(),
            "wss://ws.derivws.com/websockets/v3?app_id=999"
        );
    }
}
