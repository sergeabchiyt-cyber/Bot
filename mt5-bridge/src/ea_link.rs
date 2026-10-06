//! EA link: the loopback TCP server the MetaTrader expert advisor dials into.
//!
//! MQL5 can only create *client* sockets (`SocketCreate`/`SocketConnect`), so
//! the terminal connects to the bridge, never the other way around. One EA
//! session is active at a time; a new session that presents the right
//! `MT5_EA_TOKEN` replaces the old one (so an EA restart after a terminal crash
//! cannot leave a half-open socket in charge of trading).
//!
//! Guarantees:
//! * every request has a bounded timeout, and this layer never retries a write —
//!   an uncertain order outcome is resolved by reconciliation, never by a blind
//!   resend;
//! * without `MT5_EA_TOKEN` the link is read-only: order/modify/close methods are
//!   refused before they can reach the terminal;
//! * EA heartbeats are surfaced so the demo guard can be re-checked continuously
//!   instead of only at startup.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};

use crate::config::BridgeConfig;
use crate::proto::{self, EaKind, EaMessage};
use crate::terminal::{now_ms, BoxFut, EaResponse, TerminalError, TerminalTransport};

/// Aborts a task when the value is dropped, so early returns from a session
/// still tear its reader down.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Methods that change broker state. Refused when the link has no write token.
pub const WRITE_METHODS: &[&str] = &[
    proto::method::ORDER_SEND,
    proto::method::POS_MODIFY,
    proto::method::POS_CLOSE,
    proto::method::CLOSE_ALL,
    proto::method::ORDER_CANCEL,
];

pub fn is_write_method(method: &str) -> bool {
    WRITE_METHODS.iter().any(|m| m.eq_ignore_ascii_case(method))
}

#[derive(Debug, Clone, PartialEq)]
pub struct HelloInfo {
    pub login: i64,
    pub server: String,
    pub company: String,
    pub currency: String,
    pub mode: String,
    pub build: i64,
    pub ea_version: String,
    pub token_ok: bool,
    pub connected_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeartbeatInfo {
    pub login: i64,
    pub mode: String,
    pub connected: bool,
    pub trade_allowed: bool,
    pub ea_ts: i64,
    pub received_at: i64,
}

/// Continuously refreshed view of the terminal link.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkState {
    pub hello: Option<HelloInfo>,
    pub last_heartbeat: Option<HeartbeatInfo>,
    pub connected: bool,
    pub write_enabled: bool,
    pub last_error: Option<String>,
    pub connected_at: Option<i64>,
    pub disconnected_at: Option<i64>,
}

impl LinkState {
    pub fn heartbeat_age_ms(&self, now: i64) -> Option<i64> {
        self.last_heartbeat
            .as_ref()
            .map(|hb| now.saturating_sub(hb.received_at))
    }

    /// The mode the terminal reports most recently (a heartbeat overrides the
    /// greeting, which is how a mid-session demo→real change is caught).
    pub fn reported_mode(&self) -> Option<String> {
        self.last_heartbeat
            .as_ref()
            .map(|hb| hb.mode.clone())
            .or_else(|| self.hello.as_ref().map(|h| h.mode.clone()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkEvent {
    Connected(Box<HelloInfo>),
    Disconnected(String),
    Heartbeat(Box<HeartbeatInfo>),
    TradeEvent(BTreeMap<String, String>),
}

struct Outbound {
    id: u64,
    line: String,
    method: String,
    timeout_ms: u64,
    reply: oneshot::Sender<Result<EaResponse, TerminalError>>,
}

struct ActiveSession {
    generation: u64,
    out_tx: mpsc::Sender<Outbound>,
    /// Asked to stop when a newer EA session takes over.
    shutdown: mpsc::Sender<()>,
}

struct PendingRequest {
    method: String,
    deadline: Instant,
    reply: Option<oneshot::Sender<Result<EaResponse, TerminalError>>>,
    accumulated: EaResponse,
    got_response: bool,
    expecting_items: bool,
}

impl PendingRequest {
    fn complete(&mut self, value: Result<EaResponse, TerminalError>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(value);
        }
    }
}

pub struct EaLink {
    cfg: Arc<BridgeConfig>,
    session: Arc<RwLock<Option<ActiveSession>>>,
    state: Arc<RwLock<LinkState>>,
    events: broadcast::Sender<LinkEvent>,
    next_id: AtomicU64,
    next_generation: AtomicU64,
}

impl EaLink {
    /// Bind the EA listener and start accepting terminal connections.
    ///
    /// Uses `cfg.ea_bind_addr` / `cfg.ea_port`; port `0` asks the OS for an
    /// ephemeral port (tests).
    pub async fn bind(
        cfg: Arc<BridgeConfig>,
    ) -> std::io::Result<(Arc<EaLink>, std::net::SocketAddr)> {
        let listener = TcpListener::bind((cfg.ea_bind_addr.as_str(), cfg.ea_port)).await?;
        let addr = listener.local_addr()?;
        let (events, _) = broadcast::channel(256);

        let link = Arc::new(EaLink {
            cfg: cfg.clone(),
            session: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(LinkState::default())),
            events,
            next_id: AtomicU64::new(1),
            next_generation: AtomicU64::new(1),
        });

        let accept_link = link.clone();
        tokio::spawn(async move {
            if accept_link.cfg.ea_write_enabled() {
                info!(
                    "EA link listening on {}:{} (writes enabled)",
                    accept_link.cfg.ea_bind_addr, accept_link.cfg.ea_port
                );
            } else {
                warn!(
                    "EA link listening on {}:{} — MT5_EA_TOKEN is unset, so the bridge is \
                     READ-ONLY and refuses order/modify/close methods",
                    accept_link.cfg.ea_bind_addr, accept_link.cfg.ea_port
                );
            }
            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        debug!("EA connection from {peer}");
                        let link = accept_link.clone();
                        tokio::spawn(async move {
                            link.run_session(stream).await;
                        });
                    }
                    Err(err) => {
                        warn!("EA accept error: {err}");
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                }
            }
        });

        Ok((link, addr))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LinkEvent> {
        self.events.subscribe()
    }

    pub async fn state(&self) -> LinkState {
        self.state.read().await.clone()
    }

    pub async fn connected(&self) -> bool {
        self.session.read().await.is_some()
    }

    /// One EA connection: greeting, then the request/response pump.
    async fn run_session(self: Arc<Self>, stream: TcpStream) {
        let (read_half, mut write_half) = tokio::io::split(stream);

        // Reads happen in their own task. tokio's `AsyncBufReadExt` has
        // `read_line` (borrowing) rather than a futures-style owned
        // `next_line`, and a dedicated reader keeps the pump loop below free of
        // borrow juggling. The task ends when the socket closes or when this
        // session drops the receiver, and the guard aborts it on early returns.
        let (line_tx, mut line_rx) = mpsc::channel::<Result<String, String>>(64);
        let reader_task = tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut buffer = String::new();
            loop {
                buffer.clear();
                match reader.read_line(&mut buffer).await {
                    Ok(0) => {
                        let _ = line_tx.send(Err("EA closed the connection".into())).await;
                        break;
                    }
                    Ok(_) => {
                        let line = buffer.trim_end().to_string();
                        if line_tx.send(Ok(line)).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = line_tx.send(Err(format!("EA read error: {err}"))).await;
                        break;
                    }
                }
            }
        });
        let _reader_guard = AbortOnDrop(reader_task);

        // ---- 1. Greeting ---------------------------------------------------
        let greeting = match tokio::time::timeout(
            Duration::from_millis(self.cfg.request_timeout_ms.max(1_000)),
            line_rx.recv(),
        )
        .await
        {
            Ok(Some(Ok(line))) => line,
            Ok(Some(Err(err))) => {
                debug!("EA greeting failed: {err}");
                return;
            }
            Ok(None) => return,
            Err(_) => {
                debug!("EA connection did not send HELLO in time");
                return;
            }
        };

        let Some(message) = proto::parse_line(&greeting) else {
            return;
        };
        if message.kind != EaKind::Hello {
            warn!(
                "EA sent '{}' as its first frame (expected HELLO) — closing",
                message.name
            );
            return;
        }

        let presented = message.get_str("token").unwrap_or_default();
        let token_ok = match self.cfg.ea_token.as_deref() {
            Some(expected) => !expected.is_empty() && presented == expected,
            None => false,
        };
        if self.cfg.ea_token.is_some() && !token_ok {
            warn!("EA presented a wrong/missing MT5_EA_TOKEN — refusing the connection");
            let _ = write_half
                .write_all(proto::encode_hello_reply(false, "bad_token").as_bytes())
                .await;
            let _ = write_half.flush().await;
            return;
        }
        let write_enabled = self.cfg.ea_write_enabled() && token_ok;
        let _ = write_half
            .write_all(proto::encode_hello_reply(true, "").as_bytes())
            .await;
        let _ = write_half.flush().await;

        let hello_info = HelloInfo {
            login: message.get_i64("login").unwrap_or(0),
            server: message.get_str("server").unwrap_or_default(),
            company: message.get_str("company").unwrap_or_default(),
            currency: message.get_str("currency").unwrap_or_else(|| "USD".into()),
            mode: message.get_str("mode").unwrap_or_else(|| "unknown".into()),
            build: message.get_i64("build").unwrap_or(0),
            ea_version: message.get_str("ea").unwrap_or_default(),
            token_ok,
            connected_at: now_ms(),
        };

        info!(
            "EA connected: login {} on {} ({}) mode={} build={} ea={} writes={}",
            hello_info.login,
            hello_info.server,
            hello_info.company,
            hello_info.mode,
            hello_info.build,
            hello_info.ea_version,
            if write_enabled { "enabled" } else { "read-only" }
        );

        // ---- 2. Register (replacing any previous session) ------------------
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        let (out_tx, mut out_rx) = mpsc::channel::<Outbound>(64);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let previous = {
            let mut guard = self.session.write().await;
            guard.replace(ActiveSession {
                generation,
                out_tx,
                shutdown: shutdown_tx,
            })
        };
        if let Some(previous) = previous {
            warn!("a new EA connection replaced the previous session");
            let _ = previous.shutdown.try_send(());
        }
        {
            let mut state = self.state.write().await;
            state.hello = Some(hello_info.clone());
            state.connected = true;
            state.write_enabled = write_enabled;
            state.connected_at = Some(now_ms());
            state.last_error = None;
        }
        let _ = self
            .events
            .send(LinkEvent::Connected(Box::new(hello_info.clone())));

        // ---- 3. Pump -------------------------------------------------------
        let mut pending: HashMap<u64, PendingRequest> = HashMap::new();
        let mut reap = tokio::time::interval(Duration::from_millis(100));
        reap.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let reason: String = loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    break "replaced by a newer EA session".to_string();
                }
                line = line_rx.recv() => {
                    match line {
                        Some(Ok(text)) => {
                            if let Some(msg) = proto::parse_line(&text) {
                                self.handle_ea_message(msg, &mut pending).await;
                            }
                        }
                        Some(Err(reason)) => break reason,
                        None => break "EA reader stopped".to_string(),
                    }
                }
                request = out_rx.recv() => {
                    match request {
                        Some(out) => {
                            pending.insert(
                                out.id,
                                PendingRequest {
                                    method: out.method.clone(),
                                    deadline: Instant::now()
                                        + Duration::from_millis(out.timeout_ms.max(1)),
                                    reply: Some(out.reply),
                                    accumulated: EaResponse::default(),
                                    got_response: false,
                                    expecting_items: false,
                                },
                            );
                            if let Err(err) =
                                write_half.write_all(format!("{}\n", out.line).as_bytes()).await
                            {
                                if let Some(entry) = pending.get_mut(&out.id) {
                                    entry.complete(Err(TerminalError::Transport(format!(
                                        "EA write failed: {err}"
                                    ))));
                                }
                                break format!("EA write error: {err}");
                            }
                            let _ = write_half.flush().await;
                        }
                        None => break "bridge shut the EA link down".to_string(),
                    }
                }
                _ = reap.tick() => {
                    let now = Instant::now();
                    let expired: Vec<u64> = pending
                        .iter()
                        .filter(|(_, p)| p.deadline <= now)
                        .map(|(id, _)| *id)
                        .collect();
                    for id in expired {
                        if let Some(mut entry) = pending.remove(&id) {
                            let method = entry.method.clone();
                            let timeout_ms = entry
                                .deadline
                                .saturating_duration_since(Instant::now())
                                .as_millis() as u64;
                            entry.complete(Err(TerminalError::Timeout {
                                method,
                                timeout_ms,
                            }));
                        }
                    }
                }
            }
        };

        warn!("EA session ended: {reason}");

        // Fail every in-flight request and release the session slot if it is
        // still ours.
        for (_, mut entry) in pending.drain() {
            entry.complete(Err(TerminalError::NotConnected(format!(
                "EA session ended: {reason}"
            ))));
        }
        {
            let mut guard = self.session.write().await;
            if guard.as_ref().map(|s| s.generation) == Some(generation) {
                *guard = None;
            }
        }
        {
            let mut state = self.state.write().await;
            state.connected = false;
            state.write_enabled = false;
            state.disconnected_at = Some(now_ms());
            state.last_error = Some(reason.clone());
        }
        let _ = self.events.send(LinkEvent::Disconnected(reason));
    }

    async fn handle_ea_message(
        &self,
        message: EaMessage,
        pending: &mut HashMap<u64, PendingRequest>,
    ) {
        match message.kind {
            EaKind::Resp => {
                let Some(id) = message.id else { return };
                let is_error = message.name.eq_ignore_ascii_case("ERR");
                if is_error {
                    if let Some(mut entry) = pending.remove(&id) {
                        let code = message.get_i64("code").unwrap_or(0);
                        let text = message
                            .get_str("msg")
                            .unwrap_or_else(|| "EA rejected the request".into());
                        entry.complete(Err(TerminalError::Ea {
                            code,
                            message: text,
                        }));
                    }
                    return;
                }
                let declared_items = message.get_u64("count").unwrap_or(0);
                if let Some(entry) = pending.get_mut(&id) {
                    entry.accumulated.fields = message.fields.clone();
                    entry.got_response = true;
                    entry.expecting_items = declared_items > 0;
                    if !entry.expecting_items {
                        entry.complete(Ok(entry.accumulated.clone()));
                    }
                }
            }
            EaKind::Item => {
                if let Some(id) = message.id {
                    if let Some(entry) = pending.get_mut(&id) {
                        entry.accumulated.items.push(message.fields);
                    }
                }
            }
            EaKind::End => {
                if let Some(id) = message.id {
                    if let Some(mut entry) = pending.remove(&id) {
                        entry.expecting_items = false;
                        let value = entry.accumulated.clone();
                        entry.complete(Ok(value));
                    }
                }
            }
            EaKind::Hb => {
                let hb = HeartbeatInfo {
                    login: message.get_i64("login").unwrap_or(0),
                    mode: message
                        .get_str("mode")
                        .unwrap_or_else(|| "unknown".into()),
                    connected: message.get_bool("connected").unwrap_or(false),
                    trade_allowed: message.get_bool("trade_allowed").unwrap_or(false),
                    ea_ts: message.get_i64("ts").unwrap_or(0),
                    received_at: now_ms(),
                };
                {
                    let mut state = self.state.write().await;
                    state.last_heartbeat = Some(hb.clone());
                }
                let _ = self.events.send(LinkEvent::Heartbeat(Box::new(hb)));
            }
            EaKind::Evt => {
                debug!("EA event: {} {:?}", message.name, message.fields);
                let _ = self.events.send(LinkEvent::TradeEvent(message.fields));
            }
            EaKind::Hello => {
                warn!("unexpected second HELLO from the EA — ignoring");
            }
            EaKind::Unknown => {
                debug!("ignoring unrecognised EA line: {}", message.name);
            }
        }
    }
}

impl TerminalTransport for EaLink {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: &'a [(&'a str, String)],
        timeout_ms: u64,
    ) -> BoxFut<'a, Result<EaResponse, TerminalError>> {
        Box::pin(async move {
            let session = {
                let guard = self.session.read().await;
                match guard.as_ref() {
                    Some(session) => session.out_tx.clone(),
                    None => {
                        return Err(TerminalError::NotConnected(
                            "no EA session is connected to the bridge".into(),
                        ))
                    }
                }
            };

            if is_write_method(method) {
                let write_enabled = self.state.read().await.write_enabled;
                if !write_enabled {
                    return Err(TerminalError::Guard(format!(
                        "{method} refused: the EA link is read-only — set MT5_EA_TOKEN to the \
                         same value in the bridge environment and in the EA inputs"
                    )));
                }
            }

            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            let line = proto::encode_req(id, method, params);
            let (reply_tx, reply_rx) = oneshot::channel();
            session
                .send(Outbound {
                    id,
                    line,
                    method: method.to_string(),
                    timeout_ms,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| {
                    TerminalError::NotConnected("EA session ended while sending".into())
                })?;

            match tokio::time::timeout(Duration::from_millis(timeout_ms + 1_000), reply_rx).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => Err(TerminalError::NotConnected(
                    "EA session dropped the request".into(),
                )),
                Err(_) => Err(TerminalError::Timeout {
                    method: method.to_string(),
                    timeout_ms,
                }),
            }
        })
    }

    fn describe(&self) -> String {
        format!("ea-link {}:{}", self.cfg.ea_bind_addr, self.cfg.ea_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn write_methods_are_recognised_case_insensitively() {
        assert!(is_write_method("ORDER_SEND"));
        assert!(is_write_method("order_send"));
        assert!(is_write_method("POS_CLOSE"));
        assert!(is_write_method("CLOSE_ALL"));
        assert!(is_write_method("POS_MODIFY"));
        assert!(is_write_method("ORDER_CANCEL"));
        assert!(!is_write_method("ACCOUNT"));
        assert!(!is_write_method("POSITIONS"));
        assert!(!is_write_method("FIND"));
    }

    #[test]
    fn link_state_reports_mode_and_heartbeat_age() {
        let mut state = LinkState::default();
        assert_eq!(state.reported_mode(), None);
        state.hello = Some(HelloInfo {
            login: 1,
            server: "Deriv-Demo".into(),
            company: "Deriv".into(),
            currency: "USD".into(),
            mode: "demo".into(),
            build: 4755,
            ea_version: "1.0.0".into(),
            token_ok: true,
            connected_at: 0,
        });
        assert_eq!(state.reported_mode().as_deref(), Some("demo"));

        // A heartbeat overrides the greeting mode: that is how a mid-session
        // demo→real change is caught without waiting for a reconnect.
        state.last_heartbeat = Some(HeartbeatInfo {
            login: 1,
            mode: "real".into(),
            connected: true,
            trade_allowed: true,
            ea_ts: 1_000,
            received_at: 1_000,
        });
        assert_eq!(state.reported_mode().as_deref(), Some("real"));
        assert_eq!(state.heartbeat_age_ms(1_500), Some(500));
    }

    #[test]
    fn pending_requests_complete_exactly_once() {
        let (tx, mut rx) = oneshot::channel();
        let mut pending = PendingRequest {
            method: "ACCOUNT".into(),
            deadline: Instant::now() + Duration::from_secs(1),
            reply: Some(tx),
            accumulated: EaResponse::default(),
            got_response: false,
            expecting_items: false,
        };
        pending.complete(Ok(EaResponse::default()));
        pending.complete(Err(TerminalError::Timeout {
            method: "ACCOUNT".into(),
            timeout_ms: 1,
        }));
        let value = rx.try_recv().expect("first completion must be delivered");
        assert!(value.is_ok());
        assert!(rx.try_recv().is_err(), "second completion must be dropped");
    }

    #[test]
    fn list_responses_accumulate_items_until_end() {
        // Mirrors the dispatch logic without needing a socket.
        let mut accumulated = EaResponse::default();
        let resp_fields: BTreeMap<String, String> =
            [("count".to_string(), "2".to_string())].into_iter().collect();
        accumulated.fields = resp_fields;
        let expected: usize = accumulated
            .fields
            .get("count")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        assert_eq!(expected, 2);
        accumulated.items.push(
            [("ticket".to_string(), "1".to_string())]
                .into_iter()
                .collect(),
        );
        accumulated.items.push(
            [("ticket".to_string(), "2".to_string())]
                .into_iter()
                .collect(),
        );
        assert_eq!(accumulated.items.len(), expected);
    }
}
