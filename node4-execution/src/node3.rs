//! Outbound execution link to Node 3 (the strategy service).
//!
//! Node 4 is the **client** of `NODE3_WS_URL` (`wss://<node3-host>/execution`).
//! It authenticates with `execution_hello` and refuses to accept intents until
//! Node 3 answers `execution_hello_ack { accepted: true }`. The link speaks
//! protocol version 1 (see `docs/NODE3_NODE4_PROTOCOL.md`).
//!
//! Responsibilities:
//!
//! 1. Bounded reconnect backoff — never a hot loop.
//! 2. Intent ingest with the durable idempotency ledger: a repeated
//!    `intent_id` replays the recorded result and can never place a second
//!    order.
//! 3. Report outbox with acknowledgement tracking: `accepted` is only sent
//!    after the intent is durably recorded, and an unacknowledged report is
//!    retried (reports are idempotent by `intent_id`), across reconnects.
//! 4. Reconciliation of unclear broker writes by `intent_id` — never by
//!    resending the order.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::{Config, ExecutionVenue};
use crate::diagnostics::DiagnosticsHub;
use crate::execution::ExecutionManager;
use crate::intent::{
    ExecutionReport, ExecutionStatus, IntentRejection, Node3Frame, Node4Frame, TradeIntent,
    SERVICE_NAME,
};
use crate::ledger::{ExecutionLedger, IntentLedgerState, LedgerRecord};

/// Heartbeat cadence on the Node 3 socket.
const HEARTBEAT_SECS: u64 = 20;
/// How long a report may stay unacknowledged before it is retried.
const REPORT_ACK_TIMEOUT_MS: i64 = 15_000;
/// Cap on report retries; reports stay in the ledger regardless.
const MAX_REPORT_ATTEMPTS: u32 = 5;
/// Delays before each reconciliation attempt of an `unknown` outcome.
const RECONCILE_DELAYS_MS: [i64; 3] = [5_000, 15_000, 45_000];
/// How often the retry/ack sweep runs.
const SWEEP_INTERVAL_SECS: u64 = 5;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// A report waiting for `execution_report_ack`.
#[derive(Debug, Clone)]
struct PendingReport {
    text: String,
    attempts: u32,
    last_sent_ms: i64,
}

/// A report on its way to the socket.
#[derive(Debug, Clone)]
struct Outgoing {
    text: String,
}

#[derive(Clone)]
struct LinkShared {
    config: Config,
    hub: DiagnosticsHub,
    manager: Arc<ExecutionManager>,
    ledger: Arc<ExecutionLedger>,
    /// Serialises broker writes: at most one order in flight at a time.
    order_gate: Arc<tokio::sync::Mutex<()>>,
    /// Reports awaiting `execution_report_ack`, surviving reconnects.
    pending: Arc<std::sync::Mutex<HashMap<String, PendingReport>>>,
    /// Timestamp of the last frame received from Node 3.
    last_frame_ms: Arc<AtomicI64>,
}

impl LinkShared {
    fn venue_label(&self) -> &'static str {
        self.manager.venue_label()
    }

    /// True when a frame arrived recently enough to trust the link.
    fn link_is_fresh(&self) -> bool {
        let last = self.last_frame_ms.load(Ordering::SeqCst);
        last > 0 && now_ms() - last <= self.config.node3_stale_ms
    }
}

/// Run the link forever, reconnecting with bounded exponential backoff.
pub async fn run(
    config: Config,
    hub: DiagnosticsHub,
    manager: Arc<ExecutionManager>,
    ledger: Arc<ExecutionLedger>,
) {
    let shared = LinkShared {
        config: config.clone(),
        hub,
        manager,
        ledger,
        order_gate: Arc::new(tokio::sync::Mutex::new(())),
        pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
        last_frame_ms: Arc::new(AtomicI64::new(0)),
    };

    let mut backoff = config.node3_reconnect_min_ms.max(100);
    loop {
        match connect_once(&shared).await {
            Ok(()) => {
                // A clean close still means the session ended; reconnect fast.
                backoff = config.node3_reconnect_min_ms.max(100);
            }
            Err(err) => {
                warn!("Node 3 link ended: {err}");
            }
        }
        shared
            .hub
            .set_node3_state(false, false, "reconnecting")
            .await;
        let delay = backoff;
        backoff = backoff
            .saturating_mul(2)
            .min(config.node3_reconnect_max_ms.max(1_000))
            .max(100);
        info!("reconnecting to Node 3 in {delay} ms");
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

/// One connection lifetime: connect, handshake, then pump frames until the
/// socket dies.
async fn connect_once(shared: &LinkShared) -> Result<(), String> {
    let url = shared.config.node3_ws_url.trim().to_string();
    if url.is_empty() {
        return Err("NODE3_WS_URL is not set".into());
    }
    let Some(token) = shared.config.node4_shared_token.clone() else {
        return Err("NODE4_SHARED_TOKEN is not set".into());
    };

    shared.hub.set_node3_state(false, false, "connecting").await;

    let (ws, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|err| format!("cannot connect to {url}: {err}"))?;
    let (mut sender, mut receiver) = ws.split();

    let hello = Node4Frame::ExecutionHello {
        token: &token,
        service: SERVICE_NAME,
        protocol_version: shared.config.protocol_version,
    };
    sender
        .send(Message::Text(hello.to_text().into()))
        .await
        .map_err(|err| format!("cannot send execution_hello: {err}"))?;

    wait_for_hello_ack(shared, &mut receiver).await?;

    shared.hub.set_node3_state(true, true, "connected").await;
    shared
        .hub
        .record_node3_hello_ack(Some(shared.config.protocol_version), true)
        .await;
    info!("Node 3 execution link is up ({url})");

    let (outbox_tx, mut outbox_rx) = mpsc::channel::<Outgoing>(64);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));
    heartbeat.tick().await;
    let mut sweep = tokio::time::interval(Duration::from_secs(SWEEP_INTERVAL_SECS));
    sweep.tick().await;

    // Anything left unacknowledged from a previous connection is retried now.
    let resend = take_retryable(shared, true);
    for (intent_id, text) in resend {
        debug!("re-sending unacknowledged report for {intent_id}");
        if sender.send(Message::Text(text.into())).await.is_err() {
            return Err("socket closed while re-sending reports".into());
        }
    }

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let text = Node4Frame::Heartbeat.to_text();
                if sender.send(Message::Text(text.into())).await.is_err() {
                    return Err("socket closed while sending a heartbeat".into());
                }
            }
            _ = sweep.tick() => {
                for (intent_id, text) in take_retryable(shared, false) {
                    debug!("retrying unacknowledged report for {intent_id}");
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        return Err("socket closed while retrying reports".into());
                    }
                }
            }
            outgoing = outbox_rx.recv() => {
                match outgoing {
                    Some(report) => {
                        if sender.send(Message::Text(report.text.into())).await.is_err() {
                            return Err("socket closed while sending a report".into());
                        }
                    }
                    None => return Err("report channel closed".into()),
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        shared.last_frame_ms.store(now_ms(), Ordering::SeqCst);
                        shared.hub.record_node3_message().await;
                        match Node3Frame::parse(&text) {
                            Some(Node3Frame::TradeIntent { data }) => {
                                let shared_clone = shared.clone();
                                let outbox = outbox_tx.clone();
                                tokio::spawn(async move {
                                    handle_intent(shared_clone, outbox, data).await;
                                });
                            }
                            Some(Node3Frame::ExecutionReportAck { intent_id, accepted, error }) => {
                                let pending = shared
                                    .pending
                                    .lock()
                                    .map(|mut guard| guard.remove(&intent_id))
                                    .unwrap_or(None);
                                if pending.is_none() {
                                    debug!("report ack for an unknown intent {intent_id}");
                                }
                                if !accepted {
                                    warn!(
                                        "Node 3 rejected the report for {intent_id}: {}",
                                        error.unwrap_or_else(|| "no reason given".into())
                                    );
                                }
                                shared.hub.record_report_ack(accepted).await;
                            }
                            Some(Node3Frame::ExecutionHelloAck { .. }) => {
                                debug!("ignoring a duplicate execution_hello_ack");
                            }
                            Some(Node3Frame::Heartbeat) => {
                                let text = Node4Frame::Heartbeat.to_text();
                                if sender.send(Message::Text(text.into())).await.is_err() {
                                    return Err("socket closed while answering a heartbeat".into());
                                }
                            }
                            Some(Node3Frame::Unknown) | None => {
                                debug!("ignoring an unrecognised Node 3 frame");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            return Err("socket closed while answering a ping".into());
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return Err("Node 3 closed the execution socket".into());
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => return Err(format!("socket error: {err}")),
                }
            }
        }
    }
}

/// Wait for `execution_hello_ack { accepted: true }` within the configured
/// timeout. Anything else — a rejection, a timeout, a closed socket — aborts the
/// connection so the caller retries with backoff.
async fn wait_for_hello_ack<S>(shared: &LinkShared, receiver: &mut S) -> Result<(), String>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let deadline = std::time::Instant::now()
        + Duration::from_millis(shared.config.node3_hello_timeout_ms.max(1_000));
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for execution_hello_ack".into());
        }
        match tokio::time::timeout(remaining, receiver.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                shared.last_frame_ms.store(now_ms(), Ordering::SeqCst);
                shared.hub.record_node3_message().await;
                match Node3Frame::parse(&text) {
                    Some(Node3Frame::ExecutionHelloAck {
                        accepted,
                        protocol_version,
                        error,
                    }) => {
                        shared
                            .hub
                            .record_node3_hello_ack(protocol_version, accepted)
                            .await;
                        if accepted {
                            return Ok(());
                        }
                        return Err(format!(
                            "Node 3 refused execution_hello: {}",
                            error.unwrap_or_else(|| "no reason given".into())
                        ));
                    }
                    Some(Node3Frame::Heartbeat) => continue,
                    Some(_) => {
                        warn!("ignoring a frame that arrived before execution_hello_ack");
                        continue;
                    }
                    None => continue,
                }
            }
            Ok(Some(Ok(Message::Ping(payload)))) => {
                // The hello sender is not available here; the socket will be
                // re-established, which is cheaper than a half-authenticated
                // session.
                let _ = payload;
                continue;
            }
            Ok(Some(Ok(Message::Close(_))) | None) | Ok(Some(Err(_))) => {
                return Err("socket closed before execution_hello_ack".into())
            }
            Ok(Some(Ok(_))) => continue,
            Err(_) => return Err("timed out waiting for execution_hello_ack".into()),
        }
    }
}

/// Collect reports that need (re)sending. `on_connect` ignores the ack timeout
/// so everything unacknowledged is retried immediately after a reconnect.
fn take_retryable(shared: &LinkShared, on_connect: bool) -> Vec<(String, String)> {
    let Ok(mut pending) = shared.pending.lock() else {
        return Vec::new();
    };
    let now = now_ms();
    let mut out = Vec::new();
    for (intent_id, entry) in pending.iter_mut() {
        if entry.attempts >= MAX_REPORT_ATTEMPTS {
            continue;
        }
        let due = on_connect || now - entry.last_sent_ms >= REPORT_ACK_TIMEOUT_MS;
        if due {
            entry.attempts += 1;
            entry.last_sent_ms = now;
            out.push((intent_id.clone(), entry.text.clone()));
        }
    }
    out
}

/// Handle one validated-scope intent: validate, deduplicate durably, then place
/// at most one broker order.
async fn handle_intent(shared: LinkShared, outbox: mpsc::Sender<Outgoing>, intent: TradeIntent) {
    let received_at = now_ms();
    shared.hub.record_intent_received(&intent).await;

    // 1. Static validation. Node 4 may refuse an unsafe intent, but never
    //    rewrites the strategy's stop/target.
    if let Err(rejection) = intent.validate(&shared.config, received_at) {
        shared.hub.record_intent_rejected(&intent, &rejection).await;
        publish_report(
            &shared,
            &outbox,
            refusal_report(
                &intent,
                shared.venue_label(),
                &rejection.code,
                &rejection.message,
            ),
            false,
        )
        .await;
        return;
    }

    // 2. Exactly one venue, with its credentials present. A contradictory
    //    configuration is refused rather than resolved by guessing.
    if let Some(err) = shared.config.venue_selection_error() {
        let rejection = IntentRejection::new("venue_configuration_error", err);
        shared.hub.record_intent_rejected(&intent, &rejection).await;
        publish_report(
            &shared,
            &outbox,
            refusal_report(
                &intent,
                shared.venue_label(),
                &rejection.code,
                &rejection.message,
            ),
            false,
        )
        .await;
        return;
    }

    // 3. Duplicate: the recorded result is replayed, and no order is placed.
    if let Some(state) = shared.ledger.state(&intent.intent_id).await {
        if state.accepted {
            replay_duplicate(&shared, &outbox, &intent, &state).await;
            return;
        }
    }

    // 4. Fail closed when the ledger cannot record ownership. A real venue must
    //    never place an order it cannot deduplicate after a crash.
    if shared.manager.venue().is_real() && !shared.ledger.available() {
        let rejection = IntentRejection::new(
            "ledger_unavailable",
            format!(
                "EXECUTION_LEDGER_FILE is not writable ({}); refusing to trade a real venue \
                 without durable idempotency",
                shared
                    .ledger
                    .open_error()
                    .unwrap_or_else(|| shared.config.execution_ledger_file.clone())
            ),
        );
        shared.hub.record_intent_rejected(&intent, &rejection).await;
        publish_report(
            &shared,
            &outbox,
            refusal_report(
                &intent,
                shared.venue_label(),
                &rejection.code,
                &rejection.message,
            ),
            false,
        )
        .await;
        return;
    }

    // 5. Durably assume responsibility *before* any broker write, then tell
    //    Node 3 that the intent is owned (the `accepted` report). The `none`
    //    venue is an explicit dry-run, so the same report carries that fact
    //    instead of a second follow-up message.
    let accepted = if shared.manager.venue() == ExecutionVenue::None {
        ExecutionReport::signal_only(&intent, shared.venue_label(), now_ms())
    } else {
        ExecutionReport::accepted(&intent, shared.venue_label(), now_ms())
    };
    let record = LedgerRecord::IntentReceived {
        ts: now_ms(),
        intent: intent.clone(),
        report: accepted.clone(),
    };
    if let Err(err) = shared.ledger.append(&record).await {
        // Nothing was written to a broker, so this is a clean refusal.
        let rejection = IntentRejection::new("ledger_unavailable", err);
        shared.hub.record_intent_rejected(&intent, &rejection).await;
        publish_report(
            &shared,
            &outbox,
            refusal_report(
                &intent,
                shared.venue_label(),
                &rejection.code,
                &rejection.message,
            ),
            false,
        )
        .await;
        return;
    }
    shared.hub.record_intent_accepted(&intent).await;
    publish_report(&shared, &outbox, accepted, false).await;

    // 6. Explicit dry-run venue: nothing else to do. The intent was validated,
    //    durably recorded and reported; no broker order exists.
    if shared.manager.venue() == ExecutionVenue::None {
        return;
    }

    // 7. Broker write, serialised with every other write and recorded first so
    //    an unclear outcome always has a local trace.
    let broker_symbol = match shared.config.resolve_symbol(&intent.symbol) {
        Ok(symbol) => symbol,
        Err(err) => {
            let rejection = IntentRejection::new("unsupported_symbol", err);
            shared.hub.record_intent_rejected(&intent, &rejection).await;
            publish_report(
                &shared,
                &outbox,
                refusal_report(
                    &intent,
                    shared.venue_label(),
                    &rejection.code,
                    &rejection.message,
                ),
                true,
            )
            .await;
            return;
        }
    };

    let _gate = shared.order_gate.lock().await;

    if !shared.link_is_fresh() {
        // Defensive: an intent can only arrive over a live socket, but never
        // trade against a link state we cannot trust.
        let rejection = IntentRejection::new(
            "node3_link_stale",
            "the Node 3 link went stale before the order could be placed",
        );
        shared.hub.record_intent_rejected(&intent, &rejection).await;
        publish_report(
            &shared,
            &outbox,
            refusal_report(
                &intent,
                shared.venue_label(),
                &rejection.code,
                &rejection.message,
            ),
            true,
        )
        .await;
        return;
    }

    let command = LedgerRecord::BrokerCommand {
        ts: now_ms(),
        intent_id: intent.intent_id.clone(),
        venue: shared.venue_label().to_string(),
        command: "place_order".into(),
        idempotency_key: intent.intent_id.clone(),
        symbol: broker_symbol.clone(),
        side: intent.side.clone(),
        volume: shared.manager.planned_volume(),
        stop_loss: intent.stop_loss,
        take_profit: intent.take_profit,
    };
    if let Err(err) = shared.ledger.append(&command).await {
        let rejection = IntentRejection::new("ledger_unavailable", err);
        shared.hub.record_intent_rejected(&intent, &rejection).await;
        publish_report(
            &shared,
            &outbox,
            refusal_report(
                &intent,
                shared.venue_label(),
                &rejection.code,
                &rejection.message,
            ),
            true,
        )
        .await;
        return;
    }

    match shared.manager.execute(&intent, &broker_symbol).await {
        Ok(outcome) => {
            let mut report =
                ExecutionReport::new(&intent, outcome.status, shared.venue_label(), now_ms());
            report.execution_id = outcome.execution_id.clone();
            report.filled_price = outcome.filled_price;
            report.quantity = outcome.quantity;
            report.quantity_unit = outcome.quantity_unit.clone();
            report.order_ticket = outcome.order_ticket;
            report.deal_ticket = outcome.deal_ticket;
            report.position_ticket = outcome.position_ticket;
            report.retcode = outcome.retcode;
            if let Some(detail) = outcome.detail.clone() {
                report.error_message = Some(detail);
            }
            match outcome.status {
                ExecutionStatus::Unknown => {
                    report.error_code = Some("broker_outcome_unknown".into())
                }
                ExecutionStatus::Rejected => report.error_code = Some("broker_rejected".into()),
                _ => {}
            }

            // A broker-confirmed fill is the only thing that may open a trade.
            if outcome.status.is_fill() {
                let trade = shared.manager.trade_from_outcome(&intent, &outcome);
                shared.hub.record_trade_opened(trade).await;
            } else if outcome.status == ExecutionStatus::Unknown {
                shared
                    .hub
                    .record_order_failed(
                        "broker_outcome_unknown",
                        report
                            .error_message
                            .as_deref()
                            .unwrap_or("the broker did not confirm the outcome"),
                    )
                    .await;
            }

            publish_report(&shared, &outbox, report, true).await;

            if outcome.status == ExecutionStatus::Unknown {
                let shared_clone = shared.clone();
                let outbox_clone = outbox.clone();
                tokio::spawn(async move {
                    reconcile_unknown(shared_clone, outbox_clone, intent).await;
                });
            }
        }
        Err(refusal) => {
            shared
                .hub
                .record_order_failed(&refusal.code, &refusal.message)
                .await;
            publish_report(
                &shared,
                &outbox,
                refusal_report(
                    &intent,
                    shared.venue_label(),
                    &refusal.code,
                    &refusal.message,
                ),
                true,
            )
            .await;
        }
    }
}

/// Replay the recorded result of a repeated `intent_id`.
async fn replay_duplicate(
    shared: &LinkShared,
    outbox: &mpsc::Sender<Outgoing>,
    intent: &TradeIntent,
    state: &IntentLedgerState,
) {
    let recorded_status = state
        .last_status
        .map(|status| status.as_str())
        .unwrap_or("accepted");
    shared
        .hub
        .record_duplicate_intent(&intent.intent_id, Some(recorded_status))
        .await;
    let _ = shared
        .ledger
        .append(&LedgerRecord::Duplicate {
            ts: now_ms(),
            intent_id: intent.intent_id.clone(),
            recorded_status: recorded_status.to_string(),
        })
        .await;

    let report = match state.last_report.clone() {
        Some(mut report) => {
            report.timestamp = now_ms();
            report.duplicate = Some(true);
            report
        }
        None => {
            let mut report = ExecutionReport::accepted(intent, shared.venue_label(), now_ms());
            report.duplicate = Some(true);
            report
        }
    };
    // The recorded report already lives in the ledger: re-sending it back to
    // Node 3 is a replay, not a new decision.
    publish_report(shared, outbox, report, false).await;
}

/// Ask the venue to reconcile an unclear write, then publish the result.
///
/// A failed reconciliation leaves the recorded `unknown` result in place: Node 4
/// never turns an unclear write into a fresh order.
async fn reconcile_unknown(
    shared: LinkShared,
    outbox: mpsc::Sender<Outgoing>,
    intent: TradeIntent,
) {
    for delay in RECONCILE_DELAYS_MS {
        tokio::time::sleep(Duration::from_millis(delay as u64)).await;
        let Some(outcome) = shared.manager.reconcile(&intent).await else {
            continue;
        };
        if outcome.status == ExecutionStatus::Unknown {
            continue;
        }
        shared.hub.record_reconciled().await;
        let mut report =
            ExecutionReport::new(&intent, outcome.status, shared.venue_label(), now_ms());
        report.reconciled = Some(true);
        report.execution_id = outcome.execution_id.clone();
        report.filled_price = outcome.filled_price;
        report.quantity = outcome.quantity;
        report.position_ticket = outcome.position_ticket;
        report.error_message = Some(outcome.detail.clone());

        let _ = shared
            .ledger
            .append(&LedgerRecord::Reconciliation {
                ts: now_ms(),
                intent_id: intent.intent_id.clone(),
                venue: shared.venue_label().to_string(),
                source: outcome.source.clone(),
                detail: outcome.detail.clone(),
                report: report.clone(),
            })
            .await;
        info!(
            "reconciled intent {} as {} via {}",
            intent.intent_id,
            report.status.as_str(),
            outcome.source
        );
        // Persisted inside the reconciliation record above.
        publish_report(&shared, &outbox, report, false).await;
        return;
    }
    warn!(
        "intent {} stayed unknown after reconciliation attempts; the recorded 'unknown' report \
         stands and no retry will be attempted",
        intent.intent_id
    );
}

/// Report a pre-write refusal (`rejected`).
///
/// Anything that failed *before* a broker write is safe to report as rejected:
/// nothing unknown reached the broker, so nothing needs reconciling.
fn refusal_report(intent: &TradeIntent, venue: &str, code: &str, message: &str) -> ExecutionReport {
    ExecutionReport::rejected(intent, venue, now_ms(), code, message)
}

/// Persist (optionally) and enqueue one report for Node 3.
async fn publish_report(
    shared: &LinkShared,
    outbox: &mpsc::Sender<Outgoing>,
    report: ExecutionReport,
    persist: bool,
) {
    if persist {
        let record = LedgerRecord::Report {
            ts: report.timestamp,
            report: report.clone(),
        };
        if let Err(err) = shared.ledger.append(&record).await {
            warn!(
                "could not persist the {} report for {}: {err}",
                report.status.as_str(),
                report.intent_id
            );
        }
    }

    let Some(text) = report.to_frame() else {
        warn!("cannot serialise a report for {}", report.intent_id);
        return;
    };
    if let Ok(mut pending) = shared.pending.lock() {
        pending.insert(
            report.intent_id.clone(),
            PendingReport {
                text: text.clone(),
                attempts: 1,
                last_sent_ms: now_ms(),
            },
        );
    }
    if outbox.send(Outgoing { text }).await.is_err() {
        // The link is down; the ledger keeps the truth and the report is
        // re-sent on the next connection from the pending map.
        debug!(
            "report for {} queued locally while the Node 3 link is down",
            report.intent_id
        );
    }
    shared.hub.record_report(&report).await;
    shared.hub.record_report_sent().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn intent() -> TradeIntent {
        TradeIntent {
            schema_version: 1,
            intent_id: "n3-xauusd-1700000000000-buy-pw-poc".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2_650.25,
            stop_loss: 2_647.45,
            take_profit: 2_656.25,
            risk_reward: 2.0,
            level_name: "PW PoC".into(),
            source_candle_time: 1_700_000_000_000,
            created_at: 1_700_000_000_100,
            expires_at: 1_700_000_120_100,
        }
    }

    fn shared() -> LinkShared {
        let config = Config {
            node3_ws_url: "wss://strategy.example/execution".into(),
            node4_shared_token: Some("token".into()),
            ..Default::default()
        };
        let hub = DiagnosticsHub::new(&config);
        let manager = Arc::new(ExecutionManager::new(&config, hub.mt5()));
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let ledger = Arc::new(ExecutionLedger::open(std::env::temp_dir().join(format!(
            "node4-node3-test-{}-{seq}.jsonl",
            std::process::id()
        ))));
        LinkShared {
            config,
            hub,
            manager,
            ledger,
            order_gate: Arc::new(tokio::sync::Mutex::new(())),
            pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
            last_frame_ms: Arc::new(AtomicI64::new(0)),
        }
    }

    #[test]
    fn a_report_is_retried_only_after_the_ack_timeout() {
        let shared = shared();
        let fresh = PendingReport {
            text: "{}".into(),
            attempts: 1,
            last_sent_ms: now_ms(),
        };
        let stale = PendingReport {
            text: "{}".into(),
            attempts: 1,
            last_sent_ms: now_ms() - REPORT_ACK_TIMEOUT_MS - 1,
        };
        shared.pending.lock().unwrap().insert("fresh".into(), fresh);
        shared.pending.lock().unwrap().insert("stale".into(), stale);

        let retried = take_retryable(&shared, false);
        assert_eq!(retried.len(), 1);
        assert_eq!(retried[0].0, "stale");

        // After a reconnect everything unacknowledged is re-sent at once.
        let all = take_retryable(&shared, true);
        assert_eq!(all.len(), 2);

        // Retries stop at the cap; the ledger remains the source of truth.
        {
            let mut pending = shared.pending.lock().unwrap();
            if let Some(entry) = pending.get_mut("stale") {
                entry.attempts = MAX_REPORT_ATTEMPTS;
            }
        }
        let capped = take_retryable(&shared, true);
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0].0, "fresh");
    }

    #[test]
    fn link_freshness_uses_the_configured_stale_window() {
        let shared = shared();
        assert!(!shared.link_is_fresh(), "no frame has been seen yet");
        shared.last_frame_ms.store(now_ms(), Ordering::SeqCst);
        assert!(shared.link_is_fresh());
        shared
            .last_frame_ms
            .store(now_ms() - 60_000, Ordering::SeqCst);
        assert!(!shared.link_is_fresh(), "a minute of silence is stale");
    }

    #[tokio::test]
    async fn a_duplicate_intent_replays_the_recorded_result() {
        let shared = shared();
        let (tx, mut rx) = mpsc::channel::<Outgoing>(4);
        let intent = intent();

        let mut state = IntentLedgerState {
            accepted: true,
            last_status: Some(ExecutionStatus::Filled),
            ..Default::default()
        };
        let mut filled =
            ExecutionReport::new(&intent, ExecutionStatus::Filled, "deriv_mt5_demo", 5);
        filled.filled_price = Some(2_650.5);
        state.last_report = Some(filled.clone());

        replay_duplicate(&shared, &tx, &intent, &state).await;

        let outgoing = rx.recv().await.expect("a replay is sent");
        let value: serde_json::Value = serde_json::from_str(&outgoing.text).unwrap();
        assert_eq!(value["type"], "execution_report");
        assert_eq!(value["data"]["status"], "filled");
        assert_eq!(value["data"]["duplicate"], true);
        assert_eq!(value["data"]["intent_id"], intent.intent_id);

        // The replay did not place an order or send an `accepted` report.
        let work = shared.hub.snapshot().await.work;
        assert_eq!(work.orders_placed, 0);
        assert_eq!(work.intents_duplicate, 1);
    }

    #[test]
    fn refusals_become_rejected_reports_with_the_exact_reason() {
        let intent = intent();
        let refusal = IntentRejection::new("intent_expired", "expires_at is in the past");
        let report = refusal_report(&intent, "none", &refusal.code, &refusal.message);
        assert_eq!(report.status, ExecutionStatus::Rejected);
        assert_eq!(report.error_code.as_deref(), Some("intent_expired"));
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["venue"], "none");
        assert_eq!(value["symbol"], "XAUUSD");
        assert_eq!(value["side"], "buy");
    }
}
