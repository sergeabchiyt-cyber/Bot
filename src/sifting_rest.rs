use anyhow::{Context, Result};
use chrono::{Duration, SecondsFormat, Utc};
use serde::Deserialize;

use crate::types::VpCandle;

/// The VP seed is deliberately fixed to the largest page SiftingIO permits.
/// Keeping this as a constant prevents a caller from silently mixing a shorter
/// history (or a different provider) into the profile.
pub const SIFTING_HISTORY_CANDLE_LIMIT: usize = 2_000;
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
    _next_cursor: Option<String>,
}

/// The top-level response from the SiftingIO historical bars endpoint.
#[derive(Debug, Deserialize)]
struct SiftingBarsResponse {
    data: Vec<SiftingBar>,
    #[serde(default)]
    meta: SiftingBarsMeta,
}

/// Build the one-page request used for the VP seed.
///
/// SiftingIO requires `start` on the first historical request. The 60-day
/// window is intentionally wider than 2,000 15-minute buckets so that a
/// near-24/7 commodity feed can still return the newest *exactly* 2,000 bars
/// when it has weekend/session gaps. `order=desc` makes those the latest bars.
fn history_url(base_url: &str, symbol: &str, now: chrono::DateTime<Utc>) -> String {
    let start = now - Duration::days(HISTORY_LOOKBACK_DAYS);
    let start = start.to_rfc3339_opts(SecondsFormat::Secs, true);
    let end = now.to_rfc3339_opts(SecondsFormat::Secs, true);

    format!(
        "{}/v1/hist/commodities/{}/bars?start={}&end={}&interval=15m&order=desc&limit={}",
        base_url.trim_end_matches('/'),
        symbol,
        start,
        end,
        SIFTING_HISTORY_CANDLE_LIMIT
    )
}

fn parse_response(
    response: SiftingBarsResponse,
    symbol: &str,
) -> Result<Vec<VpCandle>> {
    if response.data.len() != SIFTING_HISTORY_CANDLE_LIMIT {
        anyhow::bail!(
            "SiftingIO returned {} candles; VP requires exactly {}",
            response.data.len(),
            SIFTING_HISTORY_CANDLE_LIMIT
        );
    }

    if let Some(response_symbol) = response.meta.symbol.as_deref() {
        if !response_symbol.eq_ignore_ascii_case(symbol) {
            anyhow::bail!(
                "SiftingIO returned symbol {response_symbol}, expected {symbol}"
            );
        }
    }
    if let Some(interval) = response.meta.interval.as_deref() {
        if interval != "15m" {
            anyhow::bail!("SiftingIO returned interval {interval}, expected 15m");
        }
    }
    // A cursor only means that older history exists. We intentionally do not
    // follow it: this seed is one page and never more than 2,000 candles.
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

    Ok(candles)
}

/// Fetch exactly 2,000 latest 15-minute candles for `symbol` from SiftingIO
/// spot commodities (XAUUSD streams under the `com` product).
///
/// The endpoint's `limit` is hard-coded to 2,000 and a short page is an error;
/// there is intentionally no Binance REST fallback. Binance remains the
/// order-flow source, while SiftingIO is the sole historical price source for
/// the VP so the two price domains cannot be mixed.
pub async fn fetch_sifting_klines_15m(
    base_url: &str,
    api_key: &str,
    symbol: &str,
) -> Result<Vec<VpCandle>> {
    let url = history_url(base_url, symbol, Utc::now());
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
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

    let parsed: SiftingBarsResponse = resp
        .json()
        .await
        .context("failed to parse SiftingIO bars response")?;

    parse_response(parsed, symbol)
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
    fn history_request_is_a_single_fixed_two_thousand_bar_page() {
        let now = Utc::now();
        let url = history_url("https://api.sifting.io/", "XAUUSD", now);
        assert!(url.starts_with("https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?"));
        assert!(url.contains("interval=15m"));
        assert!(url.contains("order=desc"));
        assert!(url.contains("limit=2000"));
        assert!(url.contains("start="));
        assert!(url.contains("end="));
    }

    #[test]
    fn a_short_response_is_rejected_instead_of_seeding_a_partial_profile() {
        let response = SiftingBarsResponse {
            data: Vec::new(),
            meta: SiftingBarsMeta::default(),
        };
        let err = parse_response(response, "XAUUSD").unwrap_err().to_string();
        assert!(err.contains("exactly 2000"));
    }
}
