use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn, error};

use crate::types::AggTrade;

/// Bybit V5 public linear stream — XAUUSDT perpetual trades.
/// Emits protocol-level pings that tungstenite auto-replies to.
pub async fn run_bybit_stream(tx: mpsc::Sender<AggTrade>) -> Result<()> {
    let url = "wss://stream.bybit.com/v5/public/linear";
    loop {
        info!("Connecting to Bybit trade stream");
        match connect_async(url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();

                let sub = serde_json::json!({
                    "op": "subscribe",
                    "args": ["publicTrade.XAUUSDT"]
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("Bybit subscribe send failed");
                    continue;
                }
                info!("Bybit stream connected");

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            let v: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if v["topic"].as_str() != Some("publicTrade.XAUUSDT") {
                                continue;
                            }
                            if let Some(arr) = v["data"].as_array() {
                                for t in arr {
                                    let side = t["S"].as_str().unwrap_or("Buy");
                                    let price = t["p"].as_str().unwrap_or("0");
                                    let qty = t["v"].as_str().unwrap_or("0");
                                    let ts = t["T"].as_i64().unwrap_or(0);
                                    let agg = AggTrade {
                                        event_type: "aggTrade".into(),
                                        event_time: ts,
                                        symbol: "XAUUSDT".into(),
                                        agg_id: 0,
                                        price: price.into(),
                                        quantity: qty.into(),
                                        first_trade_id: 0,
                                        last_trade_id: 0,
                                        trade_time: ts,
                                        // Bybit: S="Buy" means taker bought → positive delta
                                        is_buyer_maker: side == "Sell",
                                        symbol_type: None,
                                        exchange: "bybit".into(),
                                    };
                                    let _ = tx.send(agg).await;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Bybit read error: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => warn!("Bybit connect failed: {}. Retrying in 5s", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

/// OKX v5 public stream — XAU-USDT-SWAP trades.
/// Sends `{"op":"ping"}` every 25s to keep the connection alive.
pub async fn run_okx_stream(tx: mpsc::Sender<AggTrade>) -> Result<()> {
    let url = "wss://ws.okx.com:8443/ws/v5/public";
    loop {
        info!("Connecting to OKX trade stream");
        match connect_async(url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();

                let sub = serde_json::json!({
                    "op": "subscribe",
                    "args": [{"channel": "trades", "instId": "XAU-USDT-SWAP"}]
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("OKX subscribe send failed");
                    continue;
                }
                info!("OKX stream connected");

                // Ping task owns the write half
                let ping_task = tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(25)).await;
                        let ping = serde_json::json!({"op": "ping"});
                        if write
                            .send(Message::Text(ping.to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            let v: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if v["arg"]["channel"].as_str() != Some("trades") {
                                continue;
                            }
                            if let Some(arr) = v["data"].as_array() {
                                for t in arr {
                                    let side = t["side"].as_str().unwrap_or("buy");
                                    let px = t["px"].as_str().unwrap_or("0");
                                    let sz = t["sz"].as_str().unwrap_or("0");
                                    let ts = t["ts"]
                                        .as_str()
                                        .and_then(|s| s.parse::<i64>().ok())
                                        .unwrap_or(0);
                                    let agg = AggTrade {
                                        event_type: "aggTrade".into(),
                                        event_time: ts,
                                        symbol: "XAUUSDT".into(),
                                        agg_id: 0,
                                        price: px.into(),
                                        quantity: sz.into(),
                                        first_trade_id: 0,
                                        last_trade_id: 0,
                                        trade_time: ts,
                                        // OKX: side="buy" means taker bought → positive delta
                                        is_buyer_maker: side == "sell",
                                        symbol_type: None,
                                        exchange: "okx".into(),
                                    };
                                    let _ = tx.send(agg).await;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("OKX read error: {}", e);
                            break;
                        }
                    }
                }

                ping_task.abort();
            }
            Err(e) => warn!("OKX connect failed: {}. Retrying in 5s", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}