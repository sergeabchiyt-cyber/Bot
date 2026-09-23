use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::{error, info, warn};

use crate::status::FeedStatus;
use crate::types::{AggTrade, KlineEvent};

/// Binance futures aggTrade stream — the primary order flow source.
/// Auto-reconnects every 5s after a drop.
pub async fn run_agg_trade_stream(url: String, tx: mpsc::Sender<AggTrade>, status: FeedStatus) -> Result<()> {
    loop {
        status.set("binance", "connecting");
        info!("Connecting to Binance aggTrade stream");
        match connect_async(&url).await {
            Ok((ws, _)) => {
                info!("aggTrade stream connected");
                status.set("binance", "connected");
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(m) if m.is_text() => {
                            if let Ok(mut trade) =
                                serde_json::from_str::<AggTrade>(m.to_text().unwrap_or(""))
                            {
                                trade.exchange = "binance".into();
                                status.mark_msg("binance");
                                if tx.send(trade).await.is_err() {
                                    warn!("aggTrade consumer dropped; exiting stream");
                                    status.set("binance", "disconnected");
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
                status.set("binance", "disconnected");
            }
            Err(e) => {
                warn!("aggTrade connect failed: {}. Retrying in 5s", e);
                status.set("binance", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

/// Binance futures kline stream — the primary chart + volume profile feed.
/// Auto-reconnects every 5s after a drop.
pub async fn run_kline_stream(url: String, tx: mpsc::Sender<KlineEvent>, status: FeedStatus) -> Result<()> {
    loop {
        status.set("binance_kline", "connecting");
        info!("Connecting to Binance kline stream");
        match connect_async(&url).await {
            Ok((ws, _)) => {
                info!("kline stream connected");
                status.set("binance_kline", "connected");
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(m) if m.is_text() => {
                            if let Ok(ev) =
                                serde_json::from_str::<KlineEvent>(m.to_text().unwrap_or(""))
                            {
                                status.mark_msg("binance_kline");
                                if tx.send(ev).await.is_err() {
                                    warn!("kline consumer dropped; exiting stream");
                                    status.set("binance_kline", "disconnected");
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
                status.set("binance_kline", "disconnected");
            }
            Err(e) => {
                warn!("kline connect failed: {}. Retrying in 5s", e);
                status.set("binance_kline", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
