//! Node 3 HTTP and WebSocket server.
//!
//! Public, browser-safe resources:
//! - `GET /health`
//! - `GET /diagnostics` (`/status` alias)
//! - `GET /scanning`
//! - `GET /signals`
//! - `WS  /ws` diagnostics stream
//!
//! Private service link:
//! - `WS /execution` — Node 4 authenticates first with `execution_hello`.
//!   Trade intents are never broadcast on the public socket.

use std::time::Duration;

use futures_util::{Sink, SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::diagnostics::DiagnosticsHub;
use crate::types::{WsFrame, EXECUTION_PROTOCOL_VERSION};

const OK: &str = "HTTP/1.1 200 OK\r\n\
Content-Type: text/plain\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Length: 2\r\n\
Connection: close\r\n\
\r\n\
ok";

const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\n\
Content-Type: text/plain\r\n\
Access-Control-Allow-Origin: *\r\n\
Content-Length: 9\r\n\
Connection: close\r\n\
\r\n\
not found";

const CORS_PREFLIGHT: &str = "HTTP/1.1 204 No Content\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Accept, Origin\r\n\
Access-Control-Max-Age: 600\r\n\
Content-Length: 0\r\n\
Connection: close\r\n\
\r\n";

fn clean_path(raw_path: &str) -> &str {
    raw_path.split('?').next().unwrap_or(raw_path)
}

#[cfg(test)]
pub fn response_for(path: &str) -> &'static str {
    if clean_path(path) == "/health" {
        OK
    } else {
        NOT_FOUND
    }
}

fn json_http_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\n\
Content-Type: application/json\r\n\
Cache-Control: no-store\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Accept, Origin\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\
\r\n\
{}",
        body.len(),
        body
    )
}

pub fn is_websocket_upgrade(raw_request: &str) -> bool {
    let lower = raw_request.to_ascii_lowercase();
    lower.contains("upgrade: websocket") || lower.contains("sec-websocket-key:")
}

pub fn is_public_ws_path(raw_path: &str) -> bool {
    matches!(clean_path(raw_path), "/ws" | "/stream" | "/diagnostics/ws")
}

pub fn is_execution_ws_path(raw_path: &str) -> bool {
    clean_path(raw_path) == "/execution"
}

pub fn topic_matches(topics: &[String], frame: &WsFrame) -> bool {
    // Private service frames must never reach Node 2 or any unauthenticated
    // public WebSocket client, even with an `all` subscription.
    let frame_topic = match frame {
        WsFrame::Diagnostics { .. } => "diagnostics",
        WsFrame::Scanning { .. } => "scanning",
        WsFrame::Signals { .. } => "signals",
        WsFrame::DiagnosticEvent { .. } => "diagnostic_event",
        WsFrame::Heartbeat => return true,
        _ => return false,
    };
    topics.is_empty()
        || topics.iter().any(|topic| {
            matches!(topic.as_str(), "all" | "*" | "diagnostics") || topic == frame_topic
        })
}

async fn send_frame<S>(
    sender: &mut S,
    frame: &WsFrame,
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let text = serde_json::to_string(frame).unwrap_or_else(|_| "{}".into());
    sender.send(Message::Text(text.into())).await
}

async fn send_public_snapshots<S>(
    sender: &mut S,
    hub: &DiagnosticsHub,
    topics: &[String],
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let frames = [
        WsFrame::Diagnostics {
            data: Box::new(hub.snapshot().await),
        },
        WsFrame::Scanning {
            data: hub.scanning_snapshot().await,
        },
        WsFrame::Signals {
            data: hub.signal_snapshot().await,
        },
    ];
    for frame in frames {
        if topic_matches(topics, &frame) {
            send_frame(sender, &frame).await?;
        }
    }
    Ok(())
}

async fn handle_public_ws(stream: TcpStream, hub: DiagnosticsHub) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(stream) => stream,
        Err(error) => {
            debug!("Public WebSocket handshake failed: {error}");
            return;
        }
    };

    let client_count = hub.public_client_connected();
    info!("Node 3 public WS client connected ({client_count} active)");
    let (mut sender, mut receiver) = ws_stream.split();
    let mut broadcast = hub.subscribe();
    let mut topics = Vec::<String>::new();

    if send_public_snapshots(&mut sender, &hub, &topics)
        .await
        .is_err()
    {
        hub.public_client_disconnected();
        return;
    }

    let mut heartbeat = interval(Duration::from_secs(25));
    heartbeat.tick().await;
    let mut refresh = interval(Duration::from_secs(5));
    refresh.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if send_frame(&mut sender, &WsFrame::Heartbeat).await.is_err() {
                    break;
                }
            }
            _ = refresh.tick() => {
                let frame = WsFrame::Diagnostics {
                    data: Box::new(hub.snapshot().await),
                };
                if topic_matches(&topics, &frame) && send_frame(&mut sender, &frame).await.is_err() {
                    break;
                }
            }
            event = broadcast.recv() => {
                match event {
                    Ok(frame) if topic_matches(&topics, &frame) => {
                        if send_frame(&mut sender, &frame).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if send_public_snapshots(&mut sender, &hub, &topics).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            message = receiver.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                            match frame {
                                WsFrame::Subscribe { topics: requested } => {
                                    topics = requested;
                                    if send_public_snapshots(&mut sender, &hub, &topics).await.is_err() {
                                        break;
                                    }
                                }
                                WsFrame::Snapshot => {
                                    if send_public_snapshots(&mut sender, &hub, &topics).await.is_err() {
                                        break;
                                    }
                                }
                                WsFrame::Heartbeat => {
                                    if send_frame(&mut sender, &WsFrame::Heartbeat).await.is_err() {
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    let remaining = hub.public_client_disconnected();
    info!("Node 3 public WS client disconnected ({remaining} active)");
}

async fn reject_execution<S>(sender: &mut S, error: &str)
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let _ = send_frame(
        sender,
        &WsFrame::ExecutionHelloAck {
            accepted: false,
            protocol_version: EXECUTION_PROTOCOL_VERSION,
            error: Some(error.into()),
        },
    )
    .await;
    let _ = sender.send(Message::Close(None)).await;
}

async fn handle_execution_ws(stream: TcpStream, config: Config, hub: DiagnosticsHub) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(stream) => stream,
        Err(error) => {
            debug!("Execution WebSocket handshake failed: {error}");
            return;
        }
    };
    let (mut sender, mut receiver) = ws_stream.split();

    let hello = match timeout(Duration::from_secs(5), receiver.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<WsFrame>(&text).ok(),
        _ => None,
    };
    let Some(WsFrame::ExecutionHello {
        token,
        protocol_version,
        ..
    }) = hello
    else {
        reject_execution(&mut sender, "first frame must be execution_hello").await;
        return;
    };

    let Some(expected_token) = config.node4_shared_token.as_deref() else {
        reject_execution(
            &mut sender,
            "NODE4_SHARED_TOKEN is not configured on Node 3",
        )
        .await;
        return;
    };
    if token != expected_token {
        reject_execution(&mut sender, "invalid NODE4_SHARED_TOKEN").await;
        return;
    }
    if protocol_version != EXECUTION_PROTOCOL_VERSION {
        reject_execution(
            &mut sender,
            &format!(
                "unsupported execution protocol {protocol_version}; expected {EXECUTION_PROTOCOL_VERSION}"
            ),
        )
        .await;
        return;
    }
    if !hub.try_execution_client_connected() {
        reject_execution(
            &mut sender,
            "another Node 4 execution client is already connected",
        )
        .await;
        return;
    }

    info!("Node 4 authenticated on the private execution socket");
    if send_frame(
        &mut sender,
        &WsFrame::ExecutionHelloAck {
            accepted: true,
            protocol_version: EXECUTION_PROTOCOL_VERSION,
            error: None,
        },
    )
    .await
    .is_err()
    {
        hub.execution_client_disconnected();
        return;
    }

    // Replay every unexpired, unacknowledged intent. Node 4 must deduplicate by
    // intent_id, so a disconnect between execution and report is still safe.
    for intent in hub.pending_intents().await {
        if send_frame(&mut sender, &WsFrame::TradeIntent { data: intent })
            .await
            .is_err()
        {
            hub.execution_client_disconnected();
            return;
        }
    }

    let mut broadcast = hub.subscribe();
    let mut heartbeat = interval(Duration::from_secs(20));
    heartbeat.tick().await;
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if send_frame(&mut sender, &WsFrame::Heartbeat).await.is_err() {
                    break;
                }
            }
            event = broadcast.recv() => {
                match event {
                    Ok(WsFrame::TradeIntent { data }) => {
                        if send_frame(&mut sender, &WsFrame::TradeIntent { data }).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        for intent in hub.pending_intents().await {
                            if send_frame(&mut sender, &WsFrame::TradeIntent { data: intent }).await.is_err() {
                                hub.execution_client_disconnected();
                                return;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            message = receiver.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsFrame>(&text) {
                            Ok(WsFrame::ExecutionReport { data }) => {
                                let intent_id = data.intent_id.clone();
                                let result = hub.record_execution_report(data).await;
                                let frame = WsFrame::ExecutionReportAck {
                                    intent_id,
                                    accepted: result.is_ok(),
                                    error: result.err(),
                                };
                                if send_frame(&mut sender, &frame).await.is_err() {
                                    break;
                                }
                            }
                            Ok(WsFrame::Heartbeat) => {
                                if send_frame(&mut sender, &WsFrame::Heartbeat).await.is_err() {
                                    break;
                                }
                            }
                            Ok(_) => {
                                warn!("Node 4 sent a frame that is not valid on /execution");
                            }
                            Err(error) => {
                                warn!("Node 4 sent malformed JSON: {error}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    hub.execution_client_disconnected();
    info!("Node 4 execution socket disconnected");
}

async fn handle_connection(mut stream: TcpStream, config: Config, hub: DiagnosticsHub) {
    let mut peek_buffer = [0_u8; 4_096];
    let mut bytes_read = 0;
    for _ in 0..20 {
        match timeout(Duration::from_secs(3), stream.peek(&mut peek_buffer)).await {
            Ok(Ok(count)) if count > 0 => {
                bytes_read = count;
                if String::from_utf8_lossy(&peek_buffer[..count]).contains("\r\n\r\n") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            _ => return,
        }
    }
    if bytes_read == 0 {
        return;
    }

    let preview = String::from_utf8_lossy(&peek_buffer[..bytes_read]);
    let first_line = preview.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = clean_path(raw_path).to_string();

    if is_websocket_upgrade(&preview) {
        if is_public_ws_path(&path) {
            handle_public_ws(stream, hub).await;
            return;
        }
        if is_execution_ws_path(&path) {
            handle_execution_ws(stream, config, hub).await;
            return;
        }
    }

    let mut read_buffer = [0_u8; 4_096];
    let _ = stream.read(&mut read_buffer).await;
    if method.eq_ignore_ascii_case("OPTIONS") {
        let _ = stream.write_all(CORS_PREFLIGHT.as_bytes()).await;
        let _ = stream.shutdown().await;
        return;
    }

    let response = match path.as_str() {
        "/health" => OK.to_string(),
        "/diagnostics" | "/status" => json_http_response(
            &serde_json::to_string(&hub.snapshot().await).unwrap_or_else(|_| "{}".into()),
        ),
        "/scanning" => json_http_response(
            &serde_json::to_string(&hub.scanning_snapshot().await).unwrap_or_else(|_| "{}".into()),
        ),
        "/signals" => json_http_response(
            &serde_json::to_string(&hub.signal_snapshot().await).unwrap_or_else(|_| "{}".into()),
        ),
        _ => NOT_FOUND.to_string(),
    };
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

pub async fn serve(config: Config, hub: DiagnosticsHub) {
    let listener = match TcpListener::bind(("0.0.0.0", config.port)).await {
        Ok(listener) => listener,
        Err(error) => {
            error!("Failed to bind Node 3 on 0.0.0.0:{}: {error}", config.port);
            return;
        }
    };
    info!(
        "Node 3 strategy listening on 0.0.0.0:{} (public /ws, private /execution)",
        config.port
    );

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let config = config.clone();
                let hub = hub.clone();
                tokio::spawn(async move {
                    handle_connection(stream, config, hub).await;
                });
            }
            Err(error) => error!("Server accept error: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TradeIntent, INTENT_SCHEMA_VERSION};

    async fn test_server(config: Config, hub: DiagnosticsHub) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let config = config.clone();
                let hub = hub.clone();
                tokio::spawn(async move {
                    handle_connection(stream, config, hub).await;
                });
            }
        });
        port
    }

    fn pending_intent() -> TradeIntent {
        let timestamp = chrono::Utc::now().timestamp_millis();
        TradeIntent {
            schema_version: INTENT_SCHEMA_VERSION,
            intent_id: "n3-test-1".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2_650.0,
            stop_loss: 2_647.0,
            take_profit: 2_656.0,
            risk_reward: 2.0,
            level_name: "PW PoC".into(),
            source_candle_time: timestamp,
            created_at: timestamp,
            expires_at: timestamp + 60_000,
        }
    }

    #[test]
    fn routes_are_separated() {
        assert!(is_public_ws_path("/ws?client=node2"));
        assert!(!is_public_ws_path("/execution"));
        assert!(is_execution_ws_path("/execution"));
        assert!(!is_execution_ws_path("/ws"));
        assert!(response_for("/health?t=1").starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn public_topic_filter_blocks_private_frames() {
        let intent = WsFrame::TradeIntent {
            data: pending_intent(),
        };
        assert!(!topic_matches(&[], &intent));
        assert!(!topic_matches(&["all".into()], &intent));
        assert!(topic_matches(
            &["signals".into()],
            &WsFrame::Signals {
                data: crate::types::SignalSnapshot {
                    pending: vec![],
                    recent: vec![],
                    execution_reports: vec![],
                    pending_count: 0,
                    node4_connected: false,
                    node4_token_configured: false,
                    timestamp: 0,
                }
            }
        ));
    }

    #[tokio::test]
    async fn private_socket_authenticates_and_replays_pending_intent() {
        let config = Config {
            node4_shared_token: Some("shared-test-token".into()),
            ..Config::default()
        };
        let hub = DiagnosticsHub::new(&config);
        assert!(hub.record_intent(pending_intent()).await);
        let port = test_server(config, hub).await;

        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/execution"))
                .await
                .unwrap();
        ws.send(Message::Text(
            serde_json::json!({
                "type": "execution_hello",
                "token": "shared-test-token",
                "service": "xauusd-node4-execution",
                "protocol_version": EXECUTION_PROTOCOL_VERSION
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

        let ack: serde_json::Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(ack["type"], "execution_hello_ack");
        assert_eq!(ack["accepted"], true);

        let replay: serde_json::Value =
            serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(replay["type"], "trade_intent");
        assert_eq!(replay["data"]["intent_id"], "n3-test-1");
    }

    #[tokio::test]
    async fn public_http_snapshot_identifies_strategy_service() {
        let config = Config::default();
        let hub = DiagnosticsHub::new(&config);
        let port = test_server(config, hub).await;
        let response: serde_json::Value =
            reqwest::get(format!("http://127.0.0.1:{port}/diagnostics"))
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
        assert_eq!(response["work"]["service"], "xauusd-node3-strategy");
        assert_eq!(response["work"]["role"], "strategy_only");
        assert!(response.get("deriv_account").is_none());
    }
}
