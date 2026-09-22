#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub binance_ws_url: String,
    pub binance_kline_url: String,
    pub level_proximity_pips: f64,
    pub mcp_browser_url: String,
    pub mcp_chelsea_url: Option<String>,
    pub sifting_api_key: String,
    pub deriv_api_url: String,
    pub deriv_app_id: String,
    pub deriv_api_key: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000),
            binance_ws_url: std::env::var("BINANCE_WS_URL")
                .unwrap_or_else(|_| "wss://fstream.binance.com/ws/xauusdt@aggTrade".to_string()),
            binance_kline_url: std::env::var("BINANCE_KLINE_URL")
                .unwrap_or_else(|_| "wss://fstream.binance.com/ws/xauusdt@kline_15m".to_string()),
            level_proximity_pips: std::env::var("LEVEL_PROXIMITY_PIPS")
                .unwrap_or_else(|_| "5.0".to_string())
                .parse()
                .unwrap_or(5.0),
            mcp_browser_url: std::env::var("MCP_BROWSER_URL")
                .unwrap_or_else(|_| "http://localhost:3001".to_string()),
            mcp_chelsea_url: std::env::var("MCP_CHELSEA_URL").ok(),
            sifting_api_key: std::env::var("SIFTING_API_KEY").unwrap_or_default(),
            deriv_api_url: std::env::var("DERIV_API_URL")
                .unwrap_or_else(|_| "wss://ws.derivws.com/websockets/v3".to_string()),
            deriv_app_id: std::env::var("DERIV_APP_ID").unwrap_or_default(),
            deriv_api_key: std::env::var("DERIV_API_KEY").unwrap_or_default(),
        }
    }
}