//! SiftingIO historical bars (the only candle history source).
//!
//! One endpoint serves every resolution Node 1 publishes; the seed depth per
//! resolution is chosen from what consumers actually need:
//!
//! * `1m` — a full trading week plus the current one, so the weekly windows can
//!   be rebuilt from Node 1 candles alone (~7,200+ bars, fetched in pages).
//! * `15m` — the chart's fixed 2,000-bar seed.
//! * `1h` — the drawing-boundary resolution, 2,000 bars.
//!
//! When `start` predates SiftingIO's upstream coverage for XAUUSD (common on
//! the 60-day `15m` and 400-day `1h` lookbacks), SiftingIO responds with HTTP
//! `422` (`{"error":"data_unavailable","earliest":"..."}`). [`fetch_seed`]
//! clamps `start` to `earliest` (or steps down the lookback window when
//! `earliest` is absent) and retries so coarse history still seeds at boot.
//!
//! REST history can lag the live stream, and the live stream only ever appends
//! from "now", so boot also asks for the tail explicitly
//! ([`fetch_since`]) instead of shipping a chart with a hole in it.

use std::collections::HashSet;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde::Deserialize;
use tracing::{info, warn};

use crate::types::{Candle, Interval, SYMBOL};

/// The 15m/1h seed span, unchanged from the chart's existing contract.
pub const SEED_BARS: usize = 2_000;
/// One ascending page is enough to close a restart hole: 500 bars is at least
/// five days at 15m. A longer outage is reported instead.
const TAIL_BARS: usize = 500;
const PAGE_LIMIT: usize = 2_000;
const MAX_PAGES: usize = 40;
const MAX_RETRIES: usize = 3;
const MAX_RETRY_AFTER_SECS: f64 = 15.0;
const REQUEST_TIMEOUT_SECS: u64 = 30;

const SOURCE: &str = "sifting";

/// Progressive lookback ladder used if upstream returns `422 data_unavailable`
/// without a usable `earliest` date.
const FALLBACK_LOOKBACK_DAYS: [i64; 5] = [90, 30, 14, 7, 2];

/// How far back the initial seed request looks for each resolution. 1m needs
/// the previous complete week; 15m and 1h ask for their full 2,000-bar span and
/// clamp to upstream `earliest` if coverage starts more recently.
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

#[derive(Debug)]
enum PageError {
    /// HTTP 422 / `data_unavailable` when `start` predates upstream coverage.
    DataUnavailable {
        earliest: Option<DateTime<Utc>>,
        message: String,
    },
    Other(anyhow::Error),
}

impl PageError {
    fn into_anyhow(self) -> anyhow::Error {
        match self {
            PageError::DataUnavailable { message, .. } => anyhow::anyhow!(message),
            PageError::Other(err) => err,
        }
    }
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(StdDuration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .context("failed to build SiftingIO HTTP client")
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

/// Extract `earliest` from a SiftingIO `422 data_unavailable` error payload.
/// Accepts RFC 3339 timestamps, `YYYY-MM-DD` UTC dates, and epoch ms/s values.
fn parse_earliest(body: &str) -> Option<DateTime<Utc>> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let field = parsed.get("earliest")?;
    if let Some(raw) = field.as_str() {
        let trimmed = raw.trim();
        if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
            return Some(dt.with_timezone(&Utc));
        }
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
            return date.and_hms_opt(0, 0, 0).map(|naive| naive.and_utc());
        }
        if let Ok(num) = trimmed.parse::<i64>() {
            return timestamp_to_utc(num);
        }
        return None;
    }
    field.as_i64().and_then(timestamp_to_utc)
}

fn timestamp_to_utc(value: i64) -> Option<DateTime<Utc>> {
    if value >= 1_000_000_000_000 {
        DateTime::<Utc>::from_timestamp_millis(value)
    } else if value > 0 {
        DateTime::<Utc>::from_timestamp(value, 0)
    } else {
        None
    }
}

fn is_data_unavailable(status: StatusCode, body: &str) -> bool {
    if status == StatusCode::UNPROCESSABLE_ENTITY {
        return true;
    }
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .is_some_and(|code| code == "data_unavailable")
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || (status.is_server_error() && status != StatusCode::NOT_IMPLEMENTED)
}

fn parse_retry_after(headers: &HeaderMap, body: &str) -> Option<StdDuration> {
    if let Some(secs) = headers
        .get(RETRY_AFTER)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
    {
        return Some(StdDuration::from_secs_f64(secs.min(MAX_RETRY_AFTER_SECS)));
    }
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    for key in ["retry_after", "retry_after_seconds"] {
        if let Some(secs) = parsed
            .get(key)
            .and_then(|v| {
                v.as_f64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .filter(|secs| secs.is_finite() && *secs >= 0.0)
        {
            return Some(StdDuration::from_secs_f64(secs.min(MAX_RETRY_AFTER_SECS)));
        }
    }
    None
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
) -> std::result::Result<(Vec<Candle>, Option<String>), PageError> {
    for attempt in 0..=MAX_RETRIES {
        let resp = match client
            .get(url.clone())
            .header("X-API-Key", api_key)
            .header("Accept-Encoding", "gzip")
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) if attempt < MAX_RETRIES => {
                let delay = StdDuration::from_millis(250 * (1_u64 << attempt));
                tokio::time::sleep(delay).await;
                continue;
            }
            Err(err) => {
                return Err(PageError::Other(
                    anyhow::Error::new(err).context("failed to send request to SiftingIO"),
                ));
            }
        };

        let status = resp.status();
        if status.is_success() {
            let response: SiftingBarsResponse = resp
                .json()
                .await
                .context("failed to parse SiftingIO bars response")
                .map_err(PageError::Other)?;
            return parse_page(response, symbol, interval).map_err(PageError::Other);
        }

        let headers = resp.headers().clone();
        let body = resp.text().await.unwrap_or_default();

        if is_retryable_status(status) && attempt < MAX_RETRIES {
            let delay = parse_retry_after(&headers, &body)
                .unwrap_or_else(|| StdDuration::from_millis(250 * (1_u64 << attempt)));
            warn!(
                "SiftingIO {} history HTTP {status} (attempt {}/{}); retrying in {}ms",
                interval.as_str(),
                attempt + 1,
                MAX_RETRIES + 1,
                delay.as_millis()
            );
            tokio::time::sleep(delay).await;
            continue;
        }

        let message = format!(
            "SiftingIO historical bars returned {status}: {}",
            truncate(&body, 500)
        );
        if is_data_unavailable(status, &body) {
            return Err(PageError::DataUnavailable {
                earliest: parse_earliest(&body),
                message,
            });
        }
        return Err(PageError::Other(anyhow::anyhow!(message)));
    }

    Err(PageError::Other(anyhow::anyhow!(
        "SiftingIO historical bars exhausted retry budget"
    )))
}

/// Walk the cursor until the page is exhausted (or `max_bars` closed bars have
/// been collected on a `desc` pull), returning closed rows sorted ascending.
#[allow(clippy::too_many_arguments)]
async fn fetch_pages(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    symbol: &str,
    interval: Interval,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    order: &str,
    max_bars: Option<usize>,
) -> std::result::Result<Vec<Candle>, PageError> {
    let end_ms = end.timestamp_millis();
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
        )
        .map_err(PageError::Other)?;

        let (mut page, next_cursor) = match get_page(client, url, api_key, symbol, interval).await {
            Ok(result) => result,
            // When walking backwards (`order=desc`), a later page can step past
            // upstream's earliest date after recent bars were already fetched.
            Err(PageError::DataUnavailable { .. }) if page_index > 0 && !all.is_empty() => break,
            Err(err) => return Err(err),
        };

        // Exclude any bucket whose period `[time, time + interval)` is still
        // open at `end`: `/candles` serves closed bars only.
        page.retain(|candle| candle.time + interval.millis() <= end_ms);
        all.append(&mut page);

        if let Some(target) = max_bars
            && all.len() >= target
        {
            break;
        }

        match next_cursor.filter(|next| !next.is_empty()) {
            Some(next) => {
                if !seen_cursors.insert(next.clone()) {
                    return Err(PageError::Other(anyhow::anyhow!(
                        "SiftingIO repeated a history cursor"
                    )));
                }
                if page_index + 1 == MAX_PAGES {
                    return Err(PageError::Other(anyhow::anyhow!(
                        "SiftingIO history exceeded {MAX_PAGES} pages"
                    )));
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

async fn fetch_seed_at(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    interval: Interval,
    end: DateTime<Utc>,
) -> Result<Vec<Candle>> {
    let client = http_client()?;
    let mut start = end - lookback(interval);
    let order = if interval == Interval::M1 {
        "asc"
    } else {
        "desc"
    };
    let max_bars = (interval != Interval::M1).then_some(SEED_BARS);

    for _ in 0..=FALLBACK_LOOKBACK_DAYS.len() + 1 {
        match fetch_pages(
            &client, base_url, api_key, symbol, interval, start, end, order, max_bars,
        )
        .await
        {
            Ok(mut bars) => {
                if let Some(limit) = max_bars
                    && bars.len() > limit
                {
                    let skip = bars.len() - limit;
                    bars.drain(..skip);
                }
                if bars.len() < SEED_BARS && interval != Interval::M1 {
                    warn!(
                        "SiftingIO {} history returned {} bars, short of the {} bar seed",
                        interval.as_str(),
                        bars.len(),
                        SEED_BARS
                    );
                }
                return Ok(bars);
            }
            Err(PageError::DataUnavailable { earliest, message }) => {
                if let Some(earliest_dt) = earliest
                    && earliest_dt > start
                    && earliest_dt < end
                {
                    info!(
                        "SiftingIO {} lookback predated coverage; clamping start to {}",
                        interval.as_str(),
                        earliest_dt.to_rfc3339()
                    );
                    start = earliest_dt;
                    continue;
                }
                if let Some(next_start) = FALLBACK_LOOKBACK_DAYS
                    .iter()
                    .map(|days| end - Duration::days(*days))
                    .find(|candidate| *candidate > start && *candidate < end)
                {
                    warn!(
                        "SiftingIO {} history returned data_unavailable ({message}); retrying from {}",
                        interval.as_str(),
                        next_start.to_rfc3339()
                    );
                    start = next_start;
                    continue;
                }
                return Err(anyhow::anyhow!(message));
            }
            Err(PageError::Other(err)) => return Err(err),
        }
    }

    anyhow::bail!(
        "SiftingIO {} history remained unavailable after clamping lookback",
        interval.as_str()
    )
}

/// Fetch the boot seed for one resolution.
pub async fn fetch_seed(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    interval: Interval,
) -> Result<Vec<Candle>> {
    fetch_seed_at(base_url, api_key, symbol, interval, Utc::now()).await
}

/// Fetch the closed bars newer than `after_ms`, ascending. SiftingIO's `start`
/// is inclusive, so the boundary bar the seed already has (and any forming bar
/// still open at `now`) is dropped.
pub async fn fetch_since(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    interval: Interval,
    after_ms: i64,
) -> Result<Vec<Candle>> {
    let now = Utc::now();
    let now_ms = now.timestamp_millis();
    let start = DateTime::<Utc>::from_timestamp_millis(after_ms + 1)
        .context("invalid history tail timestamp")?;
    if start >= now {
        return Ok(Vec::new());
    }
    let client = http_client()?;
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
    let (candles, _) = get_page(&client, url, api_key, symbol, interval)
        .await
        .map_err(PageError::into_anyhow)?;
    Ok(candles
        .into_iter()
        .filter(|candle| candle.time > after_ms && candle.time + interval.millis() <= now_ms)
        .collect())
}

fn truncate(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
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
    use axum::{
        Json, Router,
        extract::Query,
        http::HeaderValue,
        response::{IntoResponse, Response},
        routing::get,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    async fn spawn_mock(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        format!("http://{addr}")
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
    fn earliest_date_is_parsed_across_upstream_formats() {
        let by_date = parse_earliest(r#"{"error":"data_unavailable","earliest":"2026-08-02"}"#)
            .expect("date");
        assert_eq!(by_date.to_rfc3339(), "2026-08-02T00:00:00+00:00");

        let by_rfc =
            parse_earliest(r#"{"error":"data_unavailable","earliest":"2026-08-02T12:30:00Z"}"#)
                .expect("rfc3339");
        assert_eq!(by_rfc.to_rfc3339(), "2026-08-02T12:30:00+00:00");

        let by_ms = parse_earliest(r#"{"error":"data_unavailable","earliest":1785628800000}"#)
            .expect("epoch ms");
        assert_eq!(by_ms.timestamp_millis(), 1_785_628_800_000);

        assert!(parse_earliest(r#"{"error":"not_found"}"#).is_none());
        assert!(parse_earliest("not json").is_none());
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
        assert_eq!(truncate("é", 1), "");
        assert_eq!(truncate("é", 2), "é");
    }

    #[tokio::test]
    async fn fetch_seed_clamps_422_data_unavailable_to_earliest_and_drops_forming_bar() {
        let end = DateTime::parse_from_rfc3339("2026-10-26T18:37:19Z")
            .unwrap()
            .with_timezone(&Utc);
        let earliest = DateTime::parse_from_rfc3339("2026-09-15T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let closed_h1 = Interval::H1.align(end.timestamp_millis()) - Interval::H1.millis();
        let forming_h1 = Interval::H1.align(end.timestamp_millis());
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_route = calls.clone();

        let router = Router::new().route(
            "/v1/hist/commodities/XAUUSD/bars",
            get(move |Query(params): Query<HashMap<String, String>>| {
                let calls = calls_for_route.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let start = DateTime::parse_from_rfc3339(params.get("start").unwrap())
                        .unwrap()
                        .with_timezone(&Utc);
                    let interval = params.get("interval").cloned().unwrap_or_default();
                    if start < earliest {
                        return (
                            StatusCode::UNPROCESSABLE_ENTITY,
                            Json(serde_json::json!({
                                "error": "data_unavailable",
                                "message": "start predates upstream coverage",
                                "earliest": "2026-09-15"
                            })),
                        )
                            .into_response();
                    }
                    Json(serde_json::json!({
                        "data": [
                            { "t": forming_h1, "o": 3381.0, "h": 3383.0, "l": 3380.0, "c": 3382.0, "v": 15.0 },
                            { "t": closed_h1, "o": 3378.0, "h": 3382.0, "l": 3377.0, "c": 3381.0, "v": 420.0 },
                            { "t": closed_h1 - 3_600_000, "o": 3375.0, "h": 3379.0, "l": 3374.0, "c": 3378.0, "v": 390.0 }
                        ],
                        "meta": {
                            "symbol": "XAUUSD",
                            "interval": interval
                        }
                    }))
                    .into_response()
                }
            }),
        );

        let base = spawn_mock(router).await;
        let bars = fetch_seed_at(&base, "sft_test", "XAUUSD", Interval::H1, end)
            .await
            .expect("clamped 1h seed");

        assert_eq!(calls.load(Ordering::SeqCst), 2, "422 then clamped retry");
        assert_eq!(bars.len(), 2, "forming 1h bucket must be excluded");
        assert_eq!(bars[0].time, closed_h1 - 3_600_000);
        assert_eq!(bars[1].time, closed_h1);
        assert!(bars.iter().all(|b| b.closed));
    }

    #[tokio::test]
    async fn desc_seed_stops_after_first_full_page_instead_of_walking_all_cursors() {
        let end = DateTime::parse_from_rfc3339("2026-10-26T18:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let newest_closed = end.timestamp_millis() - Interval::M15.millis();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_route = calls.clone();

        let router = Router::new().route(
            "/v1/hist/commodities/XAUUSD/bars",
            get(move |Query(params): Query<HashMap<String, String>>| {
                let calls = calls_for_route.clone();
                async move {
                    let call_num = calls.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(params.get("order").map(String::as_str), Some("desc"));
                    assert_eq!(call_num, 0, "must not request cursor page 2 once 2000 bars land");
                    let rows: Vec<serde_json::Value> = (0..SEED_BARS)
                        .map(|i| {
                            let t = newest_closed - (i as i64) * Interval::M15.millis();
                            serde_json::json!({
                                "t": t, "o": 3380.0, "h": 3381.0, "l": 3379.0, "c": 3380.5, "v": 50.0
                            })
                        })
                        .collect();
                    Json(serde_json::json!({
                        "data": rows,
                        "meta": {
                            "symbol": "XAUUSD",
                            "interval": "15m",
                            "next_cursor": "more_older_pages_unused"
                        }
                    }))
                }
            }),
        );

        let base = spawn_mock(router).await;
        let bars = fetch_seed_at(&base, "sft_test", "XAUUSD", Interval::M15, end)
            .await
            .expect("single-page 15m seed");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(bars.len(), SEED_BARS);
        assert!(bars.windows(2).all(|w| w[0].time < w[1].time));
    }

    #[tokio::test]
    async fn rate_limited_page_honors_retry_after_and_succeeds() {
        let end = DateTime::parse_from_rfc3339("2026-10-26T18:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let closed_t = end.timestamp_millis() - Interval::M15.millis();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_route = calls.clone();

        let router = Router::new().route(
            "/v1/hist/commodities/XAUUSD/bars",
            get(move || {
                let calls = calls_for_route.clone();
                async move {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        let mut resp: Response = (
                            StatusCode::TOO_MANY_REQUESTS,
                            Json(serde_json::json!({
                                "error": "rate_limit_exceeded",
                                "retry_after": 0
                            })),
                        )
                            .into_response();
                        resp.headers_mut()
                            .insert(RETRY_AFTER, HeaderValue::from_static("0"));
                        return resp;
                    }
                    Json(serde_json::json!({
                        "data": [
                            { "t": closed_t, "o": 3380.0, "h": 3382.0, "l": 3379.0, "c": 3381.0, "v": 100.0 }
                        ],
                        "meta": { "symbol": "XAUUSD", "interval": "15m" }
                    }))
                    .into_response()
                }
            }),
        );

        let base = spawn_mock(router).await;
        let bars = fetch_seed_at(&base, "sft_test", "XAUUSD", Interval::M15, end)
            .await
            .expect("retried 15m seed");

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].time, closed_t);
    }
}
