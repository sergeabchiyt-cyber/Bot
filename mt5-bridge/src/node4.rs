//! Node 4 link: the bridge dials **out** to Node 4's WebSocket endpoint and
//! keeps one authenticated session open.
//!
//! Outbound-only (no inbound exposure of the terminal host) plus a shared token
//! is the transport the design document requires. The bridge:
//!
//! * greets with `bridge_hello` and waits for Node 4's `bridge_hello_ack`;
//! * pushes `mt5_account` / `mt5_positions` / `mt5_history` / `bridge_status`
//!   snapshots on an interval (and immediately after anything that matters);
//! * answers Node 4 commands (`mt5_order`, `mt5_close`, `mt5_halt`, …) with a
//!   `bridge_ack` carrying the broker outcome;
//! * reconnects with exponential backoff, and re-pushes a full snapshot set on
//!   every (re)connect so Node 4 never has to guess.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::bridge::{Bridge, BridgeEvent};
use crate::config::BridgeConfig;
use crate::snapshot::{
    capabilities, BridgeErrorPayload, BridgeToNode4, Node4ToBridge, BRIDGE_PROTOCOL_VERSION,
    BRIDGE_VERSION,
};

const CONNECT_TIMEOUT_SECS: u64 = 20;
const HELLO_ACK_TIMEOUT_SECS: u64 = 15;

/// Reconnect loop; never returns.
pub async fn run(cfg: Arc<BridgeConfig>, bridge: Arc<Bridge>) {
    let mut backoff_secs = 1u64;
    loop {
        match run_once(cfg.clone(), bridge.clone()).await {
            Ok(()) => {
                warn!("Node 4 link closed — reconnecting in {backoff_secs}s");
                backoff_secs = 1;
            }
            Err(err) => {
                warn!("Node 4 link error: {err} — reconnecting in {backoff_secs}s");
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(30);
    }
}

async fn run_once(cfg: Arc<BridgeConfig>, bridge: Arc<Bridge>) -> anyhow::Result<()> {
    let token = cfg.node4_token.clone().ok_or_else(|| {
        anyhow::anyhow!("MT5_BRIDGE_TOKEN is not set; refusing to connect to Node 4")
    })?;

    let mut request = cfg
        .node4_ws_url
        .as_str()
        .into_client_request()
        .map_err(|err| anyhow::anyhow!("invalid NODE4_WS_URL: {err}"))?;
    request.headers_mut().insert(
        "user-agent",
        "mt5-bridge/".parse().expect("static header value"),
    );

    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out connecting to {}", cfg.node4_ws_url))?
    .map_err(|err| anyhow::anyhow!("handshake with {} failed: {err}", cfg.node4_ws_url))?;

    info!("connected to Node 4 at {}", cfg.node4_ws_url);
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = mpsc::channel::<String>(256);

    // Writer task: all socket writes go through one place.
    let writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if sink.send(Message::Text(line.into())).await.is_err() {
                break;
            }
        }
    });

    // Greeting.
    let account = bridge.account_snapshot().await;
    let hello = BridgeToNode4::BridgeHello {
        token,
        protocol: BRIDGE_PROTOCOL_VERSION,
        bridge: format!("mt5-bridge/{BRIDGE_VERSION}"),
        venue: "deriv_mt5_demo".into(),
        capabilities: capabilities(),
        account,
    };
    send_frame(&tx, &hello).await?;

    // Wait for the ack (ignoring unrelated frames such as heartbeats).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(HELLO_ACK_TIMEOUT_SECS);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("Node 4 did not acknowledge bridge_hello in time");
        }
        let msg = tokio::time::timeout(remaining, stream.next())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for bridge_hello_ack"))?;
        match msg {
            Some(Ok(Message::Text(text))) => {
                let parsed: Node4ToBridge = serde_json::from_str(&text)
                    .unwrap_or(Node4ToBridge::Unknown);
                match parsed {
                    Node4ToBridge::BridgeHelloAck { ok: true, .. } => {
                        info!("Node 4 accepted the bridge handshake");
                        break;
                    }
                    Node4ToBridge::BridgeHelloAck { ok: false, error } => {
                        anyhow::bail!(
                            "Node 4 rejected the bridge handshake: {}",
                            error.unwrap_or_else(|| "unknown reason".into())
                        );
                    }
                    Node4ToBridge::Mt5Ping { req_id } => {
                        send_frame(
                            &tx,
                            &BridgeToNode4::BridgeAck {
                                req_id,
                                ok: true,
                                data: Some(serde_json::json!({ "ts": crate::terminal::now_ms() })),
                                error: None,
                            },
                        )
                        .await?;
                    }
                    _ => debug!("ignoring frame while waiting for bridge_hello_ack"),
                }
            }
            Some(Ok(Message::Ping(payload))) => {
                let _ = send_frame(&tx, &BridgeToNode4::Heartbeat).await;
                debug!("ping from Node 4 ({} bytes)", payload.len());
            }
            Some(Ok(Message::Close(_))) | None => {
                anyhow::bail!("Node 4 closed the connection during the handshake");
            }
            Some(Ok(_)) => {}
            Some(Err(err)) => anyhow::bail!("Node 4 stream error during handshake: {err}"),
        }
    }

    // Full snapshot set on every (re)connect.
    push_snapshots(&bridge, &tx, true).await?;

    let mut events = bridge.subscribe();
    let mut push_timer = tokio::time::interval(Duration::from_millis(cfg.push_interval_ms.max(250)));
    push_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut heartbeat_timer = tokio::time::interval(Duration::from_secs(20));
    let mut history_timer = tokio::time::interval(Duration::from_secs(15));
    let mut last_history_count = bridge.history_store().deal_count().await;

    let result: anyhow::Result<()> = loop {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let parsed: Node4ToBridge = serde_json::from_str(&text)
                            .unwrap_or(Node4ToBridge::Unknown);
                        match parsed {
                            Node4ToBridge::Unknown => {
                                debug!("ignoring unrecognised Node 4 frame");
                            }
                            command => {
                                let bridge = bridge.clone();
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    handle_command(command, bridge, tx).await;
                                });
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        debug!("ping from Node 4 ({} bytes)", payload.len());
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break Ok(());
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => break Err(anyhow::anyhow!("stream error: {err}")),
                }
            }
            _ = push_timer.tick() => {
                if let Err(err) = push_snapshots(&bridge, &tx, false).await {
                    break Err(err);
                }
            }
            _ = history_timer.tick() => {
                // Only push history when it actually changed, so a quiet
                // account does not spam the frontend.
                let count = bridge.history_store().deal_count().await;
                if count != last_history_count {
                    last_history_count = count;
                    if let Err(err) = push_history(&bridge, &tx).await {
                        break Err(err);
                    }
                }
            }
            _ = heartbeat_timer.tick() => {
                if send_frame(&tx, &BridgeToNode4::Heartbeat).await.is_err() {
                    break Ok(());
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if let Err(err) = send_bridge_event(&bridge, &tx, event).await {
                            break Err(err);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("dropped {skipped} bridge events (link slower than event rate)");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break Ok(()),
                }
            }
        }
    };

    writer.abort();
    result
}

async fn send_frame(tx: &mpsc::Sender<String>, frame: &BridgeToNode4) -> anyhow::Result<()> {
    let text = serde_json::to_string(frame)?;
    tx.send(text)
        .await
        .map_err(|_| anyhow::anyhow!("Node 4 writer is gone"))
}

async fn push_snapshots(
    bridge: &Arc<Bridge>,
    tx: &mpsc::Sender<String>,
    include_history: bool,
) -> anyhow::Result<()> {
    send_frame(
        tx,
        &BridgeToNode4::Mt5Account {
            data: bridge.account_snapshot().await,
        },
    )
    .await?;
    send_frame(
        tx,
        &BridgeToNode4::Mt5Positions {
            data: bridge.positions_snapshot().await,
        },
    )
    .await?;
    send_frame(
        tx,
        &BridgeToNode4::BridgeStatus {
            data: bridge.status_snapshot().await,
        },
    )
    .await?;
    if include_history {
        push_history(bridge, tx).await?;
    }
    Ok(())
}

async fn push_history(bridge: &Arc<Bridge>, tx: &mpsc::Sender<String>) -> anyhow::Result<()> {
    send_frame(
        tx,
        &BridgeToNode4::Mt5History {
            data: bridge.history_snapshot(None).await,
        },
    )
    .await
}

async fn send_bridge_event(
    bridge: &Arc<Bridge>,
    tx: &mpsc::Sender<String>,
    event: BridgeEvent,
) -> anyhow::Result<()> {
    let (name, data) = match event {
        BridgeEvent::OrderFilled(outcome) => {
            let frame = BridgeToNode4::BridgeEvent {
                event: "order_filled".into(),
                data: serde_json::to_value(&*outcome)?,
            };
            send_frame(tx, &frame).await?;
            // Fills change positions immediately; do not wait for the timer.
            push_snapshots(bridge, tx, false).await?;
            return Ok(());
        }
        BridgeEvent::OrderRejected(outcome) => (
            "order_rejected",
            serde_json::to_value(&*outcome)?,
        ),
        BridgeEvent::OrderUnknown(outcome) => (
            "order_unknown",
            serde_json::to_value(&*outcome)?,
        ),
        BridgeEvent::Halted {
            reason,
            auto,
            flattened,
        } => {
            let data = serde_json::json!({
                "reason": reason,
                "auto": auto,
                "flattened": flattened,
                "halted": true,
            });
            send_frame(
                tx,
                &BridgeToNode4::BridgeEvent {
                    event: "halted".into(),
                    data,
                },
            )
            .await?;
            push_snapshots(bridge, tx, false).await?;
            return Ok(());
        }
        BridgeEvent::Resumed => (
            "resumed",
            serde_json::json!({ "halted": false }),
        ),
        BridgeEvent::LinkDisconnected(reason) => (
            "ea_link_disconnected",
            serde_json::json!({ "reason": reason }),
        ),
        BridgeEvent::BrokerTrade(fields) => (
            "trade_transaction",
            serde_json::json!({ "fields": fields }),
        ),
    };
    send_frame(
        tx,
        &BridgeToNode4::BridgeEvent {
            event: name.into(),
            data,
        },
    )
    .await
}

async fn ack_ok(
    tx: &mpsc::Sender<String>,
    req_id: String,
    data: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    send_frame(
        tx,
        &BridgeToNode4::BridgeAck {
            req_id,
            ok: true,
            data,
            error: None,
        },
    )
    .await
}

async fn ack_err(
    tx: &mpsc::Sender<String>,
    req_id: String,
    code: &str,
    message: impl Into<String>,
) -> anyhow::Result<()> {
    send_frame(
        tx,
        &BridgeToNode4::BridgeAck {
            req_id,
            ok: false,
            data: None,
            error: Some(BridgeErrorPayload::new(code, message)),
        },
    )
    .await
}

async fn handle_command(command: Node4ToBridge, bridge: Arc<Bridge>, tx: mpsc::Sender<String>) {
    match command {
        Node4ToBridge::Mt5Order {
            req_id,
            idempotency_key,
            strategy_id,
            symbol,
            side,
            volume,
            sl,
            tp,
            level_name,
            entry_ref,
            timeout_ms,
        } => {
            let intent = crate::bridge::OrderIntent {
                idempotency_key,
                intent_id: format!("N3-{}", req_id),
                strategy_id,
                requested_symbol: symbol,
                side,
                volume,
                sl,
                tp,
                level_name,
                entry_ref,
                timeout_ms,
            };
            let outcome = bridge.place_order(intent).await;
            let data = serde_json::to_value(&outcome).ok();
            if let Err(err) = ack_ok(&tx, req_id, data).await {
                debug!("failed to ack order: {err}");
            }
        }
        Node4ToBridge::Mt5Modify {
            req_id,
            position_ticket,
            sl,
            tp,
        } => {
            let result = bridge
                .modify_position(position_ticket, sl, tp)
                .await;
            match result {
                Ok(()) => {
                    let _ = ack_ok(&tx, req_id, Some(serde_json::json!({ "modified": true })))
                        .await;
                }
                Err(err) => {
                    let _ = ack_err(&tx, req_id, "modify_failed", err.to_string()).await;
                }
            }
        }
        Node4ToBridge::Mt5Close {
            req_id,
            position_ticket,
            volume,
        } => match bridge.close_position(position_ticket, volume).await {
            Ok(outcome) => {
                let _ = ack_ok(&tx, req_id, serde_json::to_value(&outcome).ok()).await;
            }
            Err(err) => {
                let _ = ack_err(&tx, req_id, "close_failed", err.to_string()).await;
            }
        },
        Node4ToBridge::Mt5CloseAll { req_id, reason } => {
            let reason = reason.unwrap_or_else(|| "node4 request".into());
            match bridge.close_all(&reason).await {
                Ok(closed) => {
                    let _ = ack_ok(
                        &tx,
                        req_id,
                        Some(serde_json::json!({
                            "closed": closed.len(),
                            "positions": closed,
                        })),
                    )
                    .await;
                }
                Err(err) => {
                    let _ = ack_err(&tx, req_id, "close_all_failed", err.to_string()).await;
                }
            }
        }
        Node4ToBridge::Mt5Halt {
            req_id,
            reason,
            flatten,
        } => {
            let reason = reason.unwrap_or_else(|| "halt requested by Node 4".into());
            let flatten = flatten.unwrap_or(false);
            let changed = bridge.halt(reason.clone(), flatten).await;
            let control = bridge.control().snapshot().await;
            let _ = ack_ok(
                &tx,
                req_id,
                Some(serde_json::json!({
                    "halted": control.halted,
                    "reason": control.reason,
                    "flatten": flatten,
                    "changed": changed,
                })),
            )
            .await;
        }
        Node4ToBridge::Mt5Resume { req_id } => match bridge.resume().await {
            Ok(()) => {
                let _ = ack_ok(&tx, req_id, Some(serde_json::json!({ "halted": false }))).await;
            }
            Err(err) => {
                let _ = ack_err(&tx, req_id, "resume_refused", err.to_string()).await;
            }
        },
        Node4ToBridge::Mt5SnapshotRequest { req_id, what } => {
            let what = what.unwrap_or_else(|| vec!["account".into(), "positions".into()]);
            let mut data = serde_json::Map::new();
            if what.iter().any(|w| w == "account") {
                data.insert(
                    "account".into(),
                    serde_json::to_value(bridge.account_snapshot().await)
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            if what.iter().any(|w| w == "positions") {
                data.insert(
                    "positions".into(),
                    serde_json::to_value(bridge.positions_snapshot().await)
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            if what.iter().any(|w| w == "history") {
                data.insert(
                    "history".into(),
                    serde_json::to_value(bridge.history_snapshot(None).await)
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            if what.iter().any(|w| w == "status") {
                data.insert(
                    "status".into(),
                    serde_json::to_value(bridge.status_snapshot().await)
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            let _ = ack_ok(&tx, req_id, Some(serde_json::Value::Object(data))).await;
        }
        Node4ToBridge::Mt5Ping { req_id } => {
            let _ = ack_ok(
                &tx,
                req_id,
                Some(serde_json::json!({ "ts": crate::terminal::now_ms() })),
            )
            .await;
        }
        Node4ToBridge::BridgeHelloAck { .. } => {}
        Node4ToBridge::Unknown => {}
    }
}
