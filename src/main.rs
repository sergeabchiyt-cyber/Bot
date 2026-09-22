mod config;
mod types;
mod binance_ws;
mod binance_rest;
mod multi_exchange;
mod volume_profile;
mod order_flow;
mod execution;
mod execution_deriv;
mod execution_chelsea;
mod mcp_client;
mod ws_server;

use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::info;
use tracing_subscriber::EnvFilter;

use config::Config;
use order_flow::OrderFlowAnalyzer;
use types::WsFrame;
use volume_profile::VolumeProfileEngine;
use ws_server::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    info!("Starting XAUUSD engine on port {}", config.port);

    let (bc_tx, _) = broadcast::channel::<WsFrame>(1024);
    let (tick_tx, mut tick_rx) = mpsc::channel::<types::AggTrade>(8192);
    let (kline_tx, mut kline_rx) = mpsc::channel::<types::KlineEvent>(256);

    let vp = Arc::new(RwLock::new(VolumeProfileEngine::new()));
    let cached_levels = Arc::new(RwLock::new(Vec::<types::VpLevels>::new()));

    // ---------- Cold start: seed VP windows from REST ----------
    match binance_rest::fetch_klines_15m("XAUUSDT", 1000).await {
        Ok(candles) => {
            info!("Seeded {} historical 15M candles", candles.len());
            let mut vp_w = vp.write().await;
            for c in candles {
                vp_w.ingest_candle(c);
            }
            let levels = vp_w.all_levels();
            for lvl in &levels {
                info!("{}: poc={:.3} vah={:.3} val={:.3}",
                    lvl.window, lvl.poc, lvl.vah, lvl.val);
            }
            *cached_levels.write().await = levels;
        }
        Err(e) => tracing::warn!("Cold-start REST fetch failed: {}. Continuing without history.", e),
    }

    // ---------- Binance aggTrade + kline ----------
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

    // ---------- Multi-exchange order flow (Bybit + OKX) ----------
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

    // ---------- Tick consumer: order flow + bubble detection ----------
    // Threshold raised from 5.0 to 15.0 now that three exchanges feed the buffer.
    {
        let vp = vp.clone();
        let bc = bc_tx.clone();
        let mut analyzer = OrderFlowAnalyzer::new(20_000);
        tokio::spawn(async move {
            while let Some(trade) = tick_rx.recv().await {
                analyzer.ingest(&trade);
                let vp_read = vp.read().await;
                if let Some(bubble) =
                    analyzer.detect_absorption(&vp_read, trade.price_f64(), 15.0)
                {
                    let _ = bc.send(WsFrame::Bubbles { data: bubble });
                }
            }
        });
    }

    // ---------- Candle consumer: VP update on 15M close ----------
    {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            while let Some(ev) = kline_rx.recv().await {
                if !ev.kline.is_closed {
                    continue;
                }
                let candle = ev.kline.to_vp_candle();
                let mut vp_w = vp.write().await;
                vp_w.ingest_candle(candle);
                let levels = vp_w.all_levels();
                *cached.write().await = levels.clone();
                for lvl in levels {
                    let _ = bc.send(WsFrame::Levels { data: lvl });
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