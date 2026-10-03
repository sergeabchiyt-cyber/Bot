mod config;
mod diagnostics;
mod execution;
mod execution_chelsea;
mod execution_deriv;
mod health;
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
use diagnostics::DiagnosticsHub;
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
    info!("HTTP & Diagnostics WS port: {}", config.port);

    let hub = DiagnosticsHub::new(&config);

    // Serve HTTP (/health, /diagnostics, /scanning, /open-trades, /deriv) and WebSocket (/ws) on 0.0.0.0:$PORT
    tokio::spawn(health::serve(config.port, hub.clone()));

    // Spawn live Deriv Demo account monitor (streams balance & open contracts when DERIV_DEMO_API is set)
    tokio::spawn(execution_deriv::spawn_deriv_monitor(
        config.clone(),
        hub.clone(),
    ));

    info!(
        "Strategy Settings: Vol Threshold > {:.0}, SL: {:.0}-{:.0} pips, TP: {:.0}-{:.0} pips, Target RR: {:.1}-{:.1}",
        config.volume_threshold,
        config.sl_min_pips,
        config.sl_max_pips,
        config.tp_min_pips,
        config.tp_max_pips,
        config.rr_min,
        config.rr_max
    );

    let mut exec_mgr = ExecutionManager::new(config.clone());
    let mut strategy = StrategyEngine::new();
    let mut first_attempt = true;

    loop {
        info!("Connecting to Node 1 WS at {}", config.node1_ws_url);
        hub.set_node1_state(
            false,
            if first_attempt {
                "connecting"
            } else {
                "reconnecting"
            },
            !first_attempt,
        )
        .await;
        first_attempt = false;

        match connect_async(&config.node1_ws_url).await {
            Ok((mut ws, _)) => {
                info!("Connected to Node 1 WebSocket stream");
                hub.set_node1_state(true, "connected", false).await;

                let subscribe_msg = serde_json::json!({
                    "type": "subscribe",
                    "topics": ["candle", "levels", "trades"]
                });

                if let Err(e) = ws
                    .send(Message::Text(subscribe_msg.to_string().into()))
                    .await
                {
                    error!("Failed to send subscribe frame: {e}");
                    hub.set_node1_state(false, "subscribe_error", false).await;
                    sleep(Duration::from_secs(3)).await;
                    continue;
                }

                while let Some(msg) = ws.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            hub.record_ws_message().await;

                            if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                                match frame {
                                    WsFrame::Levels { data } => {
                                        strategy.update_levels(&data);
                                        let scanning = strategy.build_scanning_snapshot(
                                            &exec_mgr,
                                            config.volume_threshold,
                                        );
                                        hub.record_levels_update(
                                            &data.window,
                                            data.poc,
                                            data.vah,
                                            data.val,
                                            scanning,
                                        )
                                        .await;
                                    }
                                    WsFrame::Candle { data } => {
                                        let report = strategy.evaluate_candle_with_diagnostics(
                                            &data,
                                            config.volume_threshold,
                                        );
                                        let atr = strategy.calculate_atr();
                                        exec_mgr.update_atr(atr);

                                        let (_, _, sl_pips, tp_pips, rr) =
                                            exec_mgr.compute_sl_tp_details(data.close, "buy");
                                        let scanning = strategy.build_scanning_snapshot(
                                            &exec_mgr,
                                            config.volume_threshold,
                                        );

                                        hub.record_candle_update(
                                            &data,
                                            atr,
                                            sl_pips,
                                            tp_pips,
                                            rr,
                                            strategy.candle_history.len(),
                                            report.breaks_detected,
                                            report.breaks_invalidated,
                                            report.retests_rejected_low_volume,
                                            report.signals_confirmed,
                                            report.events,
                                            scanning,
                                        )
                                        .await;

                                        if let Some((side, price, level_name)) = report.triggered {
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
                                                    hub.record_trade_opened(trade.clone()).await;
                                                    let trade_frame = serde_json::json!({
                                                        "type": "trades",
                                                        "data": trade
                                                    });
                                                    let _ = ws
                                                        .send(Message::Text(
                                                            trade_frame.to_string().into(),
                                                        ))
                                                        .await;
                                                }
                                                Err(e) => {
                                                    error!("Trade execution failed: {e}");
                                                    hub.record_trade_failed(&e.to_string()).await;
                                                }
                                            }
                                        }
                                    }
                                    WsFrame::Heartbeat => {
                                        let _ = ws
                                            .send(Message::Text(
                                                r#"{"type":"heartbeat"}"#.into(),
                                            ))
                                            .await;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(Message::Ping(p)) => {
                            hub.record_ws_message().await;
                            let _ = ws.send(Message::Pong(p)).await;
                        }
                        Ok(Message::Close(_)) => {
                            warn!("Node 1 closed the connection");
                            hub.set_node1_state(false, "disconnected", false).await;
                            break;
                        }
                        Err(e) => {
                            error!("WebSocket stream error: {e}");
                            hub.set_node1_state(false, "stream_error", false).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                warn!("Failed to connect to Node 1: {e}. Retrying in 5 seconds...");
                hub.set_node1_state(false, "connection_failed", false).await;
            }
        }

        sleep(Duration::from_secs(5)).await;
    }
}
