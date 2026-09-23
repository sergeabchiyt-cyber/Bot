mod config;
mod types;
mod binance_ws;
mod sifting_rest;
mod multi_exchange;
mod volume_profile;
mod order_flow;
mod execution;
mod execution_deriv;
mod execution_chelsea;
mod mcp_client;
mod sifting_ws;
mod ws_server;

use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use config::Config;
use execution::ExecutionManager;
use order_flow::OrderFlowAnalyzer;
use types::{OrderflowEvent, WsFrame};
use volume_profile::VolumeProfileEngine;
use ws_server::AppState;

/// Parses the Forex Factory markdown table into structured events.
fn parse_calendar_markdown(md: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for line in md.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cols: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|s| s.trim())
            .collect();
        if cols.len() < 4 {
            continue;
        }
        if cols[0].eq_ignore_ascii_case("time") || cols[0].starts_with(':') {
            continue;
        }
        out.push(serde_json::json!({
            "time": cols[0],
            "currency": cols[1],
            "impact": cols[2],
            "event": cols[3],
        }));
    }
    out
}

/// Returns true if `price` is within `pips` of any PoC/VaH/VaL across all windows.
fn near_level(levels: &[types::VpLevels], price: f64, pips: f64) -> bool {
    let dollars = pips * 0.01;
    for lvl in levels {
        for target in [lvl.poc, lvl.vah, lvl.val] {
            if (price - target).abs() <= dollars {
                return true;
            }
        }
    }
    false
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    info!("Starting XAUUSD engine on port {}", config.port);

    let (bc_tx, _) = broadcast::channel(1024);
    let (tick_tx, mut tick_rx) = mpsc::channel(8192);
    let (kline_tx, mut kline_rx) = mpsc::channel(256);
    let (exec_tx, mut exec_rx) = mpsc::channel::<(types::VpLevels, f64)>(64);

    let vp = Arc::new(RwLock::new(VolumeProfileEngine::new()));
    let cached_levels = Arc::new(RwLock::new(Vec::new()));
    let recent_bubbles: Arc<RwLock<Vec<OrderflowEvent>>> = Arc::new(RwLock::new(Vec::new()));

    // ---------- Cold-start REST fetch (Sifting.io spot) ----------
    match sifting_rest::fetch_sifting_klines_15m(&config.sifting_api_key, "XAUUSD", 1000).await {
        Ok(candles) => {
            info!("Seeded {} historical 15M candles from Sifting.io", candles.len());
            let mut vp_w = vp.write().await;
            for c in candles {
                vp_w.ingest_candle(c);
            }
            let levels = vp_w.all_levels();
            for lvl in &levels {
                info!(
                    "{}: poc={:.3} vah={:.3} val={:.3}",
                    lvl.window, lvl.poc, lvl.vah, lvl.val
                );
            }
            *cached_levels.write().await = levels;
        }
        Err(e) => warn!("Cold-start Sifting REST fetch failed: {}. Continuing.", e),
    }

    // ... (rest of main.rs remains exactly as before) ...
    // The full file continues with the Sifting spot WebSocket stream,
    // Binance WebSocket streams, order flow analysis, execution manager,
    // and the Axum WebSocket server setup.

    Ok(())
}