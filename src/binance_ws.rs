use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tokio_tungstenite::connect_async;
use tracing::{info, warn, error};

use crate::types::{AggTrade, WsFrame};

pub async fn run_agg_trade_stream(
    url: String,
    tx: broadcast::Sender<WsFrame>,
) -> Result<()> {
    loop {
        info!("Connecting to Binance aggTrade WS: {}", url);
        match connect_async(&url).await {
            Ok((ws_stream, _)) => {
                info!("Binance aggTrade WS connected");
                let (_, mut read) = ws_stream.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(msg) if msg.is_text() => {
                            let text = msg.to_text().unwrap_or("");
                            if let Ok(trade) = serde_json::from_str::<AggTrade>(text) {
                                let _ = tx.send(WsFrame::Learn {
                                    features: vec![trade.signed_delta()],
                                    target: 0.0,
                                });
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Binance WS read error: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Binance WS connect failed: {}. Retrying in 5s", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}