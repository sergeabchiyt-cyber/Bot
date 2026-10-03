use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpLevels {
    pub window: String,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    #[serde(default)]
    pub start: i64,
    #[serde(default)]
    pub end: i64,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpCandle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    pub trade_id: String,
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub entry: f64,
    pub sl: f64,
    pub tp: f64,
    pub status: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    #[serde(rename = "levels")]
    Levels { data: VpLevels },

    #[serde(rename = "candle")]
    Candle { data: VpCandle },

    #[serde(rename = "trades")]
    Trades { data: TradeEvent },

    #[serde(rename = "subscribe")]
    Subscribe { topics: Vec<String> },

    #[serde(rename = "heartbeat")]
    Heartbeat,

    #[serde(other)]
    Unknown,
}
