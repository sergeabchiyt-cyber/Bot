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
    /// Port for the local HTTP health endpoint (`GET /health`). 10000 by default.
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
            // health endpoint is reachable out of the box.
            port: env_u16("PORT", 10_000),
            mcp_chelsea_url: env_opt("MCP_CHELSEA_URL"),
            deriv_demo_api: env_opt("DERIV_DEMO_API"),
            deriv_app_id: env_opt("DERIV_APP_ID"),
            deriv_api_url: env_str("DERIV_API_URL", "wss://ws.derivws.com/websockets/v3"),

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
}
