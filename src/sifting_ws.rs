use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::broadcast;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use crate::types::{VpCandle, WsFrame};

struct Aggregator {
    start_time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

pub async fn run_sifting_stream(url: String, bc_tx: broadcast::Sender<WsFrame>) -> anyhow::Result<()> {
    info!("Connecting to Sifting WS: {}", url);
    let (ws_stream, _) = connect_async(&url).await?;
    info!("Sifting WS connected");
    let (mut write, mut read) = ws_stream.split();

    let subscribe_msg = r#"{"op":"subscribe","product":"com","symbols":["XAUUSD"]}"#;
    write.send(Message::Text(subscribe_msg.into())).await?;
    info!("Sifting subscribe sent");

    let mut agg: Option<Aggregator> = None;
    let mut logged = 0;

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if logged < 5 {
                    info!("Sifting raw: {}", text);
                    logged += 1;
                }
                
                let val: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let price = extract_price(&val);
                let ts = extract_time(&val).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

                if let Some(p) = price {
                    let candle_start = (ts / (15 * 60 * 1000)) * (15 * 60 * 1000);

                    if let Some(ref mut current) = agg {
                        if current.start_time == candle_start {
                            if p > current.high { current.high = p; }
                            if p < current.low { current.low = p; }
                            current.close = p;
                        } else {
                            *current = Aggregator {
                                start_time: candle_start,
                                open: p, high: p, low: p, close: p,
                            };
                        }
                    } else {
                        agg = Some(Aggregator {
                            start_time: candle_start,
                            open: p, high: p, low: p, close: p,
                        });
                    }

                    if let Some(c) = &agg {
                        let _ = bc_tx.send(WsFrame::Candle {
                            data: VpCandle {
                                time: c.start_time,
                                open: c.open,
                                high: c.high,
                                low: c.low,
                                close: c.close,
                                volume: 0.0,
                            },
                        });
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("Sifting WS closed by server");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                warn!("Sifting WS error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

fn extract_price(val: &Value) -> Option<f64> {
    // Average bid/ask for mid-market spot
    if let (Some(b), Some(a)) = (val.get("b").and_then(|v| v.as_f64()), val.get("a").and_then(|v| v.as_f64())) {
        return Some((b + a) / 2.0);
    }
    if let (Some(b), Some(a)) = (val.get("bid").and_then(|v| v.as_f64()), val.get("ask").and_then(|v| v.as_f64())) {
        return Some((b + a) / 2.0);
    }
    if let Some(p) = val.get("p").and_then(|v| v.as_f64()) { return Some(p); }
    if let Some(p) = val.get("price").and_then(|v| v.as_f64()) { return Some(p); }
    if let Some(p) = val.get("last").and_then(|v| v.as_f64()) { return Some(p); }
    if let Some(p) = val.get("p").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()) { return Some(p); }
    if let Some(p) = val.get("price").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()) { return Some(p); }
    None
}

fn extract_time(val: &Value) -> Option<i64> {
    if let Some(t) = val.get("t").and_then(|v| v.as_i64()) { return Some(t); }
    if let Some(t) = val.get("timestamp").and_then(|v| v.as_i64()) { return Some(t); }
    if let Some(t) = val.get("time").and_then(|v| v.as_i64()) { return Some(t); }
    None
}