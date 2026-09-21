use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub binance_ws_url: String,
    pub binance_rest_url: String,
    pub mcp_browser_url: String,
    pub mcp_chelsea_url: String,
    pub port: u16,
    pub volume_spike_percentile: f64,
    pub level_proximity_pips: f64,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub value_area_pct: f64,
    pub vp_bins: usize,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();
        Self {
            binance_ws_url: env::var("BINANCE_WS_URL")
                .unwrap_or_else(|_| "wss://fstream.binance.com/market/ws/xauusdt@aggTrade".into()),
            binance_rest_url: env::var("BINANCE_REST_URL")
                .unwrap_or_else(|_| "https://fapi.binance.com".into()),
            mcp_browser_url: env::var("MCP_BROWSER_URL")
                .unwrap_or_else(|_| "https://backend-browser-agent.onrender.com/mcp".into()),
            mcp_chelsea_url: env::var("MCP_CHELSEA_URL")
                .unwrap_or_default(),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()
                .unwrap_or(8080),
            volume_spike_percentile: env::var("VOLUME_SPIKE_PCT")
                .unwrap_or_else(|_| "95.0".into())
                .parse()
                .unwrap_or(95.0),
            level_proximity_pips: env::var("LEVEL_PROXIMITY_PIPS")
                .unwrap_or_else(|_| "30.0".into())
                .parse()
                .unwrap_or(30.0),
            sl_min_pips: 200.0,
            sl_max_pips: 300.0,
            tp_min_pips: 600.0,
            tp_max_pips: 800.0,
            rr_min: 2.0,
            rr_max: 3.0,
            value_area_pct: 0.70,
            vp_bins: 100,
        }
    }
}
