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
    // Closed-candle fan-out for the execution trigger.
    let (exec_tx, mut exec_rx) = mpsc::channel::<types::VpCandle>(64);

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

    // ---------- Sifting spot stream (Chart price only) ----------
    {
        let url = format!(
            "wss://stream.sifting.io/ws/v1?key={}",
            config.sifting_api_key
        );
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = sifting_ws::run_sifting_stream(url, bc).await {
                tracing::error!("Sifting stream terminated: {}", e);
            }
        });
    }

    // ---------- Exchange streams ----------
    {
        let url = config.binance_ws_url.clone();
        let tx = tick_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = binance_ws::run_agg_trade_stream(url, tx).await {
                tracing::error!("aggTrade stream terminated: {}", e);
            }
        });
    }

    {
        let url = config.binance_kline_url.clone();
        let tx = kline_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = binance_ws::run_kline_stream(url, tx).await {
                tracing::error!("kline stream terminated: {}", e);
            }
        });
    }

    {
        let tx = tick_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_bybit_stream(tx).await {
                tracing::error!("Bybit stream terminated: {}", e);
            }
        });
    }

    {
        let tx = tick_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_okx_stream(tx).await {
                tracing::error!("OKX stream terminated: {}", e);
            }
        });
    }

    // ---------- Tick consumer (order flow) ----------
    {
        let vp = vp.clone();
        let bc = bc_tx.clone();
        let recent = recent_bubbles.clone();
        let mut analyzer = OrderFlowAnalyzer::new(20_000);
        tokio::spawn(async move {
            while let Some(trade) = tick_rx.recv().await {
                analyzer.ingest(&trade);
                let vp_read = vp.read().await;
                let events = analyzer.detect_events(&vp_read, trade.price_f64(), 0.90, 0.97);
                drop(vp_read);
                for ev in events {
                    {
                        let mut buf = recent.write().await;
                        buf.push(ev.clone());
                        if buf.len() > 200 {
                            let drain = buf.len() - 200;
                            buf.drain(0..drain);
                        }
                    }
                    let _ = bc.send(WsFrame::Bubbles { data: ev });
                }
            }
        });
    }

    // ---------- Kline consumer (VP + execution fan-out) ----------
    // Binance klines feed the Volume Profile and Execution trigger.
    // WsFrame::Candle is broadcast by the Sifting stream for the chart.
    {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let bc = bc_tx.clone();
        let exec = exec_tx.clone();
        tokio::spawn(async move {
            while let Some(ev) = kline_rx.recv().await {
                if !ev.kline.is_closed {
                    continue;
                }
                let candle = ev.kline.to_vp_candle();
                let mut vp_w = vp.write().await;
                vp_w.ingest_candle(candle.clone());
                let levels = vp_w.all_levels();
                drop(vp_w);
                *cached.write().await = levels.clone();
                for lvl in levels {
                    let _ = bc.send(WsFrame::Levels { data: lvl });
                }
                let _ = exec.send(candle).await;
            }
        });
    }

    // ---------- MCP calendar scraper (every 15 min) ----------
    {
        let mcp = Arc::new(mcp_client::McpClient::new(
            config.mcp_browser_url.clone(),
            config.mcp_chelsea_url.clone().unwrap_or_default(),
        ));
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(900));
            loop {
                interval.tick().await;
                match mcp.scrape_calendar().await {
                    Ok(md) => {
                        let events = parse_calendar_markdown(&md);
                        info!("Calendar scraped: {} events", events.len());
                        let _ = bc.send(WsFrame::Calendar {
                            data: serde_json::json!({ "events": events }),
                        });
                    }
                    Err(e) => warn!("Calendar scrape failed: {}", e),
                }
            }
        });
    }

    // ---------- Execution trigger loop ----------
    {
        let levels = cached_levels.clone();
        let bubbles = recent_bubbles.clone();
        let exec_mgr = Arc::new(ExecutionManager::new(config.clone()));
        let proximity = config.level_proximity_pips;
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            const MIN_CONFIRM_STRENGTH: f64 = 15.0;
            while let Some(candle) = exec_rx.recv().await {
                let price = candle.close;
                let lvls = levels.read().await.clone();
                if !near_level(&lvls, price, proximity) {
                    continue;
                }
                let cutoff = candle.time;
                let candidates: Vec<OrderflowEvent> = {
                    let buf = bubbles.read().await;
                    buf.iter()
                        .filter(|b| b.timestamp >= cutoff && b.strength >= MIN_CONFIRM_STRENGTH)
                        .cloned()
                        .collect()
                };
                if candidates.is_empty() {
                    info!(
                        "Price {:.3} near level but no qualifying bubble — skipping",
                        price
                    );
                    continue;
                }
                let best = candidates
                    .iter()
                    .max_by(|a, b| a.strength.partial_cmp(&b.strength).unwrap())
                    .unwrap();
                let side = match best.kind.as_str() {
                    "BUY_BUBBLE" | "ABS_BUY" => "buy",
                    "SELL_BUBBLE" | "ABS_SELL" => "sell",
                    _ => continue,
                };
                info!(
                    "Execution trigger: {} @ {:.3} (strength {:.1}, kind {})",
                    side, price, best.strength, best.kind
                );
                match exec_mgr.execute(side, 0.01, price, &lvls[0]).await {
                    Ok(ev) => {
                        info!("Trade event: {:?}", ev);
                        let _ = bc.send(WsFrame::Trades { data: ev });
                    }
                    Err(e) => warn!("Execution failed: {}", e),
                }
            }
        });
    }

    // ---------- HTTP + WS server ----------
    let state = AppState {
        tx: bc_tx.clone(),
        subscriptions: Arc::new(dashmap::DashMap::new()),
        cached_levels: cached_levels.clone(),
        vp: vp.clone(),
    };
    let app = ws_server::router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    info!("Listening on 0.0.0.0:{}", config.port);
    axum::serve(listener, app).await?;

    Ok(())
}