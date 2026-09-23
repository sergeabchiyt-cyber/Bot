use anyhow::{Context, Result};
use serde::Deserialize;
use crate::types::VpCandle;

/// A single OHLC bar from the Sifting.io commodities historical endpoint.
/// The API returns bars with `t` (timestamp ms), `o`, `h`, `l`, `c` fields.
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
    #[serde(rename = "v", default)]
    volume: f64,
}

/// The top-level response from the Sifting.io historical bars endpoint.
#[derive(Debug, Deserialize)]
struct SiftingBarsResponse {
    data: Vec<SiftingBar>,
}

/// Fetch the last `limit` 15-minute candles for `symbol` from Sifting.io
/// spot commodities (XAUUSD streams under the `com` product).
///
/// # Arguments
/// * `base_url` - Sifting.io API base (default `https://api.sifting.io`).
/// * `api_key`  - Your Sifting.io API key (starts with `sft_`).
/// * `symbol`   - The commodity, e.g. `"XAUUSD"`.
/// * `limit`    - Maximum number of bars to return.
pub async fn fetch_sifting_klines_15m(
    base_url: &str,
    api_key: &str,
    symbol: &str,
    limit: usize,
) -> Result<Vec<VpCandle>> {
    // Historical bars endpoint per docs: /v1/hist/commodities/{symbol}/bars
    let url = format!(
        "{}/v1/hist/commodities/{}/bars?interval=15m&limit={}",
        base_url.trim_end_matches('/'), symbol, limit
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("X-API-Key", api_key)
        .header("Accept-Encoding", "gzip") // bars endpoints require gzip
        .send()
        .await
        .context("Failed to send request to Sifting.io")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Sifting.io API returned {status}: {}", body);
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
            source: "sifting".into(),
        })
        .collect();

    Ok(candles)
}
