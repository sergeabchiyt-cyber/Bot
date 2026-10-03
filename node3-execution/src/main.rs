mod config;
mod execution;
mod execution_chelsea;
mod execution_deriv;
mod strategy;
mod types;

use std::time::Duration;
use futures_util::{SinkExt, StreamExt};
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::Config;
use execution::ExecutionManager;
use strategy::StrategyEngine;
use types::WsFrame;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    info!("Starting Node 3 (Rust Automated Execution Service)");
    info!("Node 1 WS URL: {}", config.node1_ws_url);
    info!(
        "Strategy Settings: Vol Threshold > {:.0}, SL: {:.0}-{:.0} pips, TP: {:.0}-{:.0} pips, Target RR: {:.1}-{:.1}",
        config.volume_threshold, config.sl_min_pips, config.sl_max_pips, config.tp_min_pips, config.tp_max_pips, config.rr_min, config.rr_max
    );

    let mut exec_mgr = ExecutionManager::new(config.clone());
    let mut strategy = StrategyEngine::new();

    loop {
        info!("Connecting to Node 1 WS at {}", config.node1_ws_url);
        match connect_async(&config.node1_ws_url).await {
            Ok((mut ws, _)) => {
                info!("Connected to Node 1 WebSocket stream");

                let subscribe_msg = serde_json::json!({
                    "type": "subscribe",
                    "topics": ["candle", "levels", "trades"]
                });

                if let Err(e) = ws.send(Message::Text(subscribe_msg.to_string().into())).await {
                    error!("Failed to send subscribe frame: {e}");
                    sleep(Duration::from_secs(3)).await;
                    continue;
                }

                while let Some(msg) = ws.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                                match frame {
                                    WsFrame::Levels { data } => {
                                        strategy.update_levels(&data);
                                    }
                                    WsFrame::Candle { data } => {
                                        let atr = strategy.calculate_atr();
                                        exec_mgr.update_atr(atr);

                                        if let Some((side, price, level_name)) =
                                            strategy.evaluate_candle(&data, config.volume_threshold)
                                        {
                                            info!(
                                                "Triggering order execution: {} @ {:.2} on level {}",
                                                side, price, level_name
                                            );
                                            match exec_mgr
                                                .execute(side, config.order_size, price, &level_name)
                                                .await
                                            {
                                                Ok(trade) => {
                                                    info!("Trade successfully placed: {:?}", trade);
                                                    let trade_frame = serde_json::json!({
                                                        "type": "trades",
                                                        "data": trade
                                                    });
                                                    let _ = ws.send(Message::Text(trade_frame.to_string().into())).await;
                                                }
                                                Err(e) => {
                                                    error!("Trade execution failed: {e}");
                                                }
                                            }
                                        }
                                    }
                                    WsFrame::Heartbeat => {
                                        let _ = ws.send(Message::Text(r#"{"type":"heartbeat"}"#.into())).await;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(Message::Ping(p)) => {
                            let _ = ws.send(Message::Pong(p)).await;
                        }
                        Ok(Message::Close(_)) => {
                            warn!("Node 1 closed the connection");
                            break;
                        }
                        Err(e) => {
                            error!("WebSocket stream error: {e}");
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                warn!("Failed to connect to Node 1: {e}. Retrying in 5 seconds...");
            }
        }

        sleep(Duration::from_secs(5)).await;
    }
}
