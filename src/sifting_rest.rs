//! SiftingIO historical bars (the only candle history source).
//!
//! One endpoint serves every resolution Node 1 publishes; the seed depth per
//! resolution is chosen from what consumers actually need:
//!
//! * `1m` — a full trading week plus the current one, so the weekly windows can
//!   be rebuilt from Node 1 candles alone (~7,200 bars, fetched in pages).
//! * `15m` — the chart's fixed 2,000-bar seed.
//! * `1h` — the drawing-boundary resolution, 2,000 bars.
//!
//! REST history can lag the live stream, and the live stream only ever appends
//! from "now", so boot also asks for the tail explicitly
//! ([`fetch_since`]) instead of shipping a chart with a hole in it.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Deserialize;
use tracing::warn;

use crate::types::{Candle, Interval, SYMBOL};

/// The 15m/1h seed span, unchanged from the chart's existing contract.
pub const SEED_BARS: usize = 2_000;
/// One ascending page is enough to close a restart hole: 500 bars is at least
/// five days at 15m. A longer outage is reported instead.
const TAIL_BARS: usize = 500;
const PAGE_LIMIT: usize = 2_000;
const MAX_PAGES: usize = 40;

const SOURCE: &str = "sifting";

/// How far back the seed request looks for each resolution. 1m needs the
/// previous complete week; 15m and 1h need their full 2,000-bar span.
fn lookback(interval: Interval) -> Duration {
    match interval {
        Interval::M1 => Duration::days(14),
        Interval::M15 => Duration::days(60),
        Interval::H1 => Duration::days(400),
    }
}

/// A single OHLC bar from SiftingIO's commodities historical endpoint. The API
/// returns Unix epoch milliseconds in `t` and tick-count volume in `v`.
#[derive(Debug, Deserialize)]
struct SiftingBar {
    #[serde(rename = "t")]
    time: i64,
    #[serde(rename = "o")]
    open: f64,
    #[serde(rename = "h")]
    high: f64,
    #[serde(rename = "l")]
    low: f64,
    #[serde(rename = "c")]
    close: f64,
    #[serde(rename = "v")]
    volume: f64,
}

#[derive(Debug, Default, Deserialize)]
struct SiftingBarsMeta {
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    interval: Option<String>,
    #[serde(rename = "next_cursor", default)]
    next_cursor: Option<String>,
}

/// The top-level response from SiftingIO's historical bars endpoint.
#[derive(Debug, Deserialize)]
struct SiftingBarsResponse {
    data: Vec<SiftingBar>,
    #[serde(default)]
    meta: SiftingBarsMeta,
}

// Eight positional arguments, one per query parameter plus the pagination
// cursor. Every one is read at the call site, so a struct would only move the
// noise.
#[allow(clippy::too_many_arguments)]
fn history_url(
    base_url: &str,
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    interval: &str,
    order: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<reqwest::Url> {
    let endpoint = format!(
        "{}/v1/hist/commodities/{}/bars",
        base_url.trim_end_matches('/'),
        symbol
    );
    let mut url = reqwest::Url::parse(&endpoint).context("invalid SiftingIO history URL")?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("start", &start.to_rfc3339_opts(SecondsFormat::Millis, true))
            .append_pair("end", &end.to_rfc3339_opts(SecondsFormat::Millis, true))
            .append_pair("interval", interval)
            .append_pair("order", order)
            .append_pair("limit", &limit.to_string());
        if let Some(cursor) = cursor {
            query.append_pair("cursor", cursor);
        }
    }
    Ok(url)
}

fn parse_page(
    response: SiftingBarsResponse,
    symbol: &str,
    interval: Interval,
) -> Result<(Vec<Candle>, Option<String>)> {
    if let Some(response_symbol) = response.meta.symbol.as_deref()
        && !response_symbol.eq_ignore_ascii_case(symbol)
    {
        anyhow::bail!("SiftingIO returned symbol {response_symbol}, expected {symbol}");
    }
    // Accept the upstream's synonyms (`60m` for `1h`) but nothing else.
    if let Some(returned) = response.meta.interval.as_deref()
        && Interval::parse(returned) != Some(interval)
    {
        anyhow::bail!(
            "SiftingIO returned interval {returned}, expected {}",
            interval.as_str()
        );
    }

    let mut candles: Vec<Candle> = response
        .data
        .into_iter()
        .map(|bar| Candle {
            symbol: SYMBOL.to_string(),
            interval: interval.as_str().to_string(),
            time: bar.time,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
            source: SOURCE.to_string(),
            closed: true,
        })
        .collect();

    if candles.iter().any(|candle| !has_valid_values(candle)) {
        anyhow::bail!("SiftingIO returned an invalid OHLCV bar");
    }

    candles.sort_by_key(|candle| candle.time);
    if candles.windows(2).any(|pair| pair[0].time == pair[1].time) {
        anyhow::bail!("SiftingIO returned duplicate candle timestamps");
    }

    Ok((candles, response.meta.next_cursor))
}

async fn get_page(
    client: &reqwest::Client,
    url: reqwest::Url,
    api_key: &str,
    symbol: &str,
    interval: Interval,
) -> Result<(Vec<Candle>, Option<String>)> {
    let resp = client
        .get(url)
        .header("X-API-Key", api_key)
        .header("Accept-Encoding", "gzip")
        .send()
        .await
        .context("failed to send request to SiftingIO")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "SiftingIO historical bars returned {status}: {}",
            truncate(&body, 500)
        );
    }

    let response: SiftingBarsResponse = resp
        .json()
        .await
        .context("failed to parse SiftingIO bars response")?;
    parse_page(response, symbol, interval)
}

/// Walk the cursor until the page is exhausted, returning every row.
async fn fetch_pages(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    interval: Interval,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    order: &str,
) -> Result<Vec<Candle>> {
    let client = reqwest::Client::new();
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();

    for page_index in 0..MAX_PAGES {
        let url = history_url(
            base_url,
            symbol,
            start,
            end,
            interval.as_str(),
            order,
            PAGE_LIMIT,
            cursor.as_deref(),
        )?;
        let (mut page, next_cursor) =
            get_page(&client, url, api_key, symbol, interval).await?;
        all.append(&mut page);

        match next_cursor.filter(|next| !next.is_empty()) {
            Some(next) => {
                if !seen_cursors.insert(next.clone()) {
                    anyhow::bail!("SiftingIO repeated a history cursor");
                }
                if page_index + 1 == MAX_PAGES {
                    anyhow::bail!("SiftingIO history exceeded {MAX_PAGES} pages");
                }
                cursor = Some(next);
            }
            None => break,
        }
    }

    all.sort_by_key(|candle| candle.time);
    all.dedup_by_key(|candle| candle.time);
    Ok(all)
}

/// Fetch the boot seed for one resolution.
pub async fn fetch_seed(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    interval: Interval,
) -> Result<Vec<Candle>> {
    let end = Utc::now();
    let start = end - lookback(interval);
    let order = if interval == Interval::M1 { "asc" } else { "desc" };
    let mut bars = fetch_pages(base_url, api_key, symbol, interval, start, end, order).await?;

    // The seed is bounded; keep the newest rows.
    if bars.len() > SEED_BARS && interval != Interval::M1 {
        let skip = bars.len() - SEED_BARS;
        bars = bars.into_iter().skip(skip).collect();
    }
    if bars.len() < SEED_BARS && interval != Interval::M1 {
        warn!(
            "SiftingIO {} history returned {} bars, short of the {} bar seed",
            interval.as_str(),
            bars.len(),
            SEED_BARS
        );
    }
    Ok(bars)
}

/// Fetch the bars newer than `after_ms`, ascending. SiftingIO's `start` is
/// inclusive, so the boundary bar the seed already has is dropped.
pub async fn fetch_since(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    interval: Interval,
    after_ms: i64,
) -> Result<Vec<Candle>> {
    let now = Utc::now();
    let start = DateTime::<Utc>::from_timestamp_millis(after_ms + 1)
        .context("invalid history tail timestamp")?;
    if start >= now {
        return Ok(Vec::new());
    }
    let client = reqwest::Client::new();
    let url = history_url(
        base_url,
        symbol,
        start,
        now,
        interval.as_str(),
        "asc",
        TAIL_BARS,
        None,
    )?;
    let (candles, _) = get_page(&client, url, api_key, symbol, interval).await?;
    Ok(candles
        .into_iter()
        .filter(|candle| candle.time > after_ms)
        .collect())
}

fn truncate(text: &str, limit: usize) -> &str {
    match text.get(..limit) {
        Some(prefix) => prefix,
        None => text,
    }
}

/// `Candle::is_valid` also checks bucket alignment, which the book (the single
/// authority on what enters the cache) enforces and counts. The page parser
/// only rejects values that are impossible for *any* bar.
fn has_valid_values(candle: &Candle) -> bool {
    let positive = |value: f64| value.is_finite() && value > 0.0;
    positive(candle.open)
        && positive(candle.high)
        && positive(candle.low)
        && positive(candle.close)
        && candle.volume.is_finite()
        && candle.volume >= 0.0
        && candle.high >= candle.low
        && candle.time > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_for(interval: Interval) -> String {
        history_url(
            "https://api.sifting.io/",
            "XAUUSD",
            Utc::now() - Duration::days(7),
            Utc::now(),
            interval.as_str(),
            "asc",
            PAGE_LIMIT,
            Some("cursor/with+reserved=chars"),
        )
        .expect("url")
        .to_string()
    }

    fn page(rows: serde_json::Value, interval: &str) -> SiftingBarsResponse {
        serde_json::from_value(serde_json::json!({
            "data": rows,
            "meta": { "symbol": "XAUUSD", "interval": interval }
        }))
        .expect("page")
    }

    #[test]
    fn history_requests_carry_interval_order_limit_and_cursor() {
        let url = url_for(Interval::M1);
        assert!(url.starts_with("https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?"));
        assert!(url.contains("interval=1m"));
        assert!(url.contains("order=asc"));
        assert!(url.contains("limit=2000"));
        assert!(url.contains("cursor=cursor%2Fwith%2Breserved%3Dchars"));
        assert!(url.contains("start="));
        assert!(url.contains("end="));

        assert!(url_for(Interval::H1).contains("interval=1h"));
        assert!(url_for(Interval::M15).contains("interval=15m"));
    }

    #[test]
    fn the_seed_lookback_covers_the_bars_it_asks_for() {
        assert!(lookback(Interval::M15) >= Duration::days(21));
        assert!(lookback(Interval::H1) >= Duration::days(84));
        assert!(lookback(Interval::M1) >= Duration::days(12));
    }

    #[test]
    fn a_page_is_sorted_ascending_and_typed() {
        let response = page(
            serde_json::json!([
                { "t": 1_700_000_900_000_i64, "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5, "v": 10.0 },
                { "t": 1_700_000_000_000_i64, "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5, "v": 10.0 }
            ]),
            "15m",
        );
        let (candles, next) = parse_page(response, "XAUUSD", Interval::M15).expect("page");
        assert!(next.is_none());
        assert_eq!(candles[0].time, 1_700_000_000_000);
        assert_eq!(candles[1].time, 1_700_000_900_000);
        assert_eq!(candles[0].symbol, SYMBOL);
        assert_eq!(candles[0].interval, "15m");
        assert_eq!(candles[0].source, "sifting");
        assert!(candles.iter().all(|candle| candle.closed));
    }

    #[test]
    fn response_validation_rejects_wrong_symbol_or_interval() {
        let response: SiftingBarsResponse = serde_json::from_value(serde_json::json!({
            "data": [],
            "meta": { "symbol": "XAGUSD", "interval": "1m" }
        }))
        .unwrap();
        assert!(parse_page(response, "XAUUSD", Interval::M1).is_err());

        let response: SiftingBarsResponse = serde_json::from_value(serde_json::json!({
            "data": [],
            "meta": { "symbol": "XAUUSD", "interval": "5m" }
        }))
        .unwrap();
        assert!(parse_page(response, "XAUUSD", Interval::M1).is_err());
    }

    #[test]
    fn impossible_bars_and_duplicate_timestamps_are_rejected() {
        let response = page(
            serde_json::json!([
                { "t": 1_700_000_000_000_i64, "o": 1.0, "h": 0.5, "l": 2.0, "c": 1.5, "v": 10.0 }
            ]),
            "15m",
        );
        assert!(parse_page(response, "XAUUSD", Interval::M15).is_err());

        let response = page(
            serde_json::json!([
                { "t": 1_700_000_000_000_i64, "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5, "v": 10.0 },
                { "t": 1_700_000_000_000_i64, "o": 1.0, "h": 2.0, "l": 0.5, "c": 1.5, "v": 11.0 }
            ]),
            "15m",
        );
        assert!(parse_page(response, "XAUUSD", Interval::M15).is_err());
    }

    #[test]
    fn an_empty_page_is_not_an_error() {
        let response = page(serde_json::json!([]), "1h");
        let (candles, _) = parse_page(response, "XAUUSD", Interval::H1).expect("empty page");
        assert!(candles.is_empty());
    }

    #[test]
    fn truncate_never_splits_a_utf8_boundary() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("abc", 99), "abc");
        assert_eq!(truncate("é", 1), "é");
    }
}
