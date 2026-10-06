use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};
use serde::Deserialize;

use crate::types::VpCandle;

/// The chart/swing seed remains exactly the largest SiftingIO 15m page.
pub const SIFTING_HISTORY_CANDLE_LIMIT: usize = 2_000;
const PROFILE_HISTORY_PAGE_LIMIT: usize = 2_000;
/// One ascending page is enough to close a restart hole: 500 x 15m bars is
/// over five days of buckets, and a longer outage is reported instead.
const SIFTING_TAIL_CANDLE_LIMIT: usize = 500;
const MAX_PROFILE_HISTORY_PAGES: usize = 32;
const HISTORY_LOOKBACK_DAYS: i64 = 60;

/// A single OHLC bar from SiftingIO's commodities historical endpoint.
/// The API returns Unix epoch milliseconds in `t` and numeric OHLCV fields.
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
    expected_interval: &str,
) -> Result<(Vec<VpCandle>, Option<String>)> {
    if let Some(response_symbol) = response.meta.symbol.as_deref() {
        if !response_symbol.eq_ignore_ascii_case(symbol) {
            anyhow::bail!(
                "SiftingIO returned symbol {response_symbol}, expected {symbol}"
            );
        }
    }
    if let Some(interval) = response.meta.interval.as_deref() {
        if interval != expected_interval {
            anyhow::bail!(
                "SiftingIO returned interval {interval}, expected {expected_interval}"
            );
        }
    }

    let mut candles: Vec<VpCandle> = response
        .data
        .into_iter()
        .map(|bar| VpCandle {
            time: bar.time,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
            source: "sifting".into(),
        })
        .collect();

    if candles.iter().any(|c| {
        !c.open.is_finite()
            || !c.high.is_finite()
            || !c.low.is_finite()
            || !c.close.is_finite()
            || !c.volume.is_finite()
            || c.volume < 0.0
            || c.high < c.low
            || c.time <= 0
    }) {
        anyhow::bail!("SiftingIO returned an invalid OHLCV bar");
    }

    candles.sort_by_key(|c| c.time);
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
    interval: &str,
) -> Result<(Vec<VpCandle>, Option<String>)> {
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

/// Fetch exactly 2,000 latest 15-minute candles for the chart and swing seed.
///
/// The 15m page is deliberately kept for the chart's existing contract and
/// for the swing detector. Time-window profiles use the dedicated 1m fetch
/// below, which pages over the precise history they need.
pub async fn fetch_sifting_klines_15m(
    base_url: &str,
    api_key: &str,
    symbol: &str,
) -> Result<Vec<VpCandle>> {
    let now = Utc::now();
    let url = history_url(
        base_url,
        symbol,
        now - Duration::days(HISTORY_LOOKBACK_DAYS),
        now,
        "15m",
        "desc",
        SIFTING_HISTORY_CANDLE_LIMIT,
        None,
    )?;
    let client = reqwest::Client::new();
    let (candles, _) = get_page(&client, url, api_key, symbol, "15m").await?;
    require_chart_seed(candles)
}

/// Fetch the 15m bars newer than `after_ms`, ascending.
///
/// SiftingIO's REST history can lag its own live stream — on the 2026-10 probe
/// the newest seeded 15m bar was fifteen *hours* behind the live bucket — and
/// the live stream only ever appends from "now". Without this tail the chart
/// would boot with a hole between the seed's edge and the first live close,
/// and nothing would ever fill it.
pub async fn fetch_sifting_klines_15m_since(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    after_ms: i64,
) -> Result<Vec<VpCandle>> {
    let now = Utc::now();
    let start = DateTime::<Utc>::from_timestamp_millis(after_ms + 1)
        .context("invalid 15m history tail timestamp")?;
    if start >= now {
        return Ok(Vec::new());
    }
    let url = history_url(
        base_url,
        symbol,
        start,
        now,
        "15m",
        "asc",
        SIFTING_TAIL_CANDLE_LIMIT,
        None,
    )?;
    let client = reqwest::Client::new();
    let (candles, _) = get_page(&client, url, api_key, symbol, "15m").await?;
    Ok(newer_than(candles, after_ms))
}

/// The strictly-newer bars. SiftingIO's `start` is inclusive, so the page can
/// repeat the boundary bar the caller already has.
fn newer_than(mut candles: Vec<VpCandle>, after_ms: i64) -> Vec<VpCandle> {
    candles.retain(|candle| candle.time > after_ms);
    candles
}

fn require_chart_seed(candles: Vec<VpCandle>) -> Result<Vec<VpCandle>> {
    if candles.len() != SIFTING_HISTORY_CANDLE_LIMIT {
        anyhow::bail!(
            "SiftingIO returned {} 15m candles; chart seed requires exactly {}",
            candles.len(),
            SIFTING_HISTORY_CANDLE_LIMIT
        );
    }
    Ok(candles)
}

/// Fetch fine candles for the time-window volume profiles at `interval`
/// (`"1m"`, `"5m"`, ...), following SiftingIO's cursor across pages.
/// `end_exclusive_ms` is exclusive, matching the profile engine's candle-window
/// semantics; the REST endpoint's end parameter is inclusive, so the request
/// ends one millisecond earlier.
pub async fn fetch_sifting_profile_candles(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    start_ms: i64,
    end_exclusive_ms: i64,
    interval: &str,
) -> Result<Vec<VpCandle>> {
    if end_exclusive_ms <= start_ms {
        anyhow::bail!("invalid 1m profile history bounds");
    }
    let start = Utc
        .timestamp_millis_opt(start_ms)
        .single()
        .context("invalid 1m profile start timestamp")?;
    let end_ms = end_exclusive_ms - 1;
    let end = Utc
        .timestamp_millis_opt(end_ms)
        .single()
        .context("invalid 1m profile end timestamp")?;

    let client = reqwest::Client::new();
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();

    for page_index in 0..MAX_PROFILE_HISTORY_PAGES {
        let url = history_url(
            base_url,
            symbol,
            start.clone(),
            end.clone(),
            interval,
            "asc",
            PROFILE_HISTORY_PAGE_LIMIT,
            cursor.as_deref(),
        )?;
        let (mut page, next_cursor) = get_page(&client, url, api_key, symbol, interval).await?;
        all.append(&mut page);

        match next_cursor.filter(|next| !next.is_empty()) {
            Some(next) => {
                if !seen_cursors.insert(next.clone()) {
                    anyhow::bail!("SiftingIO repeated a 1m profile-history cursor");
                }
                if page_index + 1 == MAX_PROFILE_HISTORY_PAGES {
                    anyhow::bail!(
                        "SiftingIO 1m profile history exceeded {MAX_PROFILE_HISTORY_PAGES} pages"
                    );
                }
                cursor = Some(next);
            }
            None => break,
        }
    }

    all.retain(|c| c.time >= start_ms && c.time < end_exclusive_ms);
    all.sort_by_key(|c| c.time);
    all.dedup_by_key(|c| c.time);
    Ok(all)
}

/// Minimum share of the window's buckets a profile page must contain to be
/// trusted. A plan- or cursor-limited response can return the right first and
/// last timestamps while covering only a fraction of the week, which would
/// silently smear the profile. Real gold history covers ~23h a day, 5 days a
/// week, so a quarter of the raw bucket count is a conservative floor.
const MIN_PROFILE_HISTORY_DENSITY: f64 = 0.25;

/// True when `candles` can honestly represent `[start_ms, end_ms)` at
/// `interval_ms`: bars must reach both boundaries (within
/// `boundary_tolerance_ms`, so a weekend/holiday gap is fine) *and* the bar
/// count must be a plausible share of the buckets the window spans.
pub fn profile_history_covers(
    candles: &[VpCandle],
    start_ms: i64,
    end_ms: i64,
    interval_ms: i64,
    boundary_tolerance_ms: i64,
) -> bool {
    if candles.is_empty() || end_ms <= start_ms || interval_ms <= 0 {
        return false;
    }
    let reaches_start = candles
        .first()
        .is_some_and(|first| first.time <= start_ms + boundary_tolerance_ms);
    let reaches_end = candles
        .last()
        .is_some_and(|last| last.time >= end_ms - boundary_tolerance_ms);
    if !reaches_start || !reaches_end {
        return false;
    }
    let span_buckets = (end_ms - start_ms) as f64 / interval_ms as f64;
    let density = candles.len() as f64 / span_buckets.max(1.0);
    density >= MIN_PROFILE_HISTORY_DENSITY
}

fn truncate(s: &str, n: usize) -> &str {
    match s.get(..n) {
        Some(prefix) => prefix,
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_history_request_is_a_single_fixed_two_thousand_bar_page() {
        let now = Utc::now();
        let url = history_url(
            "https://api.sifting.io/",
            "XAUUSD",
            now - Duration::days(HISTORY_LOOKBACK_DAYS),
            now,
            "15m",
            "desc",
            SIFTING_HISTORY_CANDLE_LIMIT,
            None,
        )
        .unwrap();
        assert!(url
            .as_str()
            .starts_with("https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?"));
        assert!(url.as_str().contains("interval=15m"));
        assert!(url.as_str().contains("order=desc"));
        assert!(url.as_str().contains("limit=2000"));
        assert!(url.as_str().contains("start="));
        assert!(url.as_str().contains("end="));
    }

    #[test]
    fn profile_history_request_carries_interval_and_encoded_cursor() {
        let now = Utc::now();
        let url = history_url(
            "https://api.sifting.io",
            "XAUUSD",
            now - Duration::days(7),
            now,
            "1m",
            "asc",
            PROFILE_HISTORY_PAGE_LIMIT,
            Some("cursor/with+reserved=chars"),
        )
        .unwrap();
        assert!(url.as_str().contains("interval=1m"));
        assert!(url.as_str().contains("order=asc"));
        assert!(url.as_str().contains("limit=2000"));
        assert!(url.as_str().contains("cursor=cursor%2Fwith%2Breserved%3Dchars"));
    }

    #[test]
    fn history_tail_drops_the_boundary_bar_and_keeps_the_newer_ones() {
        let edge = 1_700_000_000_000;
        let candles = vec![bar(edge), bar(edge + 15 * 60_000), bar(edge + 30 * 60_000)];
        let tail = newer_than(candles, edge);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].time, edge + 15 * 60_000);
        assert_eq!(tail[1].time, edge + 30 * 60_000);
    }

    fn bar(time: i64) -> VpCandle {
        VpCandle {
            time,
            open: 4100.0,
            high: 4101.0,
            low: 4099.0,
            close: 4100.5,
            volume: 1.0,
            source: "sifting".into(),
        }
    }

    /// A dense minute series covering the whole window is accepted. A gold
    /// week is ~119 trading hours inside a 120-hour Sunday-18:00 → Friday-18:00
    /// window (the last hour is the daily 17:00 NY break).
    #[test]
    fn complete_profile_history_passes_the_density_check() {
        let start = 1_700_000_000_000;
        let week: i64 = 5 * 86_400_000;
        let candles: Vec<VpCandle> = (0..(119 * 60)).map(|i| bar(start + i * 60_000)).collect();
        assert!(profile_history_covers(&candles, start, start + week, 60_000, 2 * 3_600_000));
    }

    /// ...while a page that only *reaches* the ends but is mostly empty fails.
    #[test]
    fn a_sparse_profile_page_is_rejected_even_when_it_reaches_both_ends() {
        let start = 1_700_000_000_000;
        let week: i64 = 5 * 86_400_000;
        // One bar every 30 minutes right across the window: the endpoints look
        // fine, the coverage does not.
        let candles: Vec<VpCandle> = (0..(week / 1_800_000) - 1)
            .map(|i| bar(start + i * 1_800_000))
            .collect();
        assert!(!profile_history_covers(&candles, start, start + week, 60_000, 2 * 3_600_000));
        // Nothing at all, or nothing near the far end, is rejected too.
        assert!(!profile_history_covers(&[], start, start + week, 60_000, 0));
        assert!(!profile_history_covers(&candles, start, start + 2 * week, 60_000, 0));
    }

    #[test]
    fn a_short_chart_response_is_rejected_instead_of_seeding_a_partial_page() {
        let response: SiftingBarsResponse = serde_json::from_value(serde_json::json!({
            "data": [],
            "meta": {"symbol":"XAUUSD", "interval":"15m"}
        }))
        .unwrap();
        let (candles, _) = parse_page(response, "XAUUSD", "15m").unwrap();
        assert!(require_chart_seed(candles).is_err());
    }

    #[test]
    fn response_validation_rejects_wrong_symbol_or_interval() {
        let response: SiftingBarsResponse = serde_json::from_value(serde_json::json!({
            "data": [],
            "meta": {"symbol":"XAGUSD", "interval":"1m"}
        }))
        .unwrap();
        assert!(parse_page(response, "XAUUSD", "1m").is_err());
    }
}
