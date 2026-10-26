//! Startup history from Node 1's REST `/candles` endpoint.
//!
//! Node 1's WebSocket replay carries only a few hours of bars per resolution, but
//! the weekly windows (PW, PS, CW) need days of closed history. Node 3 therefore
//! loads every resolution over REST on each connection, before it trusts the live
//! frames. A reconnect repeats the load, because Node 1 may have restarted.
//!
//! Every response is checked before any bar is used: the symbol, the requested
//! resolution, the `closed` flag, bucket alignment, finite prices, and strictly
//! ascending unique open times. Any violation fails the whole load, and the
//! caller reconnects without recording a partial history.

use std::time::Duration;

use serde::Deserialize;

use crate::feed::{validate_candle, SYMBOL};
use crate::store::{interval_ms, INTERVALS};
use crate::types::VpCandle;

/// Request timeout for each `/candles` call. The 1m series is the largest response.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// One bar as Node 1 serialises `types::Candle`.
#[derive(Debug, Deserialize)]
struct RestCandle {
    symbol: String,
    interval: String,
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    #[serde(default)]
    source: String,
    closed: bool,
}

/// Derive the REST origin from the feed WebSocket URL: `wss://host/ws` becomes
/// `https://host`, and `ws://host/ws` becomes `http://host`.
pub fn rest_base(ws_url: &str) -> Result<String, String> {
    let (scheme, rest) = if let Some(rest) = ws_url.strip_prefix("wss://") {
        ("https", rest)
    } else if let Some(rest) = ws_url.strip_prefix("ws://") {
        ("http", rest)
    } else {
        return Err(format!(
            "Node 1 URL '{ws_url}' must start with ws:// or wss://"
        ));
    };
    let path = rest.trim_end_matches('/');
    let host = path.strip_suffix("/ws").unwrap_or(path);
    if host.is_empty() {
        return Err(format!("Node 1 URL '{ws_url}' has no host"));
    }
    Ok(format!("{scheme}://{host}"))
}

/// The `/candles` URL for one resolution.
pub fn candles_url(base: &str, interval: &str) -> String {
    format!("{}/candles?interval={interval}", base.trim_end_matches('/'))
}

/// Parse and validate one `/candles` body. `Err` means no bar from it may be used.
pub fn parse_history(interval: &str, body: &str) -> Result<Vec<VpCandle>, String> {
    let step = interval_ms(interval).ok_or_else(|| format!("unsupported interval '{interval}'"))?;
    let rows: Vec<RestCandle> = serde_json::from_str(body)
        .map_err(|error| format!("{interval}: malformed /candles body: {error}"))?;

    let mut bars: Vec<VpCandle> = Vec::with_capacity(rows.len());
    for row in rows {
        if row.symbol != SYMBOL {
            return Err(format!("{interval}: unexpected symbol '{}'", row.symbol));
        }
        if row.interval != interval {
            return Err(format!(
                "{interval}: bar at {} is labelled '{}'",
                row.time, row.interval
            ));
        }
        if !row.closed {
            return Err(format!("{interval}: open bar at {} in history", row.time));
        }
        let candle = VpCandle {
            time: row.time,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
            source: row.source,
        };
        validate_candle(&candle)?;
        if candle.time % step != 0 {
            return Err(format!(
                "{interval}: bar at {} is not bucket-aligned",
                candle.time
            ));
        }
        if let Some(previous) = bars.last() {
            if candle.time <= previous.time {
                return Err(format!(
                    "{interval}: times must strictly ascend ({} after {})",
                    candle.time, previous.time
                ));
            }
        }
        bars.push(candle);
    }
    Ok(bars)
}

async fn fetch_interval(
    client: &reqwest::Client,
    base: &str,
    interval: &str,
) -> Result<Vec<VpCandle>, String> {
    let url = candles_url(base, interval);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("{interval}: request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{interval}: /candles returned HTTP {status}"));
    }
    let body = response
        .text()
        .await
        .map_err(|error| format!("{interval}: reading /candles failed: {error}"))?;
    parse_history(interval, &body)
}

/// Fetch and validate every resolution. Nothing is returned unless all three pass.
pub async fn fetch_all(base: &str) -> Result<Vec<(&'static str, Vec<VpCandle>)>, String> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|error| format!("HTTP client setup failed: {error}"))?;
    let mut loaded = Vec::with_capacity(INTERVALS.len());
    for interval in INTERVALS {
        let bars = fetch_interval(&client, base, interval).await?;
        loaded.push((interval, bars));
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M15: i64 = 900_000;
    const T0: i64 = 1_700_000_100_000 - 1_700_000_100_000 % M15;

    fn bar(time: i64, interval: &str) -> String {
        format!(
            r#"{{"symbol":"XAUUSD","interval":"{interval}","time":{time},"open":4100.0,"high":4102.0,"low":4099.0,"close":4101.0,"volume":12.0,"source":"sifting","closed":true}}"#
        )
    }

    fn body(rows: &[String]) -> String {
        format!("[{}]", rows.join(","))
    }

    #[test]
    fn derives_the_rest_origin_from_the_feed_url() {
        assert_eq!(
            rest_base("wss://engine.example.com/ws").unwrap(),
            "https://engine.example.com"
        );
        assert_eq!(
            rest_base("ws://127.0.0.1:8080/ws").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            rest_base("wss://engine.example.com/ws/").unwrap(),
            "https://engine.example.com"
        );
        assert_eq!(
            rest_base("wss://engine.example.com").unwrap(),
            "https://engine.example.com"
        );
    }

    #[test]
    fn rejects_other_schemes_and_empty_hosts() {
        assert!(rest_base("https://engine.example.com/ws").is_err());
        assert!(rest_base("wss:///ws").is_err());
        assert!(rest_base("wss://").is_err());
    }

    #[test]
    fn builds_the_candles_url_for_each_resolution() {
        assert_eq!(
            candles_url("https://engine.example.com/", "1h"),
            "https://engine.example.com/candles?interval=1h"
        );
        assert_eq!(
            candles_url("http://localhost:8080", "1m"),
            "http://localhost:8080/candles?interval=1m"
        );
    }

    #[test]
    fn accepts_ascending_closed_bars_and_keeps_their_values() {
        let rows = [bar(T0, "15m"), bar(T0 + M15, "15m")];
        let bars = parse_history("15m", &body(&rows)).unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].time, T0);
        assert_eq!(bars[1].time, T0 + M15);
        assert_eq!(bars[0].close, 4101.0);
        assert_eq!(bars[0].source, "sifting");
    }

    #[test]
    fn an_empty_history_is_valid_but_empty() {
        assert!(parse_history("1h", "[]").unwrap().is_empty());
    }

    #[test]
    fn rejects_a_malformed_body() {
        assert!(parse_history("15m", "{\"error\":\"nope\"}").is_err());
        assert!(parse_history("15m", "not json").is_err());
    }

    #[test]
    fn rejects_unsupported_interval_request() {
        assert!(parse_history("5m", "[]").is_err());
    }

    #[test]
    fn rejects_out_of_order_bars() {
        let rows = [bar(T0 + M15, "15m"), bar(T0, "15m")];
        assert!(parse_history("15m", &body(&rows)).is_err());
    }

    #[test]
    fn rejects_duplicate_bars() {
        let rows = [bar(T0, "15m"), bar(T0, "15m")];
        assert!(parse_history("15m", &body(&rows)).is_err());
    }

    #[test]
    fn rejects_an_open_bar_in_history() {
        let open = bar(T0, "15m").replace("\"closed\":true", "\"closed\":false");
        assert!(parse_history("15m", &body(&[open])).is_err());
    }

    #[test]
    fn rejects_wrong_symbol_or_resolution() {
        let wrong_symbol = bar(T0, "15m").replace("XAUUSD", "EURUSD");
        assert!(parse_history("15m", &body(&[wrong_symbol])).is_err());
        let wrong_interval = bar(T0, "1h");
        assert!(parse_history("15m", &body(&[wrong_interval])).is_err());
    }

    #[test]
    fn rejects_a_misaligned_bucket() {
        assert!(parse_history("15m", &body(&[bar(T0 + 1_000, "15m")])).is_err());
    }

    #[test]
    fn rejects_malformed_prices() {
        let inverted = bar(T0, "15m").replace("\"high\":4102.0", "\"high\":4098.0");
        assert!(parse_history("15m", &body(&[inverted])).is_err());
        let negative = bar(T0, "15m").replace("\"volume\":12.0", "\"volume\":-1.0");
        assert!(parse_history("15m", &body(&[negative])).is_err());
    }
}
