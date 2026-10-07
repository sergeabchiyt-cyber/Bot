//! Node 4's outbound client of Node 3's authenticated execution endpoint.
//!
//! Transport (protocol version 1, see `docs/NODE3_NODE4_PROTOCOL.md`):
//!
//! * Node 4 dials out to `NODE3_WS_URL` (e.g. `wss://<node3-host>/execution`);
//! * the **first frame** is `execution_hello` carrying
//!   `NODE4_SHARED_TOKEN`; Node 4 only accepts intents after
//!   `execution_hello_ack.accepted == true`;
//! * reconnects use bounded exponential backoff (1s → 30s cap) and repeat
//!   the hello; Node 3 then replays still-valid pending intents, which is
//!   safe because Node 4 persists/deduplicates `intent_id`;
//! * either side may send `{"type":"heartbeat"}` and the peer replies in
//!   kind; a silent session is treated as stale and redialed.
//!
//! Reports are handed off through a bounded queue so an intent handler never
//! blocks the socket, and queued reports are delivered after a reconnect
//! instead of being lost.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::sync::{Notify, RwLock};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::{MaybeTlsStream, Message};
use tracing::{debug, error, info, warn};

use crate::config::{Config, EXECUTION_PROTOCOL_VERSION, SERVICE_NAME};
use crate::diagnostics::DiagnosticsHub;
use crate::types::{ExecutionReport, Node3Frame, Node4Frame, TradeIntent, now_ms};

/// Bounded reconnect backoff: 1s, 2s, 4s, … capped at 30s.
const RECONNECT_MIN_SECS: u64 = 1;
const RECONNECT_MAX_SECS: u64 = 30;
/// How long we wait for the socket to open.
const CONNECT_TIMEOUT_SECS: u64 = 20;
/// The hello must go out within five seconds of connecting (protocol).
const HELLO_DEADLINE_SECS: u64 = 5;
/// How long we wait for `execution_hello_ack`.
const HELLO_ACK_TIMEOUT_SECS: u64 = 10;
/// Application-level heartbeat cadence.
const HEARTBEAT_SECS: u64 = 20;
/// A session silent for this long is stale: no new broker writes are made
/// and the link is redialed (monitoring/controls keep running).
const STALE_AFTER_SECS: u64 = 60;
/// Bounded queue of reports waiting for the link.
const REPORT_QUEUE_CAPACITY: usize = 256;

type Node3Ws =
    WebSocketStream<MaybeTlsStream<std::net::TcpStream>>;

#[derive(Debug, Clone)]
pub struct LinkState {
    pub configured: bool,
    pub url: String,
    pub connected: bool,
    /// `not_configured` | `connecting` | `auth_rejected` | `connected` |
    /// `reconnecting` | `disconnected` | `stale`
    pub state: String,
    pub reconnect_count: u64,
    pub last_msg_ts: Option<i64>,
    pub last_error: Option<String>,
    pub reports_sent: u64,
    pub reports_acked: u64,
}

impl Default for LinkState {
    fn default() -> Self {
        Self {
            configured: false,
            url: String::new(),
            connected: false,
            state: "not_configured".into(),
            reconnect_count: 0,
            last_msg_ts: None,
            last_error: None,
            reports_sent: 0,
            reports_acked: 0,
        }
    }
}

/// Bounded FIFO of serialized reports waiting for the link to (re)connect.
#[derive(Default)]
pub struct ReportQueue {
    inner: tokio::sync::Mutex<VecDeque<String>>,
    notify: Notify,
    len: AtomicUsize,
}

impl ReportQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push one report; `Err` when the bounded queue is full (the caller
    /// records the loss — the ledger already holds the outcome, the report
    /// is operator-visible metadata).
    pub async fn push(&self, text: String) -> Result<(), String> {
        let mut guard = self.inner.lock().await;
        if guard.len() >= REPORT_QUEUE_CAPACITY {
            return Err(format!(
                "report queue full ({} reports waiting for the Node 3 link)",
                guard.len()
            ));
        }
        guard.push_back(text);
        self.len.fetch_add(1, Ordering::SeqCst);
        drop(guard);
        self.notify.notify_waiters();
        Ok(())
    }

    /// Pop all queued reports (the per-connection pump drains them in order).
    pub async fn drain(&self) -> Vec<String> {
        let mut guard = self.inner.lock().await;
        let drained = guard.drain(..).collect::<Vec<_>>();
        self.len
            .fetch_sub(drained.len(), Ordering::SeqCst);
        drained
    }

    /// Wait until at least one report is queued, then drain the whole batch.
    pub async fn next_batch(&self) -> Vec<String> {
        loop {
            let notified = self.notify.notified();
            if self.len.load(Ordering::SeqCst) > 0 {
                return self.drain().await;
            }
            notified.await;
        }
    }

    pub fn len(&self) -> usize {
        self.len.load(Ordering::SeqCst)
    }
}

/// Shared handle to the Node 3 link.
#[derive(Clone)]
pub struct Node3Link {
    state: Arc<RwLock<LinkState>>,
    /// Intents parsed off the wire, for the intent processor.
    pub intent_tx: mpsc::UnboundedSender<TradeIntent>,
    /// Reports to deliver back to Node 3.
    pub report_tx: mpsc::Sender<ExecutionReport>,
    queue: Arc<ReportQueue>,
    pub hub: DiagnosticsHub,
}

impl Node3Link {
    pub async fn snapshot(&self) -> LinkState {
        self.state.read().await.clone()
    }

    /// True only while authenticated: the gate every broker write must pass
    /// ("no new broker write after its Node 3 session is stale/disconnected").
    pub async fn accepts_broker_writes(&self) -> bool {
        self.state.read().await.state == "connected"
    }

    pub fn pending_reports(&self) -> usize {
        self.queue.len()
    }

    async fn set_state(&self, connected: bool, state: &str, last_error: Option<String>) {
        let should_count = {
            let mut guard = self.state.write().await;
            let should_count = !connected && state != "connecting";
            guard.connected = connected;
            guard.state = state.to_string();
            if should_count {
                guard.reconnect_count = guard.reconnect_count.saturating_add(1);
            }
            guard.last_error = last_error.clone();
            should_count
        };
        self.hub
            .set_node3_state(connected, state, should_count)
            .await;
    }

    pub async fn note_message(&self) {
        self.state.write().await.last_msg_ts = Some(now_ms());
        self.hub.note_node3_message().await;
    }

    async fn count_report(&self, field: &str) {
        let mut guard = self.state.write().await;
        if field == "sent" {
            guard.reports_sent = guard.reports_sent.saturating_add(1);
        } else {
            guard.reports_acked = guard.reports_acked.saturating_add(1);
        }
    }
}

/// Spawn the link (reader + reconnect loop) and the report writer.
///
/// Returns the link handle and the intent receive channel for the processor.
///
/// When `NODE3_WS_URL` is empty the service runs in monitor-only mode: the
/// link reports `not_configured` and no intents can arrive.
pub fn spawn(
    config: &Config,
    hub: DiagnosticsHub,
) -> (Node3Link, mpsc::UnboundedReceiver<TradeIntent>) {
    let (intent_tx, intent_rx) = mpsc::unbounded_channel::<TradeIntent>();
    let (report_tx, report_rx) = mpsc::channel::<ExecutionReport>(REPORT_QUEUE_CAPACITY);
    let queue = Arc::new(ReportQueue::new());

    let configured = config.node3_configured();
    let url = config.node3_ws_url.clone();
    let token = config.node4_shared_token.clone();

    let link = Node3Link {
        state: Arc::new(RwLock::new(LinkState {
            configured,
            url: url.clone(),
            state: if configured {
                "connecting".into()
            } else {
                "not_configured".into()
            },
            ..Default::default()
        })),
        intent_tx,
        report_tx,
        queue: queue.clone(),
        hub: hub.clone(),
    };

    if !configured {
        info!("NODE3_WS_URL not set — Node 3 intent consumption disabled (monitor-only mode)");
        tokio::spawn(report_writer(report_rx, queue, hub, false));
        return (link, intent_rx);
    }

    let token = token.expect("startup validation guarantees the token when the URL is set");
    tokio::spawn(link_loop(
        url,
        token,
        link.clone(),
        intent_rx,
        queue.clone(),
    ));
    tokio::spawn(report_writer(report_rx, queue, hub, true));
    (link, intent_rx)
}

/// Receive reports from the intent processor and queue them for delivery.
async fn report_writer(
    mut rx: mpsc::Receiver<ExecutionReport>,
    queue: Arc<ReportQueue>,
    hub: DiagnosticsHub,
    link_configured: bool,
) {
    while let Some(report) = rx.recv().await {
        let text = match serde_json::to_string(&Node4Frame::ExecutionReport {
            data: report.clone(),
        }) {
            Ok(text) => text,
            Err(err) => {
                error!("could not serialize execution report: {err}");
                continue;
            }
        };
        if !link_configured {
            warn!(
                "dropping execution report {} ({}): NODE3_WS_URL is not configured",
                report.intent_id, report.status
            );
            continue;
        }
        match queue.push(text).await {
            Ok(()) => {
                hub.on_report_queued(&report.intent_id, &report.status)
                    .await;
            }
            Err(err) => {
                error!(
                    "report {} ({}) lost: {err}",
                    report.intent_id, report.status
                );
                hub.on_report_dropped(&report.intent_id, &report.status)
                    .await;
            }
        }
    }
}

/// Reconnect loop; never returns.
async fn link_loop(
    url: String,
    token: String,
    link: Node3Link,
    mut intent_rx: mpsc::UnboundedReceiver<TradeIntent>,
    queue: Arc<ReportQueue>,
) {
    let mut backoff_secs = RECONNECT_MIN_SECS;
    loop {
        match run_once(&url, &token, &link, &mut intent_rx, &queue).await {
            Ok(()) => {
                warn!("Node 3 execution link closed — reconnecting in {backoff_secs}s");
                backoff_secs = RECONNECT_MIN_SECS;
            }
            Err(err) => {
                warn!("Node 3 execution link error: {err} — reconnecting in {backoff_secs}s");
                backoff_secs = (backoff_secs * 2).min(RECONNECT_MAX_SECS);
            }
        }
        sleep(Duration::from_secs(backoff_secs)).await;
    }
}

/// One connection: hello, ack, then the read loop.
async fn run_once(
    url: &str,
    token: &str,
    link: &Node3Link,
    intent_rx: &mut mpsc::UnboundedReceiver<TradeIntent>,
    queue: &Arc<ReportQueue>,
) -> anyhow::Result<()> {
    let first_attempt = {
        let state = link.state.read().await;
        state.reconnect_count == 0 && !state.connected
    };
    link.set_state(
        false,
        if first_attempt {
            "connecting"
        } else {
            "reconnecting"
        },
        None,
    )
    .await;

    let (mut ws, _response) = timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        tokio_tungstenite::connect_async(url),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out connecting to {url}"))?
    .map_err(|err| anyhow::anyhow!("handshake with {url} failed: {err}"))?;

    info!("connected to Node 3 execution endpoint at {url}");

    // First frame: execution_hello (within five seconds of connecting).
    let hello = Node4Frame::ExecutionHello {
        token: token.to_string(),
        service: SERVICE_NAME.into(),
        protocol_version: EXECUTION_PROTOCOL_VERSION,
    };
    let hello_text = serde_json::to_string(&hello)?;
    timeout(
        Duration::from_secs(HELLO_DEADLINE_SECS),
        ws.send(Message::Text(hello_text.into())),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out sending execution_hello"))?
    .map_err(|err| anyhow::anyhow!("failed to send execution_hello: {err}"))?;

    // Wait for execution_hello_ack; heartbeats are answered in kind.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(HELLO_ACK_TIMEOUT_SECS);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("Node 3 did not acknowledge execution_hello in time");
        }
        let msg = timeout(remaining, ws.next())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for execution_hello_ack"))?;
        let Some(msg) = msg else {
            anyhow::bail!("Node 3 closed the connection during the handshake");
        };
        match msg {
            Ok(Message::Text(text)) => {
                let parsed: Node3Frame =
                    serde_json::from_str(&text).unwrap_or(Node3Frame::Unknown);
                match parsed {
                    Node3Frame::ExecutionHelloAck {
                        accepted: true,
                        protocol_version,
                        ..
                    } => {
                        let mismatch = match protocol_version {
                            Some(version) if version != EXECUTION_PROTOCOL_VERSION => Some(version),
                            _ => None,
                        };
                        if let Some(version) = mismatch {
                            link.set_state(
                                false,
                                "auth_rejected",
                                Some(format!(
                                    "Node 3 speaks execution protocol {version}, this \
                                     service speaks {EXECUTION_PROTOCOL_VERSION} — \
                                     failing closed"
                                )),
                            )
                            .await;
                            anyhow::bail!("execution protocol version mismatch");
                        }
                        info!("Node 3 accepted the execution handshake (protocol {EXECUTION_PROTOCOL_VERSION})");
                        break;
                    }
                    Node3Frame::ExecutionHelloAck {
                        accepted: false,
                        error,
                        ..
                    } => {
                        let reason = error.unwrap_or_else(|| "unknown reason".into());
                        // `reason` is Node 3's non-secret diagnostic; the
                        // token never appears in logs.
                        link.set_state(false, "auth_rejected", Some(reason.clone()))
                            .await;
                        anyhow::bail!("Node 3 rejected the execution handshake: {reason}");
                    }
                    Node3Frame::Heartbeat => {
                        let _ = ws
                            .send(Message::Text(
                                serde_json::to_string(&Node4Frame::Heartbeat)
                                    .unwrap_or_else(|_| r#""heartbeat""#.to_string()),
                            ))
                            .await;
                    }
                    // Intents before the ack are not accepted by contract:
                    // drop them; Node 3 replays them on a valid session.
                    Node3Frame::TradeIntent { .. } => {
                        debug!("dropping trade_intent before hello ack (not accepted yet)");
                    }
                    _ => {}
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = ws.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(_)) => {
                anyhow::bail!("Node 3 closed the connection during the handshake");
            }
            Ok(_) => {}
            Err(err) => anyhow::bail!("Node 3 stream error during handshake: {err}"),
        }
    }

    link.set_state(true, "connected", None).await;
    link.note_message().await;

    let heartbeat_interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    heartbeat_interval.tick().await;
    let mut stale_sleep = sleep(Duration::from_secs(STALE_AFTER_SECS));

    let result: anyhow::Result<()> = loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                if let Ok(text) = serde_json::to_string(&Node4Frame::Heartbeat) {
                    if ws.send(Message::Text(text.into())).await.is_err() {
                        break Ok(());
                    }
                }
            }
            _ = &mut stale_sleep => {
                // No frame in STALE_AFTER_SECS: the session is stale.
                warn!("Node 3 session silent for {STALE_AFTER_SECS}s — treating as stale");
                link.set_state(false, "stale", None).await;
                break Ok(());
            }
            batch = queue.next_batch() => {
                // Deliver queued reports to the live socket, in order.
                for text in batch {
                    if ws.send(Message::Text(text.into())).await.is_err() {
                        break Ok(());
                    }
                    link.count_report("sent").await;
                }
            }
            msg = ws.next() => {
                let Some(msg) = msg else {
                    break Ok(());
                };
                match msg {
                    Ok(Message::Text(text)) => {
                        handle_incoming(&text, &mut ws, link, intent_rx).await;
                        // Any frame resets the stale deadline.
                        stale_sleep = sleep(Duration::from_secs(STALE_AFTER_SECS));
                    }
                    Ok(Message::Ping(payload)) => {
                        stale_sleep = sleep(Duration::from_secs(STALE_AFTER_SECS));
                        if ws.send(Message::Pong(payload)).await.is_err() {
                            break Ok(());
                        }
                    }
                    Ok(Message::Close(_)) => break Ok(()),
                    Ok(_) => {}
                    Err(err) => break Err(anyhow::anyhow!("stream error: {err}")),
                }
            }
        }
    };

    link.set_state(
        false,
        "disconnected",
        result.as_ref().err().map(|e| e.to_string()),
    )
    .await;
    result
}

/// Parse one frame from Node 3; intents go to the processor, everything else
/// updates link bookkeeping.
async fn handle_incoming(
    text: &str,
    ws: &mut Node3Ws,
    link: &Node3Link,
    _intent_rx: &mut mpsc::UnboundedReceiver<TradeIntent>,
) {
    let parsed: Node3Frame = match serde_json::from_str(text) {
        Ok(frame) => frame,
        Err(err) => {
            debug!("unparseable Node 3 frame: {err}");
            return;
        }
    };
    match parsed {
        Node3Frame::Heartbeat => {
            link.note_message().await;
            if let Ok(reply) = serde_json::to_string(&Node4Frame::Heartbeat) {
                if ws.send(Message::Text(reply.into())).await.is_err() {
                    // The socket is dying; the read loop will notice.
                }
            }
        }
        Node3Frame::TradeIntent { data } => {
            link.note_message().await;
            match serde_json::from_value::<TradeIntent>(data) {
                Ok(intent) => {
                    if link.intent_tx.send(intent).is_err() {
                        // Processor gone: the service is shutting down.
                        warn!("intent processor gone — intent dropped");
                    }
                }
                Err(err) => {
                    // A frame we cannot even parse is rejected; there is no
                    // intent_id to attach the error to, so it is logged
                    // (non-secret) and surfaced in diagnostics.
                    warn!("Node 3 sent a trade_intent that does not match the schema: {err}");
                    link.hub
                        .push_event(
                            "error",
                            "node3",
                            format!("trade_intent failed schema parse: {err}"),
                        )
                        .await;
                }
            }
        }
        Node3Frame::ExecutionReportAck {
            intent_id,
            accepted,
        } => {
            link.note_message().await;
            if !accepted {
                warn!(
                    "Node 3 rejected the execution report for intent {intent_id}"
                );
            }
            link.count_report("acked").await;
            link.hub.on_report_ack(&intent_id, accepted).await;
        }
        Node3Frame::ExecutionHelloAck { .. } => {
            // A second hello ack mid-session is not expected; ignore.
            debug!("ignoring a second execution_hello_ack");
        }
        Node3Frame::Unknown => {
            link.note_message().await;
            debug!("ignoring unrecognised Node 3 frame");
        }
    }
}
