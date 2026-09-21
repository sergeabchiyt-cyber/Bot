mod config;
mod types;
mod binance_ws;
mod volume_profile;
mod order_flow;
mod execution;
mod execution_deriv;
mod execution_chelsea;
mod mcp_client;
mod ws_server;

use std::sync::Arc;
use axum::Router;
use tokio::sync::broadcast;
use tracing::info;
use tracing_subscriber::EnvFilter;

use config::Config;
use types::WsFrame;
use ws_server::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    info!("Starting XAUUSD engine on port {}", config.port);

    let (tx, _rx) = broadcast::channel::<WsFrame>(1024);
    let state = AppState {
        tx: tx.clone(),
        subscriptions: Arc::new(dashmap::DashMap::new()),
    };

    // Spawn Binance WS
    let binance_url = config.binance_ws_url.clone();
    let tx_binance = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = binance_ws::run_agg_trade_stream(binance_url, tx_binance).await {
            tracing::error!("Binance WS task terminated: {}", e);
        }
    });

    // HTTP + WS server
    let app = ws_server::router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    info!("Listening on 0.0.0.0:{}", config.port);
    axum::serve(listener, app).await?;
    Ok(())
}