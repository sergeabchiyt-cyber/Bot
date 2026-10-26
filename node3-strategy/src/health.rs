//! Node 3 HTTP and WebSocket server.
//!
//! Public, browser-safe resources:
//! - `GET /health`: JSON, HTTP 200 while the process is alive. Upstream
//!   problems appear in `status`, never as a failed probe.
//! - `GET /levels`: published `VpLevels` for PW, PS, and CW.
//! - `GET /vp?window=PW|PS|CW[&rows=N]`: one published profile.
//! - `GET /vp?start=<ms>&end=<ms>[&rows=N]`: profile for an explicit range.
//! - `GET /drawings`: FRVP rectangles, daily rays, and the Sunday-open marker.
//! - `GET /scanning`, `GET /signals`, `GET /diagnostics` (`/status` alias).
//! - `WS /ws` (`/stream`, `/diagnostics/ws`): topics `levels`, `drawing`, `vp`,
//!   `scanning`, `signals`, `diagnostics`. A replay is sent on subscribe.
//!
//! Private service link:
//! - `WS /execution`: Node 4 authenticates with `execution_hello` first.
//!   Trade intents and execution reports are never sent on the public socket.

use std::time::Duration;

use futures_util::{Sink, SinkExt, StreamExt};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::diagnostics::DiagnosticsHub;
use crate::types::{WsFrame, EXECUTION_PROTOCOL_VERSION};

/// Accepted `rows` range for `/vp`. Matches the bounds in `ProfileModel::sanitized`.
const ROWS_MIN: usize = 2;
const ROWS_MAX: usize = 5_000;

const CORS_PREFLIGHT: &str = "HTTP/1.1 204 No Content\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Accept, Origin\r\n\
Access-Control-Max-Age: 600\r\n\
Content-Length: 0\r\n\
Connection: close\r\n\
\r\n";

/// A rendered HTTP response: status line and JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpReply {
    pub status: &'static str,
    pub body: String,
}

impl HttpReply {
    fn json<T: Serialize>(status: &'static str, value: &T) -> Self {
        match serde_json::to_string(value) {
            Ok(body) => Self { status, body },
            Err(error) => Self::error("500 Internal Server Error", &error.to_string()),
        }
    }

    fn error(status: &'static str, message: &str) -> Self {
        Self {
            status,
            body: serde_json::json!({ "error": message }).to_string(),
        }
    }

    fn bad_request(message: &str) -> Self {
        Self::error("400 Bad Request", message)
    }

    fn render(&self) -> String {
        format!(
            "HTTP/1.1 {}\r\n\
Content-Type: application/json\r\n\
Cache-Control: no-store\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Accept, Origin\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\
\r\n\
{}",
            self.status,
            self.body.len(),
            self.body
        )
    }
}

fn clean_path(raw_path: &str) -> &str {
    raw_path.split('?').next().unwrap_or(raw_path)
}

fn query_param<'a>(raw_path: &'a str, key: &str) -> Option<&'a str> {
    let query = raw_path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

/// Answer one GET request. Every branch returns a response; nothing panics.
pub async fn route(raw_path: &str, hub: &DiagnosticsHub) -> HttpReply {
    match clean_path(raw_path) {
        "/health" => HttpReply::json("200 OK", &hub.health_snapshot().await),
        "/levels" => HttpReply::json("200 OK", &hub.market_levels().await),
        "/drawings" => HttpReply::json("200 OK", &hub.drawings_payload().await),
        "/vp" => vp_route(raw_path, hub).await,
        "/diagnostics" | "/status" => HttpReply::json("200 OK", &hub.snapshot().await),
        "/scanning" => HttpReply::json("200 OK", &hub.scanning_snapshot().await),
        "/signals" => HttpReply::json("200 OK", &hub.signal_snapshot().await),
        _ => HttpReply::error("404 Not Found", "not found"),
    }
}

async fn vp_route(raw_path: &str, hub: &DiagnosticsHub) -> HttpReply {
    let rows = match query_param(raw_path, "rows") {
        None => None,
        Some(raw) => match raw.parse::<usize>() {
            Ok(rows) if (ROWS_MIN..=ROWS_MAX).contains(&rows) => Some(rows),
            _ => return HttpReply::bad_request("rows must be an integer from 2 to 5000"),
        },
    };

    let start = query_param(raw_path, "start");
    let end = query_param(raw_path, "end");
    if start.is_some() || end.is_some() {
        let parsed = (
            start.and_then(|value| value.parse::<i64>().ok()),
            end.and_then(|value| value.parse::<i64>().ok()),
        );
        let (Some(start), Some(end)) = parsed else {
            return HttpReply::bad_request("start and end must be epoch milliseconds");
        };
        return match hub.custom_audit(start, end, rows).await {
            Ok(audit) => HttpReply::json("200 OK", &audit),
            Err(message) => HttpReply::bad_request(&message),
        };
    }

    let Some(window) = query_param(raw_path, "window") else {
        return HttpReply::bad_request("expected window=PW|PS|CW or start=<ms>&end=<ms>");
    };
    let window = window.to_ascii_uppercase();
    if !matches!(window.as_str(), "PW" | "PS" | "CW") {
        return HttpReply::bad_request("window must be PW, PS, or CW");
    }
    match hub.market_audit(&window, rows).await {
        Some(audit) => HttpReply::json("200 OK", &audit),
        None => HttpReply::error("404 Not Found", "window is not published yet"),
    }
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

/// Decide whether a frame may go to a public subscriber.
///
/// Only the public frame types below are ever sent here. Every private
/// execution frame is refused, whatever the topics say, including `all`.
pub fn topic_matches(topics: &[String], frame: &WsFrame) -> bool {
    let frame_topic = match frame {
        WsFrame::Levels { .. } => "levels",
        WsFrame::Drawing { .. } => "drawing",
        WsFrame::Vp { .. } => "vp",
        WsFrame::Diagnostics { .. } => "diagnostics",
        WsFrame::Scanning { .. } => "scanning",
        WsFrame::Signals { .. } => "signals",
        WsFrame::DiagnosticEvent { .. } => "diagnostic_event",
        WsFrame::Heartbeat => return true,
        // TradeIntent, ExecutionHello(Ack), ExecutionReport(Ack), and any other
        // frame are private and are never public.
        _ => return false,
    };
    if topics.is_empty() {
        return true;
    }
    let diagnostics_frame = matches!(
        frame_topic,
        "diagnostics" | "scanning" | "signals" | "diagnostic_event"
    );
    topics.iter().any(|topic| match topic.as_str() {
        "all" | "*" => true,
        "diagnostics" => diagnostics_frame,
        other => other == frame_topic,
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

/// Replay of the full public state, filtered by the subscriber's topics.
async fn send_public_replay<S>(
    sender: &mut S,
    hub: &DiagnosticsHub,
    topics: &[String],
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let mut frames = hub.market_replay_frames().await;
    frames.push(WsFrame::Diagnostics {
        data: Box::new(hub.snapshot().await),
    });
    frames.push(WsFrame::Scanning {
        data: hub.scanning_snapshot().await,
    });
    frames.push(WsFrame::Signals {
        data: hub.signal_snapshot().await,
    });
    for frame in &frames {
        if topic_matches(topics, frame) {
            send_frame(sender, frame).await?;
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

    if send_public_replay(&mut sender, &hub, &topics)
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
                if topic_matches(&topics, &frame)
                    && send_frame(&mut sender, &frame).await.is_err()
                {
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
                        if send_public_replay(&mut sender, &hub, &topics).await.is_err() {
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
                                    if send_public_replay(&mut sender, &hub, &topics).await.is_err() {
                                        break;
                                    }
                                }
                                WsFrame::Snapshot => {
                                    if send_public_replay(&mut sender, &hub, &topics).await.is_err() {
                                        break;
                                    }
                                }
                                WsFrame::Heartbeat => {
                                    if send_frame(&mut sender, &WsFrame::Heartbeat).await.is_err() {
                                        break;
                                    }
                                }
                                // Public clients cannot inject frames. Anything else is ignored.
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
                            let frame = WsFrame::TradeIntent { data: intent };
                            if send_frame(&mut sender, &frame).await.is_err() {
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

    let preview = String::from_utf8_lossy(&peek_buffer[..bytes_read]).into_owned();
    let first_line = preview.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let raw_path = parts.next().unwrap_or("/").to_string();

    if is_websocket_upgrade(&preview) {
        if is_public_ws_path(&raw_path) {
            handle_public_ws(stream, hub).await;
            return;
        }
        if is_execution_ws_path(&raw_path) {
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

    let reply = route(&raw_path, &hub).await;
    let _ = stream.write_all(reply.render().as_bytes()).await;
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
    use crate::types::{ExecutionReport, TradeIntent, INTENT_SCHEMA_VERSION, STRATEGY_NAME};

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
            strategy: STRATEGY_NAME.into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2_650.0,
            stop_loss: 2_648.0,
            take_profit: 2_656.0,
            risk_reward: 3.0,
            level_name: "PW PoC".into(),
            source_candle_time: timestamp,
            created_at: timestamp,
            expires_at: timestamp + 60_000,
            atr: None,
            atr_pips: None,
            level_id: None,
            drawing_id: None,
            strategy_version: None,
            volume_confirmed: None,
        }
    }

    fn execution_report() -> ExecutionReport {
        ExecutionReport {
            schema_version: INTENT_SCHEMA_VERSION,
            intent_id: "n3-test-1".into(),
            status: "accepted".into(),
            venue: "node4_venue".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            timestamp: 0,
            execution_id: None,
            filled_price: None,
            quantity: None,
            quantity_unit: None,
            error_code: None,
            error_message: None,
        }
    }

    #[test]
    fn routes_are_separated() {
        assert!(is_public_ws_path("/ws?client=node2"));
        assert!(!is_public_ws_path("/execution"));
        assert!(is_execution_ws_path("/execution"));
        assert!(!is_execution_ws_path("/ws"));
    }

    #[test]
    fn public_topic_filter_passes_only_public_frames() {
        let private = vec![
            WsFrame::TradeIntent {
                data: pending_intent(),
            },
            WsFrame::ExecutionReport {
                data: execution_report(),
            },
            WsFrame::ExecutionHelloAck {
                accepted: true,
                protocol_version: EXECUTION_PROTOCOL_VERSION,
                error: None,
            },
            WsFrame::ExecutionHello {
                token: "not-a-real-token".into(),
                service: String::new(),
                protocol_version: EXECUTION_PROTOCOL_VERSION,
            },
            WsFrame::ExecutionReportAck {
                intent_id: "n3-test-1".into(),
                accepted: true,
                error: None,
            },
        ];
        let topic_sets: Vec<Vec<String>> = vec![
            vec![],
            vec!["all".into()],
            vec!["*".into()],
            vec!["diagnostics".into()],
            vec![
                "levels".into(),
                "drawing".into(),
                "vp".into(),
                "scanning".into(),
                "signals".into(),
            ],
        ];
        for frame in &private {
            for topics in &topic_sets {
                assert!(
                    !topic_matches(topics, frame),
                    "private frame leaked for topics {topics:?}"
                );
            }
        }
    }

    #[test]
    fn public_market_frames_follow_their_topics() {
        let levels = WsFrame::Levels { data: Vec::new() };
        assert!(topic_matches(&["levels".into()], &levels));
        assert!(topic_matches(&["all".into()], &levels));
        assert!(!topic_matches(&["signals".into()], &levels));
        assert!(!topic_matches(&["diagnostics".into()], &levels));
        let signals = WsFrame::Signals {
            data: crate::types::SignalSnapshot {
                pending: vec![],
                recent: vec![],
                execution_reports: vec![],
                pending_count: 0,
                node4_connected: false,
                node4_token_configured: false,
                timestamp: 0,
            },
        };
        assert!(topic_matches(&["diagnostics".into()], &signals));
        assert!(topic_matches(&["signals".into()], &signals));
    }

    #[test]
    fn query_parameters_are_parsed_exactly() {
        assert_eq!(query_param("/vp?window=PW&rows=64", "rows"), Some("64"));
        assert_eq!(query_param("/vp?window=PW", "start"), None);
        assert_eq!(query_param("/vp", "window"), None);
        assert_eq!(query_param("/vp?flag&window=CW", "window"), Some("CW"));
    }

    #[tokio::test]
    async fn health_is_200_json_even_when_disconnected() {
        let hub = DiagnosticsHub::new(&Config::default());
        let reply = route("/health?probe=1", &hub).await;
        assert_eq!(reply.status, "200 OK");
        let body: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
        assert_eq!(body["status"], "disconnected");
        assert_eq!(body["ok"], false);
    }

    #[tokio::test]
    async fn vp_rejects_bad_requests_and_reports_unpublished_windows() {
        let hub = DiagnosticsHub::new(&Config::default());
        assert_eq!(route("/vp", &hub).await.status, "400 Bad Request");
        assert_eq!(route("/vp?window=XX", &hub).await.status, "400 Bad Request");
        assert_eq!(
            route("/vp?window=PW&rows=1", &hub).await.status,
            "400 Bad Request"
        );
        assert_eq!(
            route("/vp?start=10&end=5", &hub).await.status,
            "400 Bad Request"
        );
        assert_eq!(
            route("/vp?window=PW", &hub).await.status,
            "404 Not Found",
            "no PW before any bars arrive"
        );
    }

    #[tokio::test]
    async fn levels_and_drawings_are_empty_arrays_before_data() {
        let hub = DiagnosticsHub::new(&Config::default());
        assert_eq!(route("/levels", &hub).await.body, "[]");
        let drawings: serde_json::Value =
            serde_json::from_str(&route("/drawings", &hub).await.body).unwrap();
        assert!(drawings["items"].as_array().unwrap().is_empty());
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
    async fn public_http_health_and_diagnostics_are_served() {
        let config = Config::default();
        let hub = DiagnosticsHub::new(&config);
        let port = test_server(config, hub).await;

        let health = reqwest::get(format!("http://127.0.0.1:{port}/health"))
            .await
            .unwrap();
        assert_eq!(health.status().as_u16(), 200);

        let response: serde_json::Value =
            reqwest::get(format!("http://127.0.0.1:{port}/diagnostics"))
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
        assert_eq!(response["work"]["service"], "xauusd-node3-strategy");
        assert_eq!(response["work"]["role"], "strategy_only");
    }
}
