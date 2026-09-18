use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Candle { pub t: i64, pub o: f64, pub h: f64, pub l: f64, pub c: f64, pub v: f64 }

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Side { Long, Short }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ray { pub label: String, pub price: f64 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal {
    pub t: i64, pub side: Side, pub level: String,
    pub entry: f64, pub sl: f64, pub tp: f64, pub vol: f64, pub size: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    pub id: u64, pub side: Side, pub entry: f64, pub sl: f64, pub tp: f64, pub size: f64,
    pub open_t: i64, pub close_t: Option<i64>, pub exit: Option<f64>, pub pnl: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Event {
    #[serde(rename = "candles")]  Candles { tf: String, candles: Vec<Candle> },
    #[serde(rename = "tick")]     Tick { tf: String, candle: Candle },
    #[serde(rename = "levels")]   Levels { rays: Vec<Ray> },
    #[serde(rename = "signal")]   Signal { signal: Signal },
    #[serde(rename = "position")] Position { position: Position },
    #[serde(rename = "equity")]   Equity { equity: f64 },
}
