use anyhow::{Context, Result};
use serde::Deserialize;
use crate::types::VpCandle;

/// The Sifting.io API base URL for historical data.
const SIFTING_BASE_URL: &str = "https://api.sifting.io";

/// A single OHLC bar from the Sifting.io forex historical endpoint.
/// The API returns bars with a `t` (timestamp in milliseconds), `o`, `h`, `l`, `c` fields.
#[derive(Debug, Deserialize)]
struct SiftingBar {
    /// Timestamp in milliseconds
    #[serde(rename = "t")]
    time: i64,
    /// Open price
    #[serde(rename = "o")]
    open: f64,
    /// High price
    #[serde(rename = "h")]
    high: f64,
    /// Low price
    #[serde(rename = "l")]
    low: f64,
    /// Close price
    #[serde(rename = "c")]
    close: f64,
    /// Volume (always 0 for OTC spot forex)
    #[serde(rename = "v", default)]
    volume: f64,
}

/// The top-level response from the Sifting.io historical bars endpoint.
#[derive(Debug, Deserialize)]
struct SiftingBarsResponse {
    data: Vec<SiftingBar>,
}

/// Fetch the last `limit` 15-minute candles for `symbol` from Sifting.io spot forex.
///
/// # Arguments
/// * `api_key` - Your Sifting.io API key (starts with `sft_`).
/// * `symbol`  - The forex pair, e.g. `"XAUUSD"`.
/// * `limit`   - Maximum number of bars to return.
pub async fn fetch_sifting_klines_15m(
    api_key: &str,
    symbol: &str,
    limit: usize,
) -> Result<Vec<VpCandle>> {
    // The Sifting.io forex historical endpoint: /v1/hist/forex/bars
    let url = format!(
        "{}/v1/hist/forex/bars?pair={}&interval=15m&limit={}",
        SIFTING_BASE_URL, symbol, limit
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("X-API-Key", api_key)
        .header("Accept-Encoding", "gzip") // Required to avoid 406 on heavy endpoints
        .send()
        .await
        .context("Failed to send request to Sifting.io")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Sifting.io API returned {}: {}", status, body);
    }

    let parsed: SiftingBarsResponse = resp
        .json()
        .await
        .context("Failed to parse Sifting.io bars response")?;

    let candles = parsed
        .data
        .into_iter()
        .map(|bar| VpCandle {
            time: bar.time,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .collect();

    Ok(candles)
}