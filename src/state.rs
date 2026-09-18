use std::collections::HashMap;
use std::sync::RwLock;
use tokio::sync::broadcast;
use crate::config::Cfg;
use crate::types::{Candle, Event, Position, Ray, Signal};

pub struct AppState {
    pub cfg: Cfg,
    pub tx: broadcast::Sender<Event>,
    pub candles: RwLock<HashMap<String, Vec<Candle>>>,
    pub levels: RwLock<Vec<Ray>>,
    pub signals: RwLock<Vec<Signal>>,
    pub positions: RwLock<Vec<Position>>,
    pub equity: RwLock<f64>,
}

impl AppState {
    pub fn new(cfg: Cfg) -> (std::sync::Arc<Self>, broadcast::Sender<Event>) {
        let (tx, _) = broadcast::channel(512);
        let st = Self { cfg, tx: tx.clone(), candles: RwLock::new(HashMap::new()),
                        levels: RwLock::new(Vec::new()), signals: RwLock::new(Vec::new()),
                        positions: RwLock::new(Vec::new()), equity: RwLock::new(0.0) };
        (std::sync::Arc::new(st), tx)
    }
    pub fn push(&self, tf: &str, cs: Vec<Candle>) { self.candles.write().unwrap().insert(tf.to_string(), cs); }
    pub fn last(&self, tf: &str) -> Option<Candle> {
        self.candles.read().unwrap().get(tf).and_then(|v| v.last().copied())
    }
}
