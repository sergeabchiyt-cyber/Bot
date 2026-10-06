mod config;
mod diagnostics;
mod health;
mod risk;
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
use risk::RiskProjector;
use strategy::StrategyEngine;
use types::{TradeIntent, WsFrame, INTENT_SCHEMA_VERSION};

fn intent_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn build_intent(
    config: &Config,
    risk: &RiskProjector,
    side: &str,
    reference_price: f64,
    level_name: &str,
    source_candle_time: i64,
) -> TradeIntent {
    let (stop_loss, take_profit, _, _, risk_reward) = risk.compute_details(reference_price, side);
    let created_at = chrono::Utc::now().timestamp_millis();
    TradeIntent {
        schema_version: INTENT_SCHEMA_VERSION,
        intent_id: format!(
            "n3-xauusd-{}-{}-{}",
            source_candle_time,
            side,
            intent_component(level_name)
        ),
        strategy: "vp_break_retest_v1".into(),
        symbol: "XAUUSD".into(),
        side: side.into(),
        order_type: "market".into(),
        reference_price,
        stop_loss,
        take_profit,
        risk_reward,
        level_name: level_name.into(),
        source_candle_time,
        created_at,
        expires_at: created_at + (config.signal_ttl_secs as i64 * 1_000),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    info!("Starting Node 3 (strategy-only service)");
    info!("Node 1 market stream: {}", config.node1_ws_url);
    info!("HTTP/public WS/private execution port: {}", config.port);
    if !config.node4_link_configured() {
        warn!(
            "NODE4_SHARED_TOKEN is not configured: the strategy will run and queue intents, but Node 4 cannot connect"
        );
    }

    let hub = DiagnosticsHub::new(&config);
    tokio::spawn(health::serve(config.clone(), hub.clone()));

    info!(
        "Strategy: volume >= {:.0}, SL {:.0}-{:.0} pips, TP {:.0}-{:.0} pips, RR {:.1}-{:.1}, intent TTL {}s",
        config.volume_threshold,
        config.sl_min_pips,
        config.sl_max_pips,
        config.tp_min_pips,
        config.tp_max_pips,
        config.rr_min,
        config.rr_max,
        config.signal_ttl_secs
    );

    let mut risk = RiskProjector::new(config.clone());
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
                info!("Connected to Node 1 market-data stream");
                hub.set_node1_state(true, "connected", false).await;
                let subscription = serde_json::json!({
                    "type": "subscribe",
                    "topics": ["candle", "levels"]
                });
                if let Err(error) = ws
                    .send(Message::Text(subscription.to_string().into()))
                    .await
                {
                    error!("Failed to subscribe to Node 1: {error}");
                    hub.set_node1_state(false, "subscribe_error", false).await;
                    sleep(Duration::from_secs(3)).await;
                    continue;
                }

                while let Some(message) = ws.next().await {
                    match message {
                        Ok(Message::Text(text)) => {
                            hub.record_ws_message().await;
                            let Ok(frame) = serde_json::from_str::<WsFrame>(&text) else {
                                continue;
                            };
                            match frame {
                                WsFrame::Levels { data } => {
                                    strategy.update_levels(&data);
                                    let scanning = strategy
                                        .build_scanning_snapshot(&risk, config.volume_threshold);
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
                                    risk.update_atr(atr);
                                    let (_, _, sl_pips, tp_pips, risk_reward) =
                                        risk.compute_details(data.close, "buy");
                                    let scanning = strategy
                                        .build_scanning_snapshot(&risk, config.volume_threshold);
                                    hub.record_candle_update(
                                        &data,
                                        atr,
                                        sl_pips,
                                        tp_pips,
                                        risk_reward,
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
                                        let intent = build_intent(
                                            &config,
                                            &risk,
                                            side,
                                            price,
                                            &level_name,
                                            data.time,
                                        );
                                        info!(
                                            "Strategy confirmed {} at {:.2} on {}; emitting intent {} for Node 4",
                                            side, price, level_name, intent.intent_id
                                        );
                                        if !hub.record_intent(intent).await {
                                            error!(
                                                "Intent was not emitted because the bounded pending queue is full"
                                            );
                                        }
                                    }
                                }
                                WsFrame::Heartbeat => {
                                    let _ = ws
                                        .send(Message::Text(r#"{"type":"heartbeat"}"#.into()))
                                        .await;
                                }
                                _ => {}
                            }
                        }
                        Ok(Message::Ping(payload)) => {
                            hub.record_ws_message().await;
                            let _ = ws.send(Message::Pong(payload)).await;
                        }
                        Ok(Message::Close(_)) => {
                            warn!("Node 1 closed the market-data stream");
                            hub.set_node1_state(false, "disconnected", false).await;
                            break;
                        }
                        Err(error) => {
                            error!("Node 1 WebSocket error: {error}");
                            hub.set_node1_state(false, "stream_error", false).await;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => {
                warn!("Failed to connect to Node 1: {error}; retrying in 5 seconds");
                hub.set_node1_state(false, "connection_failed", false).await;
            }
        }
        sleep(Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_id_is_deterministic_and_contains_no_credentials() {
        let config = Config::default();
        let risk = RiskProjector::new(config.clone());
        let intent = build_intent(&config, &risk, "buy", 2_650.25, "PW PoC", 1_700_000_000_000);
        assert_eq!(intent.intent_id, "n3-xauusd-1700000000000-buy-pw-poc");
        assert_eq!(intent.symbol, "XAUUSD");
        assert!(intent.stop_loss < intent.reference_price);
        assert!(intent.take_profit > intent.reference_price);
    }
}
