use anyhow::Result;
use crate::types::VpCandle;

/// Fetch the last `limit` 15-minute candles for `symbol` from Binance Futures.
/// Cold-start fallback when the Sifting.io history fetch is unavailable,
/// so the volume profile still seeds on a bare deployment.
pub async fn fetch_klines_15m(symbol: &str, limit: usize) -> Result<Vec<VpCandle>> {
    let url = format!(
        "https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval=15m&limit={limit}"
    );
    let resp = reqwest::get(&url).await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Binance klines REST returned {status}: {}", truncate(&body, 200));
    }
    let raw: Vec<Vec<serde_json::Value>> = resp.json().await?;

    let candles = raw
        .into_iter()
        .filter_map(|k| {
            Some(VpCandle {
                time: k.first()?.as_i64()?,
                open: k.get(1)?.as_str()?.parse().ok()?,
                high: k.get(2)?.as_str()?.parse().ok()?,
                low: k.get(3)?.as_str()?.parse().ok()?,
                close: k.get(4)?.as_str()?.parse().ok()?,
                volume: k.get(5)?.as_str()?.parse().ok()?,
                source: "binance".into(),
            })
        })
        .collect();

    Ok(candles)
}

fn truncate(s: &str, n: usize) -> &str {
    match s.get(..n) {
        Some(prefix) => prefix,
        None => s,
    }
}
