#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionVenue {
    DerivDemo,
    ChelseaLive,
    None,
}

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub binance_ws_url: String,
    pub binance_kline_url: String,
    pub level_proximity_pips: f64,
    pub mcp_browser_url: String,
    pub mcp_chelsea_url: Option<String>,
    pub sifting_api_key: String,
    pub sifting_hist_url: String,
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

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            binance_ws_url: std::env::var("BINANCE_WS_URL").unwrap_or_else(|_| {
                "wss://fstream.binance.com/ws/xauusdt@aggTrade".to_string()
            }),
            binance_kline_url: std::env::var("BINANCE_KLINE_URL").unwrap_or_else(|_| {
                "wss://fstream.binance.com/ws/xauusdt@kline_15m".to_string()
            }),
            level_proximity_pips: std::env::var("LEVEL_PROXIMITY_PIPS")
                .unwrap_or_else(|_| "5.0".to_string())
                .parse()
                .unwrap_or(5.0),
            mcp_browser_url: std::env::var("MCP_BROWSER_URL")
                .unwrap_or_else(|_| "http://localhost:3001".to_string()),
            mcp_chelsea_url: std::env::var("MCP_CHELSEA_URL").ok(),
            sifting_api_key: std::env::var("SIFTING_API_KEY").unwrap_or_default(),
            sifting_hist_url: std::env::var("SIFTING_HIST_URL")
                .unwrap_or_else(|_| "https://api.sifting.io".to_string()),
            deriv_api_url: std::env::var("DERIV_API_URL")
                .unwrap_or_else(|_| "wss://ws.derivws.com/websockets/v3".to_string()),
            deriv_app_id: std::env::var("DERIV_APP_ID").ok(),
            deriv_demo_api: std::env::var("DERIV_DEMO_API").ok(),
            sl_min_pips: std::env::var("SL_MIN_PIPS")
                .unwrap_or_else(|_| "10.0".to_string())
                .parse()
                .unwrap_or(10.0),
            sl_max_pips: std::env::var("SL_MAX_PIPS")
                .unwrap_or_else(|_| "50.0".to_string())
                .parse()
                .unwrap_or(50.0),
            tp_min_pips: std::env::var("TP_MIN_PIPS")
                .unwrap_or_else(|_| "15.0".to_string())
                .parse()
                .unwrap_or(15.0),
            tp_max_pips: std::env::var("TP_MAX_PIPS")
                .unwrap_or_else(|_| "100.0".to_string())
                .parse()
                .unwrap_or(100.0),
            rr_min: std::env::var("RR_MIN")
                .unwrap_or_else(|_| "1.0".to_string())
                .parse()
                .unwrap_or(1.0),
            rr_max: std::env::var("RR_MAX")
                .unwrap_or_else(|_| "3.0".to_string())
                .parse()
                .unwrap_or(3.0),
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