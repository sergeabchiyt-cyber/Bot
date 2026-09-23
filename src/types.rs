use serde::{Deserialize, Serialize};

// =====================================================================
// Normalized trade tick (all order flow sources funnel into this)
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggTrade {
    #[serde(rename = "e", default)]
    pub event_type: String,
    #[serde(rename = "E", default)]
    pub event_time: i64,
    #[serde(rename = "s", default)]
    pub symbol: String,
    #[serde(rename = "a", default)]
    pub agg_id: u64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "f", default)]
    pub first_trade_id: u64,
    #[serde(rename = "l", default)]
    pub last_trade_id: u64,
    #[serde(rename = "T")]
    pub trade_time: i64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
    #[serde(rename = "st", skip_serializing, default)]
    pub symbol_type: Option<i32>,

    #[serde(default = "default_exchange")]
    pub exchange: String,
    /// False for feeds that publish no taker side (quote ticks).
    /// Those count toward volume but never toward delta, so a
    /// direction-less feed cannot fabricate directional flow.
    #[serde(default = "default_true")]
    pub has_flow_side: bool,
}

fn default_exchange() -> String {
    "binance".into()
}

fn default_true() -> bool {
    true
}

impl AggTrade {
    /// Constructor used by the exchange adapters (Bybit/OKX/Bitset/Gate/Kraken/...).
    /// `taker_buy` = the aggressive side was the buyer.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        exchange: &str,
        symbol: &str,
        price: &str,
        qty: &str,
        ts: i64,
        taker_buy: bool,
        has_flow_side: bool,
    ) -> Self {
        Self {
            event_type: "trade".into(),
            event_time: ts,
            symbol: symbol.into(),
            agg_id: 0,
            price: price.into(),
            quantity: qty.into(),
            first_trade_id: 0,
            last_trade_id: 0,
            trade_time: ts,
            is_buyer_maker: !taker_buy,
            symbol_type: None,
            exchange: exchange.into(),
            has_flow_side,
        }
    }

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

// =====================================================================
// Binance market data
// =====================================================================

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
    pub fn to_vp_candle(&self, source: &str) -> VpCandle {
        VpCandle {
            time: self.start_time,
            open: self.open.parse().unwrap_or(0.0),
            high: self.high.parse().unwrap_or(0.0),
            low: self.low.parse().unwrap_or(0.0),
            close: self.close.parse().unwrap_or(0.0),
            volume: self.volume.parse().unwrap_or(0.0),
            source: source.into(),
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
    /// Which feed produced this candle: "binance" | "sifting" | ...
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpLevels {
    pub window: String,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    /// Inclusive start of the window this profile was computed over (ms).
    #[serde(default)]
    pub start: i64,
    /// Exclusive end of the window this profile was computed over (ms).
    /// For PS this is the session close, so clients can see it roll.
    #[serde(default)]
    pub end: i64,
    /// When this profile was last recomputed (ms).
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

    #[serde(rename = "calendar")]
    Calendar { data: serde_json::Value },

    #[serde(rename = "status")]
    Status { data: serde_json::Value },

    #[serde(rename = "subscribe")]
    Subscribe { topics: Vec<String> },

    #[serde(rename = "heartbeat")]
    Heartbeat,
}
