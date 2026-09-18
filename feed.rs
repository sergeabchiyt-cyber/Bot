use chrono::{DateTime, Utc};
use crate::config::FeedCfg;
use crate::types::Candle;

#[async_trait::free] // not used — plain async trait object via enum dispatch instead
pub enum Feed { Oanda(Oanda), Csv(Csv) }

impl Feed {
    pub async fn candles(&self, tf: &str, count: u32) -> Result<Vec<Candle>, Box<dyn std::error::Error + Send + Sync>> {
        match self { Oanda(f) => f.candles(tf, count).await, Csv(f) => f.candles(tf, count).await }
    }
}
