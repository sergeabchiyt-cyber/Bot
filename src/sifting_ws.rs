//! SiftingIO spot stream — the only price source.
//!
//! Per their docs:
//! * connect with `?key=`, first frame is `{f:"ack", op:"auth", ...}`
//! * subscribe `{op:"subscribe", product:"com", symbols:["XAUUSD"]}`
//! * ticks are `{f:"tick", class:"com", s, p, P, b, B, a, A, t(ms)}`
//! * the server closes connections idle for 90 s → app-level ping every 30 s
//!
//! The connection is disposable; the [`CandleBook`] is not. It lives outside
//! the reconnect loop, so an in-progress bucket keeps its tick count and its
//! close is never skipped.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::candles::CandleBook;
use crate::publish::MarketPublisher;
use crate::status::FeedStatus;

const PING_INTERVAL_SECS: u64 = 30;

/// Sifting closes silent connections after 90 s.
const RECONNECT_SECS: u64 = 5;

/// Run the market stream until the process exits, reconnecting forever.
pub async fn run_stream(
    base_url: String,
    api_key: String,
    symbol: String,
    book: CandleBook,
    publisher: MarketPublisher,
    status: FeedStatus,
) -> anyhow::Result<()> {
    let url = if base_url.contains('?') {
        format!("{base_url}&key={api_key}")
    } else {
        format!("{base_url}?key={api_key}")
    };

    loop {
        status.set("sifting", "connecting");
        publisher.set_feed_connected(false);
        info!("Connecting to the Sifting market stream");

        match connect_async(&url).await {
            Ok((stream, _)) => {
                status.set("sifting", "connected");
                publisher.set_feed_connected(true);
                info!("Sifting market stream connected");

                let (mut write, mut read) = stream.split();
                let subscribe = serde_json::json!({
                    "op": "subscribe",
                    "product": "com",
                    "symbols": [symbol]
                });
                if write
                    .send(Message::Text(subscribe.to_string().into()))
                    .await
                    .is_err()
                {
                    warn!("Sifting subscribe frame could not be sent");
                }

                let mut ping = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));
                // The first tick of a tokio interval fires immediately; it is
                // consumed here so the first ping is a full period away.
                ping.tick().await;

                loop {
                    tokio::select! {
                        _ = ping.tick() => {
                            let keepalive = serde_json::json!({ "op": "ping" });
                            if write
                                .send(Message::Text(keepalive.to_string().into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        message = read.next() => {
                            match message {
                                Some(Ok(Message::Text(text))) => handle_text(
                                    &text,
                                    &symbol,
                                    &book,
                                    &publisher,
                                    &status,
                                    chrono::Utc::now().timestamp_millis(),
                                ),
                                Some(Ok(Message::Close(frame))) => {
                                    info!("Sifting stream closed by server: {frame:?}");
                                    break;
                                }
                                Some(Ok(_)) => {}
                                Some(Err(e)) => {
                                    warn!("Sifting stream error: {e}");
                                    break;
                                }
                                None => {
                                    info!("Sifting stream ended");
                                    break;
                                }
                            }
                        }
                    }
                }

                status.set("sifting", "disconnected");
                publisher.set_feed_connected(false);
            }
            Err(e) => {
                error!("Sifting connect failed: {e}");
                status.set("sifting", "error");
                publisher.set_feed_connected(false);
            }
        }

        tokio::time::sleep(Duration::from_secs(RECONNECT_SECS)).await;
    }
}

/// Fold one inbound text frame into the book and publish what it produced.
fn handle_text(
    text: &str,
    symbol: &str,
    book: &CandleBook,
    publisher: &MarketPublisher,
    status: &FeedStatus,
    now_ms: i64,
) {
    let Some((price, ts, size)) = parse_tick(text, symbol) else {
        return;
    };
    let event = book.on_tick(price, ts, size, now_ms);
    if let Some(reason) = event.dropped {
        debug!(ts, reason = reason.as_str(), "Sifting tick dropped");
        return;
    }
    status.mark_msg("sifting");
    publisher.publish_tick(&event);
}

/// Extract `(price, ts_ms, trade_size)` from a Sifting `tick` frame for
/// `symbol`. Control frames, other symbols and unusable payloads return `None`.
///
/// Price *validity* is the book's job: a NaN or non-positive price still has
/// to be counted as a dropped tick, so it is parsed here and refused there.
fn parse_tick(text: &str, symbol: &str) -> Option<(f64, i64, Option<f64>)> {
    let value: Value = serde_json::from_str(text).ok()?;

    match value.get("f").and_then(|kind| kind.as_str()) {
        Some("tick") => {}
        Some("error") => {
            warn!(
                "Sifting server error frame: code={} message={}",
                value.get("code").and_then(|c| c.as_str()).unwrap_or("?"),
                value.get("message").and_then(|m| m.as_str()).unwrap_or("?")
            );
            return None;
        }
        // "ack" / "pong" handshake + keepalive frames, and anything unknown.
        _ => return None,
    }

    if value.get("s").and_then(|s| s.as_str()) != Some(symbol) {
        return None;
    }

    // Prefer the last-trade price `p`, fall back to the bid/ask mid.
    let price = value.get("p").and_then(|v| v.as_f64()).or_else(|| {
        match (
            value.get("b").and_then(|v| v.as_f64()),
            value.get("a").and_then(|v| v.as_f64()),
        ) {
            (Some(bid), Some(ask)) if bid > 0.0 && ask > 0.0 => Some((bid + ask) / 2.0),
            _ => None,
        }
    })?;
    let ts = value
        .get("t")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    // `P` no longer drives volume; it only helps recognise a repeated tick.
    let size = value
        .get("P")
        .and_then(|v| v.as_f64())
        .filter(|size| size.is_finite());

    Some((price, ts, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candles::DropReason;
    use crate::status::FreshnessTracker;
    use crate::tick_volume::TickVolumeStore;
    use crate::types::{Interval, SYMBOL, Sequencer};
    use tokio::sync::broadcast;

    const T0: i64 = 1_700_000_123_000 - 1_700_000_123_000 % (15 * 60_000);
    const NOW: i64 = T0 + 3_600_000;

    fn tick(price: f64, ts: i64) -> String {
        serde_json::json!({
            "f": "tick", "class": "com", "s": SYMBOL, "p": price, "P": 1.0, "t": ts
        })
        .to_string()
    }

    fn harness() -> (
        CandleBook,
        MarketPublisher,
        TickVolumeStore,
        FeedStatus,
        broadcast::Receiver<crate::types::Publication>,
    ) {
        let (tx, rx) = broadcast::channel(256);
        let store = TickVolumeStore::new(16);
        let book = CandleBook::new();
        let publisher = MarketPublisher::new(
            tx,
            Sequencer::new(),
            store.clone(),
            FreshnessTracker::new("sifting"),
        );
        let status = FeedStatus::new();
        status.set("sifting", "connected");
        (book, publisher, store, status, rx)
    }

    #[test]
    fn a_tick_is_folded_into_every_resolution() {
        let (book, publisher, _store, status, mut rx) = harness();
        handle_text(&tick(3380.0, T0), SYMBOL, &book, &publisher, &status, NOW);

        assert_eq!(book.bars(Interval::M1), 0, "the bucket is still forming");
        assert!(book.forming(Interval::M1).is_some());
        assert_eq!(status.snapshot()["sifting"]["msgs"], 1);

        let mut kinds = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            kinds.push(frame.kind);
        }
        assert_eq!(kinds, vec!["candle", "candle", "candle", "tick_volume"]);
    }

    #[test]
    fn a_bucket_roll_publishes_the_closed_bar_first() {
        let (book, publisher, store, status, mut rx) = harness();
        handle_text(&tick(3380.0, T0), SYMBOL, &book, &publisher, &status, NOW);
        while rx.try_recv().is_ok() {}

        handle_text(
            &tick(3381.0, T0 + 15 * 60_000),
            SYMBOL,
            &book,
            &publisher,
            &status,
            NOW,
        );
        let mut frames = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            frames.push(frame);
        }
        assert_eq!(frames[0].kind, "candle");
        assert_eq!(frames[0].data.as_ref().unwrap()["closed"], true);
        assert_eq!(frames[0].data.as_ref().unwrap()["interval"], "1m");
        assert!(store.snapshot().iter().any(|bar| bar.closed));
    }

    #[test]
    fn control_frames_and_other_symbols_are_ignored() {
        let (book, publisher, _store, status, mut rx) = harness();
        for frame in [
            r#"{"f":"pong"}"#.to_string(),
            r#"{"f":"ack","op":"auth"}"#.to_string(),
            tick(3380.0, T0).replace(SYMBOL, "XAGUSD"),
            "not json".to_string(),
        ] {
            handle_text(&frame, SYMBOL, &book, &publisher, &status, NOW);
        }
        assert!(rx.try_recv().is_err());
        assert!(book.forming(Interval::M1).is_none());
    }

    #[test]
    fn an_invalid_price_is_counted_as_a_drop() {
        let (book, publisher, _store, status, mut rx) = harness();
        handle_text(&tick(0.0, T0), SYMBOL, &book, &publisher, &status, NOW);
        assert_eq!(book.counters().price, 1);
        assert_eq!(book.counters().accepted, 0);
        assert!(rx.try_recv().is_err());
        assert_eq!(status.snapshot()["sifting"]["msgs"], 0);
    }

    #[test]
    fn a_second_based_timestamp_is_counted_as_a_drop() {
        let (book, publisher, _store, status, _rx) = harness();
        handle_text(
            &tick(3380.0, T0 / 1000),
            SYMBOL,
            &book,
            &publisher,
            &status,
            NOW,
        );
        assert_eq!(book.counters().seconds_timestamp, 1);
    }

    #[test]
    fn a_tick_ahead_of_the_clock_is_dropped() {
        let (book, publisher, _store, status, _rx) = harness();
        handle_text(
            &tick(3380.0, NOW + 6_000),
            SYMBOL,
            &book,
            &publisher,
            &status,
            NOW,
        );
        assert_eq!(book.counters().future, 1);
    }

    #[test]
    fn the_bid_ask_mid_is_used_when_no_last_trade_is_printed() {
        let raw = serde_json::json!({
            "f": "tick", "class": "com", "s": SYMBOL, "b": 3379.0, "a": 3381.0, "t": T0
        })
        .to_string();
        let (price, ts, size) = parse_tick(&raw, SYMBOL).expect("mid");
        assert_eq!(price, 3380.0);
        assert_eq!(ts, T0);
        assert!(size.is_none());
    }

    #[test]
    fn a_tick_without_any_usable_price_is_ignored() {
        let raw = serde_json::json!({
            "f": "tick", "class": "com", "s": SYMBOL, "b": 0.0, "a": 0.0, "t": T0
        })
        .to_string();
        assert!(parse_tick(&raw, SYMBOL).is_none());
    }

    #[test]
    fn the_drop_reason_names_are_stable() {
        assert_eq!(DropReason::Late.as_str(), "late");
        assert_eq!(DropReason::SecondsTimestamp.as_str(), "seconds_timestamp");
    }
}
