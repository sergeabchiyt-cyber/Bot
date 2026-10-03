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

use std::time::Duration;
use futures_util::{Sink, SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info};

use crate::diagnostics::DiagnosticsHub;
use crate::types::WsFrame;

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

/// Static response helper retained for `/health` vs unknown path checks.
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

    Ok(())
}

async fn handle_ws_client(stream: TcpStream, hub: DiagnosticsHub) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            debug!("WebSocket handshake error: {e}");
            return;
        }
    };

    let client_count = hub.client_connected();
    info!("Node 3 WS client connected (active clients: {client_count})");

    let (mut sender, mut receiver) = ws_stream.split();
    let mut rx = hub.subscribe();
    let mut topics: Vec<String> = Vec::new();

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

async fn handle_connection(mut stream: TcpStream, hub: DiagnosticsHub) {
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
        handle_ws_client(stream, hub).await;
        return;
    }

    // Standard HTTP request: consume from socket buffer
    let mut read_buf = [0u8; 4096];
    let _ = stream.read(&mut read_buf).await;

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
        _ => {
            let _ = stream.write_all(NOT_FOUND.as_bytes()).await;
        }
    }

    let _ = stream.shutdown().await;
}

/// Serves HTTP health/diagnostics and `/ws` WebSocket stream on `0.0.0.0:{port}`.
pub async fn serve(port: u16, hub: DiagnosticsHub) {
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind Node 3 server on 0.0.0.0:{port}: {e}");
            return;
        }
    };

    info!(
        "Node 3 HTTP & WS server listening on 0.0.0.0:{port} (GET /health, GET /diagnostics, WS /ws)"
    );

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let hub_clone = hub.clone();
                tokio::spawn(async move {
                    handle_connection(stream, hub_clone).await;
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

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let h = hub_srv.clone();
                tokio::spawn(async move {
                    handle_connection(stream, h).await;
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
}
