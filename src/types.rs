use serde::{Deserialize, Serialize};

// =====================================================================
// Binance market data
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggTrade {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "a")]
    pub agg_id: u64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "f")]
    pub first_trade_id: u64,
    #[serde(rename = "l")]
    pub last_trade_id: u64,
    #[serde(rename = "T")]
    pub trade_time: i64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
    #[serde(rename = "st")]
    pub symbol_type: Option<i32>,

    #[serde(default = "default_exchange")]
    pub exchange: String,
}

fn default_exchange() -> String {
    "binance".into()
}

impl AggTrade {
    pub fn price_f64(&self) -> f64 {
        self.price.parse().unwrap_or(0.0)
    }

    pub fn qty_f64(&self) -> f64 {
        self.quantity.parse().unwrap_or(0.0)
    }

    pub fn signed_delta(&self) -> f64 {
        if self.is_buyer_maker {
            -self.qty_f64()
        } else {
            self.qty_f64()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "k")]
    pub kline: Kline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    #[serde(rename = "t")]
    pub start_time: i64,
    #[serde(rename = "T")]
    pub close_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "i")]
    pub interval: String,
    #[serde(rename = "o")]
    pub open: String,
    #[serde(rename = "c")]
    pub close: String,
    #[serde(rename = "h")]
    pub high: String,
    #[serde(rename = "l")]
    pub low: String,
    #[serde(rename = "v")]
    pub volume: String,
    #[serde(rename = "x")]
    pub is_closed: bool,
}

impl Kline {
    pub fn to_vp_candle(&self) -> VpCandle {
        VpCandle {
            time: self.start_time,
            open: self.open.parse().unwrap_or(0.0),
            high: self.high.parse().unwrap_or(0.0),
            low: self.low.parse().unwrap_or(0.0),
            close: self.close.parse().unwrap_or(0.0),
            volume: self.volume.parse().unwrap_or(0.0),
        }
    }
}

// =====================================================================
// Volume profile
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpCandle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpLevels {
    pub window: String,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    pub timestamp: i64,
}

// =====================================================================
// Order flow events
// =====================================================================

/// kind = "BUY_BUBBLE" | "SELL_BUBBLE" | "ABS_BUY" | "ABS_SELL"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderflowEvent {
    pub kind: String,
    pub level: f64,
    pub strength: f64,
    pub timestamp: i64,
    #[serde(default = "default_exchange")]
    pub exchange: String,
}

// =====================================================================
// AI / sentiment
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentFrame {
    pub hawkish: f64,
    pub dovish: f64,
    pub neutral: f64,
    pub confidence: f64,
    pub ts: i64,
}

// =====================================================================
// Trades
// =====================================================================

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

// =====================================================================
// WebSocket frames
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    #[serde(rename = "levels")]
    Levels { data: VpLevels },

    #[serde(rename = "candle")]
    Candle { data: VpCandle },

    #[serde(rename = "bubbles")]
    Bubbles { data: OrderflowEvent },

    #[serde(rename = "trades")]
    Trades { data: TradeEvent },

    #[serde(rename = "sentiment")]
    Sentiment { data: SentimentFrame },

    #[serde(rename = "transcript")]
    Transcript { data: serde_json::Value },

    #[serde(rename = "calendar")]
    Calendar { data: serde_json::Value },

    #[serde(rename = "subscribe")]
    Subscribe { topics: Vec<String> },

    #[serde(rename = "learn")]
    Learn { features: Vec<f64>, target: f64 },

    #[serde(rename = "audio_chunk")]
    AudioChunk { data: String },

    #[serde(rename = "heartbeat")]
    Heartbeat,
}