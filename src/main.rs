mod config;
mod status;
mod types;
mod binance_rest;
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
use status::FeedStatus;
use types::{OrderflowEvent, VpCandle, WsFrame};
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
    info!("Starting XAUUSD engine v{} on port {}", env!("CARGO_PKG_VERSION"), config.port);

    let (bc_tx, _) = broadcast::channel(4096);
    let (tick_tx, mut tick_rx) = mpsc::channel(8192);
    let (kline_tx, mut kline_rx) = mpsc::channel(256);
    // Closed-candle fan-out for the execution trigger.
    let (exec_tx, mut exec_rx) = mpsc::channel::<types::VpCandle>(64);

    let status = FeedStatus::new(bc_tx.clone());
    let vp = Arc::new(RwLock::new(VolumeProfileEngine::new()));
    let cached_levels = Arc::new(RwLock::new(Vec::new()));
    let cached_candles = Arc::new(RwLock::new(Vec::<VpCandle>::new()));
    let recent_bubbles: Arc<RwLock<Vec<OrderflowEvent>>> = Arc::new(RwLock::new(Vec::new()));

    // ---------- Cold-start history ----------
    // Primary: Sifting.io commodities bars. Fallback: Binance futures klines,
    // so the volume profile seeds even without a Sifting key.
    let mut seeded = false;
    if !config.sifting_api_key.is_empty() {
        match sifting_rest::fetch_sifting_klines_15m(
            &config.sifting_hist_url,
            &config.sifting_api_key,
            &config.sifting_symbol,
            1000,
        )
        .await
        {
            Ok(candles) => {
                info!("Seeded {} historical 15M candles from Sifting.io", candles.len());
                let mut vp_w = vp.write().await;
                for c in candles {
                    vp_w.ingest_candle(c);
                }
                let levels = vp_w.all_levels();
                drop(vp_w);
                *cached_levels.write().await = levels;
                seeded = true;
            }
            Err(e) => warn!("Cold-start Sifting REST fetch failed: {e}. Trying Binance."),
        }
    } else {
        info!("No SIFTING_API_KEY set — seeding history from Binance instead");
    }
    if !seeded {
        match binance_rest::fetch_klines_15m("XAUUSDT", 1000).await {
            Ok(candles) => {
                info!("Seeded {} historical 15M candles from Binance", candles.len());
                let mut vp_w = vp.write().await;
                for c in &candles {
                    vp_w.ingest_candle(c.clone());
                }
                let levels = vp_w.all_levels();
                drop(vp_w);
                *cached_levels.write().await = levels;
                *cached_candles.write().await = candles;
            }
            Err(e) => warn!("Cold-start Binance REST fetch failed: {e}. Starting empty."),
        }
    }
    // Chart cache is Binance-only even if VP was seeded from Sifting.
    if cached_candles.read().await.is_empty() {
        match binance_rest::fetch_klines_15m("XAUUSDT", 500).await {
            Ok(candles) => {
                info!("Cached {} Binance 15M candles for chart replay", candles.len());
                *cached_candles.write().await = candles;
            }
            Err(e) => warn!("Binance chart seed failed: {e}"),
        }
    }
    for lvl in cached_levels.read().await.iter() {
        info!("{}: poc={:.3} vah={:.3} val={:.3}", lvl.window, lvl.poc, lvl.vah, lvl.val);
    }

    // ---------- Sifting spot stream (chart + remote source of truth) ----------
    if config.feed_sifting {
        let url = config.sifting_ws_url.clone();
        let key = config.sifting_api_key.clone();
        let symbol = config.sifting_symbol.clone();
        let bc = bc_tx.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = sifting_ws::run_sifting_stream(url, key, symbol, bc, st).await {
                tracing::error!("Sifting stream terminated: {e}");
            }
        });
    } else {
        status.set("sifting", "off (no key)");
    }

    // ---------- Binance chart + order flow streams ----------
    if config.feed_binance {
        {
            let url = config.binance_ws_url.clone();
            let tx = tick_tx.clone();
            let st = status.clone();
            tokio::spawn(async move {
                if let Err(e) = binance_ws::run_agg_trade_stream(url, tx, st).await {
                    tracing::error!("aggTrade stream terminated: {e}");
                }
            });
        }
        {
            let url = config.binance_kline_url.clone();
            let tx = kline_tx.clone();
            let st = status.clone();
            tokio::spawn(async move {
                if let Err(e) = binance_ws::run_kline_stream(url, tx, st).await {
                    tracing::error!("kline stream terminated: {e}");
                }
            });
        }
    } else {
        status.set("binance", "off");
        status.set("binance_kline", "off");
    }

    // ---------- Additional order flow sources ----------
    if config.feed_bybit {
        let tx = tick_tx.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_bybit_stream(tx, st).await {
                tracing::error!("Bybit stream terminated: {e}");
            }
        });
    } else {
        status.set("bybit", "off");
    }

    if config.feed_okx {
        let tx = tick_tx.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_okx_stream(tx, st).await {
                tracing::error!("OKX stream terminated: {e}");
            }
        });
    } else {
        status.set("okx", "off");
    }

    if config.feed_bitget {
        let tx = tick_tx.clone();
        let symbol = config.bitget_symbol.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_bitget_stream(tx, symbol, st).await {
                tracing::error!("Bitget stream terminated: {e}");
            }
        });
    } else {
        status.set("bitget", "off");
    }

    if config.feed_gate {
        let tx = tick_tx.clone();
        let symbol = config.gate_symbol.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_gate_stream(tx, symbol, st).await {
                tracing::error!("Gate stream terminated: {e}");
            }
        });
    } else {
        status.set("gate", "off");
    }

    if config.feed_kraken {
        let tx = tick_tx.clone();
        let product = config.kraken_product.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_kraken_stream(tx, product, st).await {
                tracing::error!("Kraken stream terminated: {e}");
            }
        });
    } else {
        status.set("kraken", "off");
    }

    if config.feed_alltick {
        let tx = tick_tx.clone();
        let url = config.alltick_ws_url.clone();
        let code = config.alltick_code.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_alltick_stream(tx, url, code, st).await {
                tracing::error!("AllTick stream terminated: {e}");
            }
        });
    } else {
        status.set("alltick", "off (no token)");
    }

    if config.feed_itick {
        let tx = tick_tx.clone();
        let url = config.itick_ws_url.clone();
        let symbol = config.itick_symbol.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = multi_exchange::run_itick_stream(tx, url, symbol, st).await {
                tracing::error!("iTick stream terminated: {e}");
            }
        });
    } else {
        status.set("itick", "off (no token)");
    }

    // ---------- Tick consumer (order flow) ----------
    {
        let vp = vp.clone();
        let bc = bc_tx.clone();
        let recent = recent_bubbles.clone();
        let st = status.clone();
        let mut analyzer = OrderFlowAnalyzer::new(20_000);
        tokio::spawn(async move {
            while let Some(trade) = tick_rx.recv().await {
                st.mark_msg("orderflow");
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

    // ---------- Kline consumer (chart broadcast + VP + execution fan-out) ----------
    // Every kline update streams to the chart (Binance is the chart source);
    // closed candles feed the Volume Profile and the execution trigger.
    {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let cached_c = cached_candles.clone();
        let bc = bc_tx.clone();
        let exec = exec_tx.clone();
        tokio::spawn(async move {
            while let Some(ev) = kline_rx.recv().await {
                let candle = ev.kline.to_vp_candle("binance");
                // Live chart update (in-progress candles included).
                {
                    let mut buf = cached_c.write().await;
                    buf.retain(|c| c.time != candle.time);
                    buf.push(candle.clone());
                    buf.sort_by_key(|c| c.time);
                    if buf.len() > 500 {
                        let drain = buf.len() - 500;
                        buf.drain(0..drain);
                    }
                }
                let _ = bc.send(WsFrame::Candle { data: candle.clone() });

                if !ev.kline.is_closed {
                    continue;
                }
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

    // ---------- MCP browser calendar scrape ----------
    {
        let mcp = Arc::new(mcp_client::McpClient::new(
            config.mcp_browser_url.clone(),
            config.mcp_browser_token.clone(),
        ));
        let bc = bc_tx.clone();
        let st = status.clone();
        let interval_secs = config.mcp_scrape_secs.max(60);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                match mcp.scrape_calendar(&st).await {
                    Ok(md) => {
                        let events = parse_calendar_markdown(&md);
                        info!("Calendar scraped: {} events", events.len());
                        let _ = bc.send(WsFrame::Calendar {
                            data: serde_json::json!({ "events": events }),
                        });
                    }
                    Err(e) => warn!("Calendar scrape failed: {e}"),
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
                if lvls.is_empty() || !near_level(&lvls, price, proximity) {
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
                match exec_mgr
                    .execute(side, 0.01, price, &lvls[0])
                    .await
                {
                    Ok(ev) => {
                        info!("Trade event: {:?}", ev);
                        let _ = bc.send(WsFrame::Trades { data: ev });
                    }
                    Err(e) => warn!("Execution failed: {e}"),
                }
            }
        });
    }

    // ---------- HTTP + WS server ----------
    let state = AppState {
        tx: bc_tx.clone(),
        subscriptions: Arc::new(dashmap::DashMap::new()),
        cached_levels: cached_levels.clone(),
        cached_candles: cached_candles.clone(),
        vp: vp.clone(),
        status: status.clone(),
        config: config.clone(),
    };
    let app = ws_server::router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    info!("Listening on 0.0.0.0:{} — dashboard at /", config.port);
    axum::serve(listener, app).await?;

    Ok(())
}
