use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::status::FeedStatus;
use crate::types::{VpCandle, WsFrame};

const PING_INTERVAL_SECS: u64 = 30; // Sifting closes silent connections after 90s
const RECONNECT_SECS: u64 = 5;
const BUCKET_MS: i64 = 15 * 60 * 1000; // 15-minute chart candles

struct Aggregator {
    start_time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

impl Aggregator {
    fn new(start_time: i64, price: f64, volume: f64) -> Self {
        Self {
            start_time,
            open: price,
            high: price,
            low: price,
            close: price,
            volume,
        }
    }

    fn update(&mut self, price: f64, volume: f64) {
        if price > self.high {
            self.high = price;
        }
        if price < self.low {
            self.low = price;
        }
        self.close = price;
        self.volume += volume;
    }

    fn candle(&self, source: &str) -> VpCandle {
        VpCandle {
            time: self.start_time,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            // Sifting ticks expose the last-trade size in `P`. When a quote
            // has no trade size, count one update rather than emitting a zero
            // volume candle that can never affect a live profile.
            volume: self.volume.max(1.0),
            source: source.into(),
        }
    }
}

/// SiftingIO spot stream for the chart and the live edge of the VP history.
/// Per their docs:
/// - connect with `?key=`, first frame is `{f:"ack", op:"auth", ...}`
/// - subscribe `{op:"subscribe", product:"com", symbols:["XAUUSD"]}`
/// - ticks are `{f:"tick", class:"com", s, p, P, b, B, a, A, t(ms)}`
/// - the server closes connections idle for 90s → app-level ping every 30s
///
/// The broadcast channel carries the current in-progress candle to clients.
/// A completed candle is also sent to `candle_tx`, allowing the backend to
/// update its cached chart and VP without using Binance price candles.
pub async fn run_sifting_stream(
    base_url: String,
    api_key: String,
    symbol: String,
    bc_tx: broadcast::Sender<WsFrame>,
    candle_tx: mpsc::Sender<VpCandle>,
    status: FeedStatus,
) -> anyhow::Result<()> {
    let url = if base_url.contains('?') {
        format!("{base_url}&key={api_key}")
    } else {
        format!("{base_url}?key={api_key}")
    };

    loop {
        status.set("sifting", "connecting");
        info!("Connecting to Sifting WS");
        match connect_async(&url).await {
            Ok((ws_stream, _)) => {
                info!("Sifting WS connected");
                status.set("sifting", "connected");
                let (mut write, mut read) = ws_stream.split();

                let subscribe_msg = serde_json::json!({
                    "op": "subscribe",
                    "product": "com",
                    "symbols": [symbol]
                });
                if write
                    .send(Message::Text(subscribe_msg.to_string().into()))
                    .await
                    .is_err()
                {
                    warn!("Sifting subscribe send failed");
                    status.set("sifting", "error");
                } else {
                    // Keepalive task owns the write half.
                    let ping_task = tokio::spawn(async move {
                        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(
                            PING_INTERVAL_SECS,
                        ));
                        iv.tick().await; // first tick fires immediately; skip
                        loop {
                            iv.tick().await;
                            let ping = serde_json::json!({ "op": "ping" });
                            if write
                                .send(Message::Text(ping.to_string().into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    });

                    let mut agg: Option<Aggregator> = None;

                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                status.mark_msg("sifting");
                                handle_text(
                                    &text,
                                    &symbol,
                                    &mut agg,
                                    &bc_tx,
                                    &candle_tx,
                                );
                            }
                            Ok(Message::Close(frame)) => {
                                info!("Sifting WS closed by server: {:?}", frame);
                                break;
                            }
                            Ok(_) => {}
                            Err(e) => {
                                warn!("Sifting WS error: {}", e);
                                break;
                            }
                        }
                    }

                    ping_task.abort();
                    status.set("sifting", "disconnected");
                }
            }
            Err(e) => {
                error!("Sifting connect failed: {}", e);
                status.set("sifting", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

fn handle_text(
    text: &str,
    symbol: &str,
    agg: &mut Option<Aggregator>,
    bc_tx: &broadcast::Sender<WsFrame>,
    candle_tx: &mpsc::Sender<VpCandle>,
) {
    let val: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    match val.get("f").and_then(|f| f.as_str()) {
        Some("ack") | Some("pong") => return, // handshake/keepalive frames
        Some("error") => {
            warn!(
                "Sifting server error frame: code={} message={}",
                val.get("code").and_then(|c| c.as_str()).unwrap_or("?"),
                val.get("message").and_then(|m| m.as_str()).unwrap_or("?")
            );
            return;
        }
        Some("tick") => {}
        _ => return,
    }

    if val.get("s").and_then(|s| s.as_str()) != Some(symbol) {
        return;
    }

    // Prefer the last-trade price `p`, fall back to bid/ask mid.
    let price = val
        .get("p")
        .and_then(|v| v.as_f64())
        .or_else(|| {
            match (
                val.get("b").and_then(|v| v.as_f64()),
                val.get("a").and_then(|v| v.as_f64()),
            ) {
                (Some(b), Some(a)) if b > 0.0 && a > 0.0 => Some((b + a) / 2.0),
                _ => None,
            }
        });
    let Some(price) = price.filter(|p| p.is_finite() && *p > 0.0) else {
        return;
    };
    let ts = val
        .get("t")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let trade_size = val
        .get("P")
        .and_then(|v| v.as_f64())
        .filter(|size| size.is_finite() && *size > 0.0)
        .unwrap_or(1.0);

    let candle_start = ts - (ts % BUCKET_MS);
    let bucket_changed = !matches!(
        agg.as_ref(),
        Some(current) if current.start_time == candle_start
    );

    if bucket_changed {
        if let Some(previous) = agg.take() {
            let closed = previous.candle("sifting");
            let _ = candle_tx.try_send(closed.clone());
            let _ = bc_tx.send(WsFrame::Candle { data: closed });
        }
        *agg = Some(Aggregator::new(candle_start, price, trade_size));
    } else if let Some(current) = agg.as_mut() {
        current.update(price, trade_size);
    }

    if let Some(current) = agg.as_ref() {
        let _ = bc_tx.send(WsFrame::Candle {
            data: current.candle("sifting"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregator_accumulates_live_trade_size() {
        let mut agg = Aggregator::new(0, 100.0, 2.0);
        agg.update(101.0, 3.0);
        let candle = agg.candle("sifting");
        assert_eq!(candle.high, 101.0);
        assert_eq!(candle.volume, 5.0);
    }
}
