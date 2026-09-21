use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::{info, warn, error};

use crate::types::{AggTrade, KlineEvent};

pub async fn run_agg_trade_stream(url: String, tx: mpsc::Sender<AggTrade>) -> Result<()> {
    loop {
        info!("Connecting to Binance aggTrade stream");
        match connect_async(&url).await {
            Ok((ws, _)) => {
                info!("aggTrade stream connected");
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(m) if m.is_text() => {
                            if let Ok(trade) =
                                serde_json::from_str::<AggTrade>(m.to_text().unwrap_or(""))
                            {
                                if tx.send(trade).await.is_err() {
                                    warn!("aggTrade consumer dropped; exiting stream");
                                    return Ok(());
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("aggTrade read error: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => warn!("aggTrade connect failed: {}. Retrying in 5s", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

pub async fn run_kline_stream(url: String, tx: mpsc::Sender<KlineEvent>) -> Result<()> {
    loop {
        info!("Connecting to Binance kline stream");
        match connect_async(&url).await {
            Ok((ws, _)) => {
                info!("kline stream connected");
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(m) if m.is_text() => {
                            if let Ok(ev) =
                                serde_json::from_str::<KlineEvent>(m.to_text().unwrap_or(""))
                            {
                                if tx.send(ev).await.is_err() {
                                    warn!("kline consumer dropped; exiting stream");
                                    return Ok(());
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("kline read error: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => warn!("kline connect failed: {}. Retrying in 5s", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}