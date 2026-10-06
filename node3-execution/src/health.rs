//! HTTP + WebSocket diagnostics and health server for Node 3.
//!
//! Serves on `0.0.0.0:$PORT` (default 10000):
//! - `WS  /ws` (or `/`)      -> Real-time WebSocket stream of scanning setups, open trades,
//!                              Deriv Demo account balance & open contracts, and engine diagnostics.
//! - `GET /health`           -> `200 OK` (`ok`) for platform health checks.
//! - `GET /diagnostics`      -> `200 OK` JSON `DiagnosticsSnapshot`.
//! - `GET /status`           -> `200 OK` JSON `DiagnosticsSnapshot`.
//! - `GET /scanning`         -> `200 OK` JSON `ScanningSnapshot`.
//! - `GET /open-trades`      -> `200 OK` JSON `OpenTradesSnapshot`.
//! - `GET /trades`           -> `200 OK` JSON `OpenTradesSnapshot`.
//! - `GET /deriv`            -> `200 OK` JSON `DerivAccountSnapshot`.
//! - `GET /account`          -> `200 OK` JSON `DerivAccountSnapshot`.
//! - `GET /mt5/account`      -> `200 OK` JSON `Mt5AccountSnapshot` (MT5 demo bridge).
//! - `GET /mt5/positions`    -> `200 OK` JSON `Mt5PositionsSnapshot` (broker positions).
//! - `GET /mt5/history`      -> `200 OK` JSON `Mt5HistorySnapshot` (closed deals).
//! - `GET /mt5/status`       -> `200 OK` JSON `Mt5BridgeStatus` (link + counters).
//! - `POST /mt5/control`     -> halt / resume / close_all / close_position, requires
//!                              `X-Control-Token: $MT5_CONTROL_TOKEN` (403 without it).
//! - `WS  /ws`               -> the MT5 bridge also connects here and identifies
//!                              itself with `bridge_hello { token }`.

use std::time::Duration;
use futures_util::{Sink, SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::diagnostics::DiagnosticsHub;
use crate::types::WsFrame;

/// Split halves of an accepted WebSocket, named so the bridge session can be a
/// separate function instead of a nested block.
type WsSender = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<TcpStream>,
    Message,
>;
type WsReceiver =
    futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>;

/// How long `/ws` waits for a `bridge_hello` before treating the socket as a
/// frontend client. The bridge sends it immediately after connecting.
const BRIDGE_HELLO_PROBE_MS: u64 = 1500;

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
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Accept, Origin, X-Control-Token, Authorization\r\n\
Access-Control-Max-Age: 600\r\n\
Content-Length: 0\r\n\
Connection: close\r\n\
\r\n";

fn clean_path(raw_path: &str) -> &str {
    raw_path.split('?').next().unwrap_or(raw_path)
}

/// Static response helper retained for `/health` vs unknown path checks.
pub fn response_for(path: &str) -> &'static str {
    if clean_path(path) == "/health" {
        OK
    } else {
        NOT_FOUND
    }
}

fn json_http_response(body: &str) -> String {
    json_http_response_status("200 OK", body)
}

/// JSON response with an explicit status line (403 for missing control tokens,
/// 503 when the bridge is unreachable, …).
fn json_http_response_status(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\n\
Content-Type: application/json\r\n\
Cache-Control: no-store\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Accept, Origin, X-Control-Token, Authorization\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\
\r\n\
{}",
        body.len(),
        body
    )
}

fn error_json(code: &str, message: &str) -> String {
    serde_json::json!({ "ok": false, "error": { "code": code, "message": message } })
        .to_string()
}

/// Case-insensitive header lookup over the raw request head.
fn header_value(head: &str, name: &str) -> Option<String> {
    for line in head.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case(name) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

/// Parse the JSON body of a control request, tolerating an empty body.
fn parse_control_body(body: &str) -> serde_json::Value {
    if body.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(body).unwrap_or_else(|_| serde_json::json!({ "action": "__invalid__" }))
    }
}

pub fn is_websocket_upgrade(raw_request: &str) -> bool {
    let lower = raw_request.to_ascii_lowercase();
    lower.contains("upgrade: websocket") || lower.contains("sec-websocket-key:")
}

pub fn is_ws_path(raw_path: &str) -> bool {
    matches!(
        clean_path(raw_path),
        "/ws" | "/" | "/stream" | "/diagnostics/ws"
    )
}

pub fn topic_matches(topics: &[String], frame: &WsFrame) -> bool {
    if topics.is_empty() {
        return true;
    }
    let has_wildcard = topics
        .iter()
        .any(|t| matches!(t.as_str(), "all" | "*" | "diagnostics"));
    if has_wildcard {
        return true;
    }
    let frame_topic = match frame {
        WsFrame::Diagnostics { .. } => "diagnostics",
        WsFrame::Scanning { .. } => "scanning",
        WsFrame::OpenTrades { .. } => "open_trades",
        WsFrame::DerivAccount { .. } => "deriv_account",
        WsFrame::Mt5Account { .. } => "mt5_account",
        WsFrame::Mt5Positions { .. } => "mt5_positions",
        WsFrame::Mt5History { .. } => "mt5_history",
        WsFrame::BridgeStatus { .. } => "bridge_status",
        WsFrame::BridgeEvent { .. } => "bridge_event",
        // Bridge-only frames (handshake + command acks) are never forwarded to
        // frontend clients, whatever they subscribe to.
        WsFrame::BridgeHello { .. } | WsFrame::BridgeHelloAck { .. } | WsFrame::BridgeAck { .. } => {
            return false
        }
        WsFrame::Trades { .. } => "trades",
        WsFrame::DiagnosticEvent { .. } => "diagnostic_event",
        WsFrame::Levels { .. } => "levels",
        WsFrame::Candle { .. } => "candle",
        WsFrame::Heartbeat => return true,
        _ => return false,
    };
    topics.iter().any(|t| t == frame_topic)
}

async fn send_initial_snapshots<S>(
    sender: &mut S,
    hub: &DiagnosticsHub,
    topics: &[String],
) -> Result<(), tokio_tungstenite::tungstenite::Error>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let diag = WsFrame::Diagnostics {
        data: hub.snapshot().await,
    };
    if topic_matches(topics, &diag) {
        if let Ok(txt) = serde_json::to_string(&diag) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    let scanning = WsFrame::Scanning {
        data: hub.scanning_snapshot().await,
    };
    if topic_matches(topics, &scanning) {
        if let Ok(txt) = serde_json::to_string(&scanning) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    let open_trades = WsFrame::OpenTrades {
        data: hub.open_trades_snapshot().await,
    };
    if topic_matches(topics, &open_trades) {
        if let Ok(txt) = serde_json::to_string(&open_trades) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    let deriv = WsFrame::DerivAccount {
        data: hub.deriv_snapshot().await,
    };
    if topic_matches(topics, &deriv) {
        if let Ok(txt) = serde_json::to_string(&deriv) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    // MT5 demo bridge resources (kept separate from the Deriv options ones).
    let mt5_account = WsFrame::Mt5Account {
        data: hub.mt5_account_snapshot().await,
    };
    if topic_matches(topics, &mt5_account) {
        if let Ok(txt) = serde_json::to_string(&mt5_account) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    let mt5_positions = WsFrame::Mt5Positions {
        data: hub.mt5_positions_snapshot().await,
    };
    if topic_matches(topics, &mt5_positions) {
        if let Ok(txt) = serde_json::to_string(&mt5_positions) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    let mt5_history = WsFrame::Mt5History {
        data: hub.mt5_history_snapshot().await,
    };
    if topic_matches(topics, &mt5_history) {
        if let Ok(txt) = serde_json::to_string(&mt5_history) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    let bridge_status = WsFrame::BridgeStatus {
        data: hub.mt5_status_snapshot().await,
    };
    if topic_matches(topics, &bridge_status) {
        if let Ok(txt) = serde_json::to_string(&bridge_status) {
            sender.send(Message::Text(txt.into())).await?;
        }
    }

    Ok(())
}

/// The MT5 bridge's own session on `/ws`.
///
/// The bridge is a *client* of this endpoint: it dials out from the terminal
/// host (so no inbound port is exposed there), authenticates with
/// `bridge_hello { token }`, then pushes snapshots and answers commands. Frames
/// it sends are applied to the shared MT5 state and rebroadcast to frontends;
/// commands from the rest of the service are queued through an mpsc channel.
async fn handle_bridge_session(
    mut sender: WsSender,
    mut receiver: WsReceiver,
    hub: DiagnosticsHub,
    config: Config,
    hello: BridgeHelloFields,
) {
    let link = hub.mt5();
    if let Err(reason) = link
        .authorize_hello(hello.token.as_deref(), config.mt5_bridge_token.as_deref())
        .await
    {
        warn!("rejecting MT5 bridge handshake: {reason}");
        let ack = WsFrame::BridgeHelloAck {
            ok: false,
            error: Some(reason),
            protocol: Some(crate::execution_mt5::BRIDGE_PROTOCOL_VERSION),
        };
        if let Ok(txt) = serde_json::to_string(&ack) {
            let _ = sender.send(Message::Text(txt.into())).await;
        }
        let _ = sender.close().await;
        return;
    }

    let info = crate::execution_mt5::BridgeSessionInfo {
        token_ok: true,
        protocol: hello.protocol.unwrap_or(1),
        bridge_version: hello.bridge.clone().unwrap_or_default(),
        venue: hello.venue.clone().unwrap_or_default(),
        capabilities: hello.capabilities.clone().unwrap_or_default(),
        ..Default::default()
    };
    let (generation, mut out_rx) = link.register_session(info).await;

    let ack = WsFrame::BridgeHelloAck {
        ok: true,
        error: None,
        protocol: Some(crate::execution_mt5::BRIDGE_PROTOCOL_VERSION),
    };
    if let Ok(txt) = serde_json::to_string(&ack) {
        if sender.send(Message::Text(txt.into())).await.is_err() {
            link.unregister_session(generation).await;
            return;
        }
    }

    // Ask for a fresh snapshot in the background: the request waits for a
    // `bridge_ack` that only the loop below can read, so awaiting it here would
    // stall the session until the timeout.
    {
        let link = hub.mt5();
        tokio::spawn(async move {
            let _ = link
                .request_snapshot(&["account", "positions", "history"])
                .await;
        });
    }

    let mut heartbeat = interval(Duration::from_secs(20));
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if let Ok(txt) = serde_json::to_string(&WsFrame::Heartbeat) {
                    if sender.send(Message::Text(txt.into())).await.is_err() {
                        break;
                    }
                }
            }
            out = out_rx.recv() => {
                match out {
                    Some(text) => {
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsFrame>(&text) {
                            Ok(WsFrame::Heartbeat) => {
                                // Applying it records liveness; answering lets the
                                // bridge measure ours.
                                hub.apply_mt5_frame(WsFrame::Heartbeat).await;
                                let reply = serde_json::to_string(&WsFrame::Heartbeat)
                                    .unwrap_or_else(|_| "{\"type\":\"heartbeat\"}".into());
                                if sender.send(Message::Text(reply.into())).await.is_err() {
                                    break;
                                }
                            }
                            Ok(WsFrame::BridgeHello { .. }) => {
                                warn!("bridge sent a second bridge_hello — ignoring");
                            }
                            Ok(frame) => {
                                // `apply_mt5_frame` records liveness and
                                // rebroadcasts; nothing else to do here.
                                let _ = hub.apply_mt5_frame(frame).await;
                            }
                            Err(err) => {
                                debug!("unparseable bridge frame: {err}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(err)) => {
                        debug!("bridge socket error: {err}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    link.unregister_session(generation).await;
    info!("MT5 bridge session closed");
}

/// `POST /mt5/control` — the operator kill switch.
///
/// Actions: `halt` (refuse new orders, keep monitoring, optionally `flatten`),
/// `resume`, `close_all`, `close_position` (by `position_ticket`). Every action
/// travels to the bridge as a normal command and is acknowledged by the broker,
/// so a 200 here means "the bridge confirmed", not "the request was queued".
async fn handle_mt5_control(
    hub: &DiagnosticsHub,
    config: &Config,
    method: &str,
    head: &str,
    body: &str,
) -> (&'static str, String) {
    if !method.eq_ignore_ascii_case("POST") {
        return (
            "405 Method Not Allowed",
            error_json("method_not_allowed", "POST /mt5/control"),
        );
    }

    let Some(expected) = config.mt5_control_token.as_deref() else {
        return (
            "403 Forbidden",
            error_json(
                "control_disabled",
                "MT5_CONTROL_TOKEN is not set on this service, so remote control is disabled",
            ),
        );
    };
    let presented = header_value(head, "x-control-token").or_else(|| {
        header_value(head, "authorization").and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
                .map(|token| token.trim().to_string())
        })
    });
    match presented.as_deref() {
        Some(token) if token == expected => {}
        Some(_) => {
            return (
                "403 Forbidden",
                error_json("bad_control_token", "X-Control-Token did not match"),
            )
        }
        None => {
            return (
                "403 Forbidden",
                error_json("missing_control_token", "send X-Control-Token"),
            )
        }
    }

    let link = hub.mt5();
    if !link.configured() {
        return (
            "503 Service Unavailable",
            error_json("mt5_not_configured", "MT5_BRIDGE_TOKEN is not set"),
        );
    }
    if !link.connected().await {
        return (
            "503 Service Unavailable",
            error_json(
                "bridge_not_connected",
                "the MT5 bridge is not connected; the command was not sent",
            ),
        );
    }

    let payload = parse_control_body(body);
    let action = payload
        .get("action")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let reason = payload
        .get("reason")
        .and_then(|value| value.as_str())
        .unwrap_or("operator request via /mt5/control")
        .to_string();

    let result = match action.as_str() {
        "halt" => {
            let flatten = payload
                .get("flatten")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            link.halt(&reason, flatten).await
        }
        "resume" => link.resume().await,
        "close_all" => link.close_all(&reason).await,
        "close_position" => match payload.get("position_ticket").and_then(|v| v.as_i64()) {
            Some(ticket) => {
                let volume = payload.get("volume").and_then(|v| v.as_f64());
                link.close_position(ticket, volume).await
            }
            None => {
                return (
                    "400 Bad Request",
                    error_json("missing_position_ticket", "position_ticket is required"),
                )
            }
        },
        "" => {
            return (
                "400 Bad Request",
                error_json(
                    "missing_action",
                    "action must be one of halt, resume, close_all, close_position",
                ),
            )
        }
        other => {
            return (
                "400 Bad Request",
                error_json(
                    "unknown_action",
                    &format!("action '{other}' is not one of halt, resume, close_all, close_position"),
                ),
            )
        }
    };

    match result {
        Ok(reply) => {
            let body = serde_json::json!({
                "ok": true,
                "action": action,
                "req_id": reply.req_id,
                "data": reply.data,
            })
            .to_string();
            ("200 OK", body)
        }
        Err(err) => (
            "502 Bad Gateway",
            error_json("bridge_error", &err.to_string()),
        ),
    }
}

/// Fields of `bridge_hello`, unpacked so the session handler has a plain struct.
#[derive(Debug, Clone, Default)]
pub struct BridgeHelloFields {
    pub token: Option<String>,
    pub protocol: Option<u32>,
    pub bridge: Option<String>,
    pub venue: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

async fn handle_ws_client(stream: TcpStream, hub: DiagnosticsHub, config: Config) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            debug!("WebSocket handshake error: {e}");
            return;
        }
    };

    let (mut sender, mut receiver) = ws_stream.split();

    // Role split: the MT5 bridge identifies itself with `bridge_hello` as its
    // first frame, so a short probe is enough to tell it apart from a browser.
    let mut pending_first: Option<WsFrame> = None;
    if let Ok(Some(Ok(Message::Text(text)))) =
        timeout(Duration::from_millis(BRIDGE_HELLO_PROBE_MS), receiver.next()).await
    {
        match serde_json::from_str::<WsFrame>(&text) {
            Ok(WsFrame::BridgeHello {
                token,
                protocol,
                bridge,
                venue,
                capabilities,
                ..
            }) => {
                let hello = BridgeHelloFields {
                    token,
                    protocol,
                    bridge,
                    venue,
                    capabilities,
                };
                handle_bridge_session(sender, receiver, hub, config, hello).await;
                return;
            }
            Ok(frame) => pending_first = Some(frame),
            Err(err) => debug!("unparseable first WS frame: {err}"),
        }
    }

    let client_count = hub.client_connected();
    info!("Node 3 WS client connected (active clients: {client_count})");

    let mut rx = hub.subscribe();
    let mut topics: Vec<String> = Vec::new();
    if let Some(WsFrame::Subscribe { topics: new_topics }) = pending_first {
        topics = new_topics;
    }

    if send_initial_snapshots(&mut sender, &hub, &topics)
        .await
        .is_err()
    {
        hub.client_disconnected();
        return;
    }

    let mut heartbeat_timer = interval(Duration::from_secs(25));
    heartbeat_timer.tick().await;

    let mut diag_refresh_timer = interval(Duration::from_secs(5));
    diag_refresh_timer.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat_timer.tick() => {
                if let Ok(txt) = serde_json::to_string(&WsFrame::Heartbeat) {
                    if sender.send(Message::Text(txt.into())).await.is_err() {
                        break;
                    }
                }
            }
            _ = diag_refresh_timer.tick() => {
                let diag = WsFrame::Diagnostics {
                    data: hub.snapshot().await,
                };
                if topic_matches(&topics, &diag) {
                    if let Ok(txt) = serde_json::to_string(&diag) {
                        if sender.send(Message::Text(txt.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            broadcast_msg = rx.recv() => {
                match broadcast_msg {
                    Ok(frame) => {
                        if topic_matches(&topics, &frame) {
                            if let Ok(txt) = serde_json::to_string(&frame) {
                                if sender.send(Message::Text(txt.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Client fell behind; send latest authoritative snapshot
                        if send_initial_snapshots(&mut sender, &hub, &topics).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            client_msg = receiver.next() => {
                match client_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                            match frame {
                                WsFrame::Subscribe { topics: new_topics } => {
                                    topics = new_topics;
                                    if send_initial_snapshots(&mut sender, &hub, &topics).await.is_err() {
                                        break;
                                    }
                                }
                                WsFrame::Snapshot => {
                                    if send_initial_snapshots(&mut sender, &hub, &topics).await.is_err() {
                                        break;
                                    }
                                }
                                WsFrame::Heartbeat => {
                                    if let Ok(txt) = serde_json::to_string(&WsFrame::Heartbeat) {
                                        if sender.send(Message::Text(txt.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        if sender.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    let remaining = hub.client_disconnected();
    info!("Node 3 WS client disconnected (active clients: {remaining})");
}

async fn handle_connection(mut stream: TcpStream, hub: DiagnosticsHub, config: Config) {
    let mut peek_buf = [0u8; 4096];
    let mut n = 0;

    // Wait briefly until the HTTP header terminator `\r\n\r\n` is in the socket buffer
    for _ in 0..20 {
        match timeout(Duration::from_secs(3), stream.peek(&mut peek_buf)).await {
            Ok(Ok(bytes_read)) if bytes_read > 0 => {
                n = bytes_read;
                if String::from_utf8_lossy(&peek_buf[..n]).contains("\r\n\r\n") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            _ => return,
        }
    }

    if n == 0 {
        return;
    }

    let preview = String::from_utf8_lossy(&peek_buf[..n]);
    let first_line = preview.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = clean_path(raw_path).to_string();

    if is_websocket_upgrade(&preview) && is_ws_path(&path) {
        handle_ws_client(stream, hub, config).await;
        return;
    }

    // Standard HTTP request: consume what has been buffered, then any body the
    // declared Content-Length still expects (used by `POST /mt5/control`).
    let mut read_buf = [0u8; 8192];
    let mut read = 0usize;
    while read < read_buf.len() {
        let bytes = match timeout(Duration::from_millis(500), stream.read(&mut read_buf[read..])).await {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(bytes)) => bytes,
            Ok(Err(_)) => break,
        };
        read += bytes;
        let text = String::from_utf8_lossy(&read_buf[..read]);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let declared = header_value(head, "content-length")
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if body.len() >= declared {
                break;
            }
        }
    }
    let raw = String::from_utf8_lossy(&read_buf[..read]).to_string();
    let (head, body) = match raw.split_once("\r\n\r\n") {
        Some((head, body)) => (head.to_string(), body.to_string()),
        None => (raw.clone(), String::new()),
    };

    if method.eq_ignore_ascii_case("OPTIONS") {
        let _ = stream.write_all(CORS_PREFLIGHT.as_bytes()).await;
        let _ = stream.shutdown().await;
        return;
    }

    match path.as_str() {
        "/health" => {
            let _ = stream.write_all(OK.as_bytes()).await;
        }
        "/diagnostics" | "/status" => {
            let snap = hub.snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        "/scanning" => {
            let snap = hub.scanning_snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        "/trades" | "/open-trades" => {
            let snap = hub.open_trades_snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        "/deriv" | "/account" => {
            let snap = hub.deriv_snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        // ---- MT5 demo bridge resources (separate from the Deriv options ones)
        "/mt5/account" => {
            let snap = hub.mt5_account_snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        "/mt5/positions" => {
            let snap = hub.mt5_positions_snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        "/mt5/history" => {
            let snap = hub.mt5_history_snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        "/mt5/status" => {
            let snap = hub.mt5_status_snapshot().await;
            let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
            let resp = json_http_response(&body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        // Kill switch / position control. Token-gated: without MT5_CONTROL_TOKEN
        // configured the endpoint answers 403 so a public deployment cannot be
        // told to flatten positions by an anonymous caller.
        "/mt5/control" => {
            let (status, body) = handle_mt5_control(&hub, &config, method, &head, &body).await;
            let resp = json_http_response_status(status, &body);
            let _ = stream.write_all(resp.as_bytes()).await;
        }
        _ => {
            let _ = stream.write_all(NOT_FOUND.as_bytes()).await;
        }
    }

    let _ = stream.shutdown().await;
}

/// Serves HTTP health/diagnostics and `/ws` WebSocket stream on `0.0.0.0:{port}`.
pub async fn serve(port: u16, hub: DiagnosticsHub, config: Config) {
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind Node 3 server on 0.0.0.0:{port}: {e}");
            return;
        }
    };

    info!(
        "Node 3 HTTP & WS server listening on 0.0.0.0:{port} (GET /health, GET /diagnostics, \
         GET /mt5/*, POST /mt5/control, WS /ws)"
    );

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let hub_clone = hub.clone();
                let config_clone = config.clone();
                tokio::spawn(async move {
                    handle_connection(stream, hub_clone, config_clone).await;
                });
            }
            Err(e) => error!("Server accept error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_path_is_ok() {
        assert!(response_for("/health").starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn query_string_is_ignored() {
        assert!(response_for("/health?t=1").starts_with("HTTP/1.1 200 OK"));
    }

    #[test]
    fn other_paths_are_not_found() {
        assert!(response_for("/").starts_with("HTTP/1.1 404"));
        assert!(response_for("/healthz").starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn detects_websocket_upgrade_headers() {
        let req = "GET /ws HTTP/1.1\r\nHost: localhost:10000\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: abc\r\n\r\n";
        assert!(is_websocket_upgrade(req));
        assert!(is_ws_path("/ws"));
        assert!(is_ws_path("/ws?client=node2"));
        assert!(is_ws_path("/"));
        assert!(!is_ws_path("/health"));
    }

    #[test]
    fn topic_filter_allows_default_and_specific_topics() {
        let empty: Vec<String> = vec![];
        assert!(topic_matches(&empty, &WsFrame::Heartbeat));

        let specific = vec!["scanning".to_string(), "deriv_account".to_string()];
        assert!(topic_matches(&specific, &WsFrame::Heartbeat));
        assert!(!topic_matches(
            &specific,
            &WsFrame::Trades {
                data: crate::types::TradeEvent {
                    trade_id: "1".into(),
                    symbol: "XAUUSD".into(),
                    side: "buy".into(),
                    size: 0.01,
                    entry: 2650.0,
                    sl: 2647.0,
                    tp: 2656.0,
                    status: "open".into(),
                    timestamp: 0,
                    level_name: None,
                    venue: None,
                    rr: None,
                    current_price: None,
                    unrealized_pnl: None,
                    closed_at: None,
                }
            }
        ));
    }

    #[tokio::test]
    async fn ws_and_http_diagnostics_server_roundtrip() {
        let cfg = crate::config::Config::from_env();
        let hub = DiagnosticsHub::new(&cfg);
        hub.update_deriv_balance(10_000.0, Some("USD".into()), Some("VRTC9001".into()))
            .await;

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hub_srv = hub.clone();

        let cfg_srv = cfg.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let h = hub_srv.clone();
                let c = cfg_srv.clone();
                tokio::spawn(async move {
                    handle_connection(stream, h, c).await;
                });
            }
        });

        // 1. Test WebSocket stream on /ws
        let ws_url = format!("ws://127.0.0.1:{port}/ws");
        let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

        let msg1 = ws.next().await.unwrap().unwrap();
        let v1: serde_json::Value = serde_json::from_str(msg1.to_text().unwrap()).unwrap();
        assert_eq!(v1["type"], "diagnostics");
        assert_eq!(v1["data"]["deriv_account"]["balance"], 10_000.0);
        assert_eq!(v1["data"]["deriv_account"]["account_id"], "VRTC9001");

        let msg2 = ws.next().await.unwrap().unwrap();
        let v2: serde_json::Value = serde_json::from_str(msg2.to_text().unwrap()).unwrap();
        assert_eq!(v2["type"], "scanning");

        let msg3 = ws.next().await.unwrap().unwrap();
        let v3: serde_json::Value = serde_json::from_str(msg3.to_text().unwrap()).unwrap();
        assert_eq!(v3["type"], "open_trades");

        let msg4 = ws.next().await.unwrap().unwrap();
        let v4: serde_json::Value = serde_json::from_str(msg4.to_text().unwrap()).unwrap();
        assert_eq!(v4["type"], "deriv_account");
        assert_eq!(v4["data"]["balance"], 10_000.0);

        let _ = ws.close(None).await;

        // 2. Test HTTP GET /diagnostics
        let client = reqwest::Client::new();
        let diag_resp: serde_json::Value = client
            .get(format!("http://127.0.0.1:{port}/diagnostics"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(diag_resp["deriv_account"]["balance"], 10_000.0);
        assert_eq!(diag_resp["work"]["service"], "xauusd-node3-execution");
    }

    /// Spin up the real server on an ephemeral port with MT5 credentials and a
    /// temp dir for anything that writes, returning the port.
    async fn spawn_server(cfg: crate::config::Config) -> u16 {
        let hub = DiagnosticsHub::new(&cfg);
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let cfg_srv = cfg.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let h = hub.clone();
                let c = cfg_srv.clone();
                tokio::spawn(async move {
                    handle_connection(stream, h, c).await;
                });
            }
        });
        port
    }

    /// Read the next *command* frame from the fake bridge, skipping the
    /// housekeeping Node 3 sends on its own (session snapshot request,
    /// heartbeats). Returns `None` when nothing command-like arrives in time.
    async fn next_command_frame<S>(
        ws: &mut S,
        wait: Duration,
    ) -> Option<serde_json::Value>
    where
        S: futures_util::Stream<
                Item = Result<Message, tokio_tungstenite::tungstenite::Error>,
            > + Unpin,
    {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let message = match tokio::time::timeout(remaining, ws.next()).await {
                Ok(Some(Ok(message))) => message,
                _ => return None,
            };
            let Ok(text) = message.to_text() else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
                continue;
            };
            match value["type"].as_str().unwrap_or("") {
                "mt5_snapshot_request" | "mt5_ping" | "heartbeat" => continue,
                _ => return Some(value),
            }
        }
    }

    fn mt5_config() -> crate::config::Config {
        let mut cfg = crate::config::Config::from_env();
        cfg.mt5_bridge_token = Some("test-bridge-token".into());
        cfg.mt5_control_token = Some("test-control-token".into());
        cfg.mt5_symbol = "XAUUSD".into();
        cfg
    }

    /// The whole bridge role: authenticate on `/ws`, push a snapshot, and be
    /// addressable by the operator kill switch.
    #[tokio::test]
    async fn mt5_bridge_session_authenticates_streams_and_accepts_control() {
        let cfg = mt5_config();
        let port = spawn_server(cfg.clone()).await;

        let ws_url = format!("ws://127.0.0.1:{port}/ws");
        let (mut bridge_ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

        // 1. Wrong token is rejected and the socket is closed.
        let (mut impostor, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        let hello_bad = serde_json::json!({
            "type": "bridge_hello",
            "token": "not-the-token",
            "protocol": 1,
            "bridge": "mt5-bridge-test",
            "venue": "deriv_mt5_demo",
            "capabilities": ["order_send", "close_all", "halt", "resume"],
        });
        impostor
            .send(Message::Text(hello_bad.to_string().into()))
            .await
            .unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(5), impostor.next())
            .await
            .expect("hello_ack timeout")
            .expect("socket closed without an ack")
            .unwrap();
        let reply: serde_json::Value = serde_json::from_str(reply.to_text().unwrap()).unwrap();
        assert_eq!(reply["type"], "bridge_hello_ack");
        assert_eq!(reply["ok"], false);
        assert!(reply["error"].as_str().unwrap().contains("invalid"));

        // 2. The real token is accepted.
        let hello = serde_json::json!({
            "type": "bridge_hello",
            "token": "test-bridge-token",
            "protocol": 1,
            "bridge": "mt5-bridge-test",
            "venue": "deriv_mt5_demo",
            "capabilities": ["order_send", "position_close", "close_all", "halt", "resume"],
        });
        bridge_ws
            .send(Message::Text(hello.to_string().into()))
            .await
            .unwrap();
        let ack = tokio::time::timeout(Duration::from_secs(5), bridge_ws.next())
            .await
            .expect("hello_ack timeout")
            .expect("bridge socket closed")
            .unwrap();
        let ack: serde_json::Value = serde_json::from_str(ack.to_text().unwrap()).unwrap();
        assert_eq!(ack["type"], "bridge_hello_ack");
        assert_eq!(ack["ok"], true);

        // The bridge pushes its account snapshot; Node 3 must rebroadcast it and
        // serve it from `/mt5/account` without ever seeing MT5_LOGIN/PASSWORD.
        let mut account = crate::types::Mt5AccountSnapshot::default();
        account.configured = true;
        account.connected = true;
        account.authorized = true;
        account.account_type = "demo".into();
        account.login = Some(123_456);
        account.server = Some("Deriv-Demo".into());
        account.company = Some("Deriv".into());
        account.balance = Some(10_000.0);
        account.equity = Some(10_000.0);
        account.requested_symbol = "XAUUSD".into();
        account.broker_symbol = Some("XAUUSD".into());
        account.symbol_digits = Some(3);
        account.symbol_volume_min = Some(0.01);
        account.symbol_volume_step = Some(0.01);
        account.symbol_contract_size = Some(100.0);
        account.trade_allowed = true;
        let frame = WsFrame::Mt5Account { data: account };
        bridge_ws
            .send(Message::Text(serde_json::to_string(&frame).unwrap().into()))
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/mt5/account");
        let mut served = None;
        for _ in 0..20 {
            let value: serde_json::Value = client
                .get(&url)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if value["connected"] == true {
                served = Some(value);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let served = served.expect("GET /mt5/account never reflected the pushed snapshot");
        assert_eq!(served["account_type"], "demo");
        assert_eq!(served["login"], 123_456);
        assert_eq!(served["broker_symbol"], "XAUUSD");
        assert_eq!(served["balance"], 10_000.0);

        // 3. Kill switch: without the control token nothing is sent.
        let denied = client
            .post(format!("http://127.0.0.1:{port}/mt5/control"))
            .json(&serde_json::json!({ "action": "halt" }))
            .send()
            .await
            .unwrap();
        assert_eq!(denied.status().as_u16(), 403);
        if let Some(frame) = next_command_frame(&mut bridge_ws, Duration::from_millis(300)).await {
            panic!("an unauthorised control request reached the bridge: {frame}");
        }

        // 4. With the token the command travels to the bridge and the HTTP
        //    response reflects the bridge's acknowledgement.
        let control = async {
            client
                .post(format!("http://127.0.0.1:{port}/mt5/control"))
                .header("X-Control-Token", "test-control-token")
                .json(&serde_json::json!({ "action": "halt", "reason": "test" }))
                .send()
                .await
                .unwrap()
        };
        let (http_reply, bridge_frame) = tokio::join!(
            control,
            async {
                let frame = next_command_frame(&mut bridge_ws, Duration::from_secs(5))
                    .await
                    .expect("bridge never received the halt command");
                // Ack like the real bridge does: as soon as the command is
                // processed. The HTTP response above waits for exactly this, so
                // acking after the join would deadlock the test.
                let req_id = frame["req_id"].as_str().unwrap_or_default().to_string();
                bridge_ws
                    .send(Message::Text(
                        serde_json::json!({
                            "type": "bridge_ack",
                            "req_id": req_id,
                            "ok": true,
                            "data": { "halted": true, "halt_reason": "test" },
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
                frame
            }
        );
        assert_eq!(bridge_frame["type"], "mt5_halt");
        assert_eq!(bridge_frame["reason"], "test");

        let body: serde_json::Value = http_reply.json().await.unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["action"], "halt");
        assert_eq!(body["data"]["halted"], true);

        // 5. A frontend client sees the MT5 resources it subscribes to.
        let (mut frontend, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
        frontend
            .send(Message::Text(
                serde_json::json!({ "type": "subscribe", "topics": ["mt5_account"] })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let pushed = tokio::time::timeout(Duration::from_secs(5), frontend.next())
            .await
            .expect("frontend never received mt5_account")
            .expect("frontend socket closed")
            .unwrap();
        let pushed: serde_json::Value = serde_json::from_str(pushed.to_text().unwrap()).unwrap();
        assert_eq!(pushed["type"], "mt5_account");
        assert_eq!(pushed["data"]["login"], 123_456);
    }

    /// The bridge command path is useless without a live session: the strategy
    /// must be told to refuse rather than trade blind.
    #[tokio::test]
    async fn mt5_commands_fail_closed_without_a_bridge_session() {
        let cfg = mt5_config();
        let hub = DiagnosticsHub::new(&cfg);
        let link = hub.mt5();
        assert!(link.configured());
        assert!(!link.connected().await);
        assert!(link.close_all("test").await.is_err());
        assert!(link.halt("test", false).await.is_err());
        assert!(link.resume().await.is_err());
        assert!(link.ping(200).await.is_err());
    }
}
