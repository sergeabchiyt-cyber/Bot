mod calendar;
mod config;
mod status;
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
use status::FeedStatus;
use types::{OrderflowEvent, VpCandle, WsFrame};
use volume_profile::VolumeProfileEngine;
use ws_server::AppState;

/// Deterministic 15m candle history covering the same fixed 2,000-bar span,
/// used only when `SEED_SYNTHETIC_CANDLES=1` and SiftingIO history failed.
/// Each session gets its own price band so the PS window is clearly distinct.
fn synthetic_history(now_ms: i64) -> Vec<VpCandle> {
    const FIFTEEN_MIN: i64 = 15 * 60 * 1000;
    let bars = sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT as i64;
    let start = now_ms - bars * FIFTEEN_MIN;
    let mut out = Vec::with_capacity(bars as usize);
    for i in 0..bars {
        let t = start + i * FIFTEEN_MIN;
        // A slow drift plus an intraday wave -> realistic-looking profile.
        let day = (i / 96) as f64;
        let phase = (i % 96) as f64 / 96.0 * std::f64::consts::TAU;
        let base = 3300.0 + day * 7.5 + phase.sin() * 12.0;
        let low = base - 1.5;
        let high = base + 1.5;
        out.push(VpCandle {
            time: t,
            open: base,
            high,
            low,
            close: base + 0.25,
            volume: 100.0 + ((i % 17) as f64) * 3.0,
            source: "synthetic".into(),
        });
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
    // Sifting is the sole live price/candle source. Binance remains on the
    // separate aggTrade channel below for order flow only.
    let (sifting_candle_tx, mut sifting_candle_rx) = mpsc::channel::<VpCandle>(256);
    // Closed-candle fan-out for the execution trigger.
    let (exec_tx, mut exec_rx) = mpsc::channel::<types::VpCandle>(64);

    let status = FeedStatus::new(bc_tx.clone());
    let vp = Arc::new(RwLock::new(VolumeProfileEngine::new()));
    let cached_levels = Arc::new(RwLock::new(Vec::new()));
    let cached_candles = Arc::new(RwLock::new(Vec::<VpCandle>::new()));
    let recent_bubbles: Arc<RwLock<Vec<OrderflowEvent>>> = Arc::new(RwLock::new(Vec::new()));
    let cached_calendar = Arc::new(RwLock::new(serde_json::json!({
        "source": "pending",
        "count": 0,
        "events": [],
    })));

    // ---------- Cold-start history ----------
    // SiftingIO is the only historical price source. The REST adapter rejects
    // any response other than exactly 2,000 15m candles, so Binance's futures
    // price series can never leak into the VP calculation.
    let mut seeded = false;
    if !config.sifting_api_key.is_empty() {
        match sifting_rest::fetch_sifting_klines_15m(
            &config.sifting_hist_url,
            &config.sifting_api_key,
            &config.sifting_symbol,
        )
        .await
        {
            Ok(candles) => {
                debug_assert_eq!(
                    candles.len(),
                    sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT
                );
                info!(
                    "Seeded exactly {} historical 15M candles from SiftingIO",
                    candles.len()
                );
                *cached_candles.write().await = candles.clone();
                let mut vp_w = vp.write().await;
                vp_w.ingest_candles(candles);
                let levels = vp_w.all_levels();
                drop(vp_w);
                *cached_levels.write().await = levels;
                seeded = true;
            }
            Err(e) => warn!(
                "SiftingIO REST history fetch failed; refusing Binance price fallback: {e}"
            ),
        }
    } else {
        warn!("No SIFTING_API_KEY set — historical VP seed is unavailable");
    }
    // CI / air-gapped fallback: upstream history is unavailable on hosted
    // runners, so allow deterministic synthetic candles only when explicitly
    // requested. This is never enabled by default and is not a Binance path.
    if !seeded && config.seed_synthetic_candles {
        let candles = synthetic_history(chrono::Utc::now().timestamp_millis());
        warn!(
            "No SiftingIO history available — seeding {} SYNTHETIC candles (SEED_SYNTHETIC_CANDLES=1)",
            candles.len()
        );
        *cached_candles.write().await = candles.clone();
        let mut vp_w = vp.write().await;
        vp_w.ingest_candles(candles);
        let levels = vp_w.all_levels();
        drop(vp_w);
        *cached_levels.write().await = levels;
        seeded = true;
    }
    if !seeded {
        warn!("Starting with an empty volume profile.");
    }
    for lvl in cached_levels.read().await.iter() {
        info!(
            "{}: poc={:.3} vah={:.3} val={:.3} direction={}",
            lvl.window, lvl.poc, lvl.vah, lvl.val, lvl.direction
        );
    }

    // ---------- Sifting spot stream (chart + remote source of truth) ----------
    if config.feed_sifting {
        let url = config.sifting_ws_url.clone();
        let key = config.sifting_api_key.clone();
        let symbol = config.sifting_symbol.clone();
        let bc = bc_tx.clone();
        let candle_tx = sifting_candle_tx.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) =
                sifting_ws::run_sifting_stream(url, key, symbol, bc, candle_tx, st).await
            {
                tracing::error!("Sifting stream terminated: {e}");
            }
        });
    } else {
        status.set("sifting", "off (no key)");
    }

    // ---------- Binance order flow (kept deliberately) ----------
    // Binance aggTrade is used for directional order flow only. It must not
    // feed candles or volume-profile prices, which all come from SiftingIO.
    if config.feed_binance {
        let url = config.binance_ws_url.clone();
        let tx = tick_tx.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = binance_ws::run_agg_trade_stream(url, tx, st).await {
                tracing::error!("aggTrade stream terminated: {e}");
            }
        });
    } else {
        status.set("binance", "off");
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

    // ---------- Sifting candle consumer (chart + VP + execution fan-out) ----------
    // SiftingIO broadcasts in-progress candles directly from the WS adapter;
    // completed candles arrive here and are the only live price candles that
    // can update the VP. No Binance kline/REST price path remains.
    {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let cached_c = cached_candles.clone();
        let bc = bc_tx.clone();
        let exec = exec_tx.clone();
        tokio::spawn(async move {
            while let Some(candle) = sifting_candle_rx.recv().await {
                {
                    let mut buf = cached_c.write().await;
                    buf.retain(|c| c.time != candle.time);
                    buf.push(candle.clone());
                    buf.sort_by_key(|c| c.time);
                    if buf.len() > sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT {
                        let drain = buf.len() - sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT;
                        buf.drain(0..drain);
                    }
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

    // ---------- Economic calendar ----------
    // Direct ForexFactory weekly feed (no browser needed); the browser MCP
    // server is only used as a fallback when the feed fails.
    {
        let source = Arc::new(calendar::CalendarSource::new(config.calendar_url.clone()));
        let mcp = if config.calendar_use_mcp {
            Some(Arc::new(mcp_client::McpClient::new(
                config.mcp_browser_url.clone(),
                config.mcp_browser_token.clone(),
            )))
        } else {
            None
        };
        let bc = bc_tx.clone();
        let st = status.clone();
        let cal_cache = cached_calendar.clone();
        let interval_secs = config.mcp_scrape_secs.max(60);
        st.set("calendar", "starting");
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                // Ticks immediately on the first pass, so the calendar is
                // populated at boot instead of after the first interval.
                interval.tick().await;
                match source.fetch(mcp.as_deref(), &st).await {
                    Ok((events, origin)) => {
                        let payload = serde_json::json!({
                            "source": origin,
                            "count": events.len(),
                            "updated": chrono::Utc::now().timestamp_millis(),
                            "events": events,
                        });
                        *cal_cache.write().await = payload.clone();
                        let _ = bc.send(WsFrame::Calendar { data: payload });
                    }
                    Err(e) => warn!("Calendar update failed: {e}"),
                }
            }
        });
    }

    // ---------- Session-close level refresh ----------
    // PS (previous session) must roll forward at every 17:00 New York close,
    // not stay frozen at whatever it was when the process booted. Candles
    // alone cannot be relied on to trigger this (quiet feed, weekend gap), so
    // a timer checks the boundary every 30s and recomputes when it moves.
    {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let now = chrono::Utc::now().timestamp_millis();
                let mut vp_w = vp.write().await;
                if !vp_w.refresh_on_session_close(now) {
                    continue;
                }
                let levels = vp_w.all_levels();
                let session_end = vp_w.ps_session_end();
                drop(vp_w);
                info!(
                    "Session close — levels refreshed (PS session ends {:?})",
                    session_end
                );
                *cached.write().await = levels.clone();
                for lvl in levels {
                    info!(
                        "{}: poc={:.3} vah={:.3} val={:.3}",
                        lvl.window, lvl.poc, lvl.vah, lvl.val
                    );
                    let _ = bc.send(WsFrame::Levels { data: lvl });
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
        cached_calendar: cached_calendar.clone(),
        vp: vp.clone(),
        status: status.clone(),
        config: config.clone(),
    };
    let app = ws_server::router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    info!(
        "Listening on 0.0.0.0:{} — /health /status /levels /candles /calendar /ws",
        config.port
    );
    axum::serve(listener, app).await?;

    Ok(())
}
