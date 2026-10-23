//! Versioned Node 1 market-data consumer (feed contract v1, draft).
//!
//! Node 1 publishes a read-only market stream (`GET /ws`, JSON text frames).
//! The consumer is strict by design:
//!
//! * it subscribes to `candle`, `levels`, `tick_volume` and `status`;
//! * it accepts the contract only when `contract_version` equals the configured
//!   pin (`NODE1_CONTRACT_VERSION`, default `1.0`);
//! * every live data frame must carry the v1 `seq` envelope; `replay: true`
//!   frames (the post-subscribe burst) must not;
//! * a candle is tradable only when `closed == true`, and only once;
//! * a sequence gap, a gap frame, an out-of-order series or a malformed v1
//!   field forces a reconnect (a fresh replay), never a best-effort append;
//! * freshness comes from Node 1's `status.data.freshness`, and is re-checked
//!   against local silence and candle age on every trading decision.
//!
//! The module owns **no** strategy and no execution: it emits
//! [`FeedEvent`]s. `strategy_runner` turns a live closed candle into an
//! immutable intent; the existing execution pipeline decides what happens next.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::intent::IntentRejection;
use crate::strategy::{Candle, Level};
use crate::types::FeedSnapshot;

/// How often the local silence watchdog runs.
const WATCHDOG_SECS: u64 = 1;
/// Subscribe replay size documented by the contract.
#[allow(dead_code)]
const CONTRACT_REPLAY_CANDLES: u64 = 15;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// One event the strategy runner needs. Forming candles, tick-volume frames and
/// status frames update feed health only; they never become intent inputs.
#[derive(Debug, Clone)]
pub enum FeedEvent {
    ClosedCandle { candle: Candle, replay: bool },
    Levels { level: Level },
}

/// Fixed rules copied from `Config` once at startup.
#[derive(Debug, Clone)]
struct FeedRules {
    enabled: bool,
    symbol: String,
    interval: String,
    interval_ms: i64,
    contract_required: String,
    heartbeat_timeout_ms: i64,
    stale_ms: i64,
    max_candle_age_ms: i64,
    min_closed_candles: u64,
    auth: String,
}

/// Shared, cheap-to-clone feed handle. Diagnostics and the execution gate read
/// the same state the socket task writes.
#[derive(Clone)]
pub struct MarketFeed {
    inner: Arc<RwLock<FeedSnapshot>>,
    rules: Arc<FeedRules>,
}

impl MarketFeed {
    pub fn new(config: &Config) -> Self {
        let interval_ms = config.node1_interval_ms().unwrap_or(900_000);
        let auth = if config.node1_auth_configured() {
            "bearer".to_string()
        } else {
            "none (contract v1)".to_string()
        };
        let rules = Arc::new(FeedRules {
            enabled: config.node1_feed_enabled,
            symbol: config.node1_symbol.clone(),
            interval: config.node1_interval.clone(),
            interval_ms,
            contract_required: config.node1_contract_version.clone(),
            heartbeat_timeout_ms: config.node1_heartbeat_timeout_ms,
            stale_ms: config.node1_stale_ms,
            max_candle_age_ms: config.node1_max_candle_age_ms,
            min_closed_candles: config.node1_min_closed_candles,
            auth: auth.clone(),
        });
        let mut snapshot = FeedSnapshot {
            enabled: rules.enabled,
            url: config.node1_ws_url.clone(),
            auth,
            contract_required: rules.contract_required.clone(),
            symbol: rules.symbol.clone(),
            interval: rules.interval.clone(),
            state: if rules.enabled { "connecting" } else { "disabled" }.into(),
            not_ready_reason: if rules.enabled {
                Some("not connected".into())
            } else {
                Some("NODE1_FEED_ENABLED is not set".into())
            },
            ..Default::default()
        };
        snapshot.contract_ok = false;
        Self {
            inner: Arc::new(RwLock::new(snapshot)),
            rules,
        }
    }

    /// A consistent point-in-time view. `trading_ready` is recomputed here, so
    /// a socket that went silent while the operator was reading is reported as
    /// not ready immediately.
    pub fn snapshot(&self) -> FeedSnapshot {
        let mut snapshot = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let reason = self.not_ready_reason(&snapshot, now_ms());
        snapshot.trading_ready = reason.is_none();
        snapshot.not_ready_reason = reason;
        if snapshot.trading_ready {
            snapshot.state = "live".into();
        }
        snapshot
    }

    /// The execution gate. A local strategy intent is refused unless every
    /// contract, sequence, freshness and age condition still holds.
    pub fn gate_check(&self) -> Result<(), IntentRejection> {
        match self.snapshot().not_ready_reason {
            None => Ok(()),
            Some(reason) => Err(IntentRejection::new("market_data_not_ready", reason)),
        }
    }

    fn not_ready_reason(&self, snapshot: &FeedSnapshot, now: i64) -> Option<String> {
        if !self.rules.enabled {
            return Some("NODE1_FEED_ENABLED is not set".into());
        }
        if !snapshot.connected {
            return Some("market feed is not connected".into());
        }
        if !snapshot.contract_ok {
            return Some(match snapshot.contract_version.as_deref() {
                Some(version) => format!(
                    "contract_version '{version}' is not the required '{}'",
                    self.rules.contract_required
                ),
                None => "contract_version has not been received".into(),
            });
        }
        if !snapshot.sync_complete {
            return Some("post-subscribe replay has not completed".into());
        }
        if snapshot.session_closed_candles < self.rules.min_closed_candles {
            return Some(format!(
                "seeding: {} of {} closed candles observed",
                snapshot.session_closed_candles, self.rules.min_closed_candles
            ));
        }
        match snapshot.freshness_state.as_deref() {
            Some("live") => {}
            Some("closed_market") => return Some("market is closed".into()),
            Some("stale") => return Some("Node 1 freshness is stale".into()),
            Some("offline") => return Some("Node 1 feed is offline".into()),
            Some(other) => return Some(format!("Node 1 freshness state '{other}' is not live")),
            None => return Some("Node 1 freshness verdict has not been received".into()),
        }
        if snapshot.market_open != Some(true) {
            return Some("Node 1 reports market_open is not true".into());
        }
        if let Some(age) = snapshot.freshness_age_ms {
            if age > self.rules.stale_ms as u64 {
                return Some(format!(
                    "Node 1 freshness age {} ms exceeds {} ms",
                    age, self.rules.stale_ms
                ));
            }
        }
        let last_frame = snapshot.last_frame_ms.unwrap_or(0);
        if last_frame <= 0 {
            return Some("no frame has been received".into());
        }
        if now.saturating_sub(last_frame) > self.rules.heartbeat_timeout_ms {
            return Some(format!(
                "no Node 1 frame for {} ms (heartbeat timeout {} ms)",
                now.saturating_sub(last_frame),
                self.rules.heartbeat_timeout_ms
            ));
        }
        let Some(last_closed) = snapshot.last_closed_candle_time else {
            return Some("no closed candle has been received".into());
        };
        let close_at = last_closed.saturating_add(self.rules.interval_ms);
        if now.saturating_sub(close_at) > self.rules.max_candle_age_ms {
            return Some(format!(
                "last closed candle closed {} ms ago (max {} ms)",
                now.saturating_sub(close_at),
                self.rules.max_candle_age_ms
            ));
        }
        None
    }

    fn update(&self, change: impl FnOnce(&mut FeedSnapshot)) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        change(&mut guard);
    }

    fn begin_connect(&self) {
        self.update(|snapshot| {
            snapshot.state = "connecting".into();
            snapshot.connected = false;
            snapshot.sync_complete = false;
            snapshot.contract_ok = false;
            snapshot.contract_version = None;
            snapshot.last_seq = None;
            snapshot.session_closed_candles = 0;
            snapshot.last_closed_candle_time = None;
            snapshot.freshness_state = None;
            snapshot.freshness_age_ms = None;
            snapshot.market_open = None;
            snapshot.last_error = None;
        });
    }

    fn connected(&self) {
        self.update(|snapshot| {
            snapshot.connected = true;
            snapshot.state = "subscribing".into();
            snapshot.reconnect_count = snapshot.reconnect_count.saturating_add(1);
            snapshot.last_frame_ms = Some(now_ms());
        });
    }

    fn disconnected(&self) {
        self.update(|snapshot| {
            snapshot.connected = false;
            snapshot.state = "offline".into();
            snapshot.sync_complete = false;
            snapshot.contract_ok = false;
            snapshot.contract_version = None;
            snapshot.last_seq = None;
            snapshot.session_closed_candles = 0;
            snapshot.last_closed_candle_time = None;
        });
    }

    fn failed(&self, error: impl Into<String>) {
        let error = error.into();
        self.update(|snapshot| {
            snapshot.connected = false;
            snapshot.state = "offline".into();
            snapshot.last_error = Some(error);
            snapshot.last_event_ms = Some(now_ms());
        });
    }

    fn touch(&self) {
        let now = now_ms();
        self.update(|snapshot| {
            snapshot.last_frame_ms = Some(now);
            snapshot.frames_received = snapshot.frames_received.saturating_add(1);
            snapshot.last_event_ms = Some(now);
        });
    }

    fn silent_for_ms(&self) -> i64 {
        let last = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last_frame_ms
            .unwrap_or(0);
        if last <= 0 {
            i64::MAX
        } else {
            now_ms().saturating_sub(last)
        }
    }

    fn record_violation(&self, message: impl Into<String>) {
        let message = message.into();
        warn!("Node 1 feed contract violation: {message}");
        self.update(|snapshot| {
            snapshot.contract_violations = snapshot.contract_violations.saturating_add(1);
            snapshot.state = "contract_violation".into();
            snapshot.last_error = Some(message);
            snapshot.last_event_ms = Some(now_ms());
        });
    }

    fn record_ignored(&self) {
        self.update(|snapshot| {
            snapshot.last_event_ms = Some(now_ms());
        });
    }

    fn record_gap(&self, dropped: u64) {
        warn!("Node 1 reported a feed gap: {dropped} dropped frames; forcing a resync");
        self.update(|snapshot| {
            snapshot.gaps = snapshot.gaps.saturating_add(1);
            snapshot.resyncs = snapshot.resyncs.saturating_add(1);
            snapshot.state = "resyncing".into();
            snapshot.last_error = Some(format!("Node 1 reported a gap of {dropped} frames"));
            snapshot.last_event_ms = Some(now_ms());
        });
    }

    fn record_resync(&self, message: String) {
        self.update(|snapshot| {
            snapshot.resyncs = snapshot.resyncs.saturating_add(1);
            snapshot.state = "resyncing".into();
            snapshot.last_error = Some(message);
            snapshot.last_event_ms = Some(now_ms());
        });
    }

    fn record_status(
        &self,
        contract_version: &str,
        freshness: &WireFreshness,
    ) -> Result<(), String> {
        let contract_ok = contract_version == self.rules.contract_required;
        let mut error = None;
        self.update(|snapshot| {
            snapshot.status_frames = snapshot.status_frames.saturating_add(1);
            snapshot.last_status_ms = Some(now_ms());
            snapshot.contract_version = Some(contract_version.to_string());
            snapshot.contract_ok = contract_ok;
            snapshot.freshness_state = Some(freshness.state.clone());
            snapshot.freshness_age_ms = Some(freshness.age_ms);
            snapshot.market_open = Some(freshness.market_open);
            if !contract_ok {
                snapshot.state = "contract_mismatch".into();
                let message = format!(
                    "Node 1 contract_version '{contract_version}' is not the required '{}'",
                    self.rules.contract_required
                );
                snapshot.last_error = Some(message.clone());
                error = Some(message);
            } else {
                snapshot.last_error = None;
            }
        });
        match error {
            Some(message) => Err(message),
            None => Ok(()),
        }
    }

    fn record_closed_candle(&self, time: i64, replay: bool) {
        let min = self.rules.min_closed_candles;
        self.update(|snapshot| {
            snapshot.closed_candles = snapshot.closed_candles.saturating_add(1);
            snapshot.session_closed_candles = snapshot.session_closed_candles.saturating_add(1);
            snapshot.last_closed_candle_time = Some(time);
            snapshot.last_candle_time = Some(time);
            if !replay {
                snapshot.sync_complete = true;
            }
            let can_be_live = !replay
                && snapshot.contract_ok
                && snapshot.freshness_state.as_deref() == Some("live")
                && snapshot.market_open == Some(true)
                && snapshot.session_closed_candles >= min;
            if can_be_live {
                snapshot.state = "live".into();
            }
        });
    }

    fn record_levels(&self, level: &Level) {
        self.update(|snapshot| {
            snapshot.levels_received = snapshot.levels_received.saturating_add(1);
            snapshot.last_levels_ts = Some(level.timestamp);
            snapshot.last_event_ms = Some(now_ms());
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeqOutcome {
    Ok,
    Duplicate,
    Resync,
}

impl MarketFeed {
    /// v1 sequence rule: strictly increasing on live frames, absent on replay.
    fn observe_seq(&self, seq: Option<u64>, replay: bool, kind: &str) -> SeqOutcome {
        if replay {
            return SeqOutcome::Ok;
        }
        let Some(seq) = seq else {
            self.record_violation(format!(
                "{kind} frame is missing the v1 seq envelope"
            ));
            return SeqOutcome::Resync;
        };
        let mut outcome = SeqOutcome::Ok;
        self.update(|snapshot| {
            snapshot.sync_complete = true;
            match snapshot.last_seq {
                None => snapshot.last_seq = Some(seq),
                Some(last) if seq == last => {
                    snapshot.duplicates = snapshot.duplicates.saturating_add(1);
                    outcome = SeqOutcome::Duplicate;
                }
                Some(last) if seq < last => {
                    snapshot.out_of_order = snapshot.out_of_order.saturating_add(1);
                    snapshot.gaps = snapshot.gaps.saturating_add(1);
                    snapshot.state = "resyncing".into();
                    snapshot.last_error =
                        Some(format!("{kind} seq {seq} is older than {last}"));
                    outcome = SeqOutcome::Resync;
                }
                Some(last) if seq.saturating_sub(last) > 1 => {
                    snapshot.gaps = snapshot.gaps.saturating_add(1);
                    snapshot.state = "resyncing".into();
                    snapshot.last_error =
                        Some(format!("{kind} seq gap: {last} -> {seq}"));
                    outcome = SeqOutcome::Resync;
                }
                Some(_) => snapshot.last_seq = Some(seq),
            }
        });
        if outcome == SeqOutcome::Resync {
            self.record_resync(format!("{kind} frame broke the v1 sequence"));
        }
        outcome
    }
}

/// Run the feed forever with bounded exponential backoff.
pub async fn run(config: Config, feed: MarketFeed, tx: mpsc::Sender<FeedEvent>) {
    let mut backoff = config.node1_reconnect_min_ms.max(100);
    loop {
        feed.begin_connect();
        match connect_once(&config, &feed, &tx).await {
            Ok(()) => {
                info!("Node 1 market feed closed cleanly");
                backoff = config.node1_reconnect_min_ms.max(100);
            }
            Err(error) => {
                warn!("Node 1 market feed ended: {error}");
                feed.failed(error);
            }
        }
        feed.disconnected();
        let delay = backoff;
        backoff = backoff
            .saturating_mul(2)
            .min(config.node1_reconnect_max_ms.max(1_000))
            .max(100);
        info!("reconnecting to Node 1 in {delay} ms");
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

async fn connect_once(
    config: &Config,
    feed: &MarketFeed,
    tx: &mpsc::Sender<FeedEvent>,
) -> Result<(), String> {
    let mut request = config
        .node1_ws_url
        .trim()
        .into_client_request()
        .map_err(|error| format!("invalid NODE1_WS_URL: {error}"))?;
    if let Some(token) = config.node1_feed_token.as_deref() {
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|error| format!("NODE1_FEED_TOKEN is not a valid header value: {error}"))?;
        request.headers_mut().insert("authorization", value);
    }

    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|error| format!("cannot connect to {}: {error}", config.node1_ws_url))?;
    feed.connected();
    info!("Node 1 market feed connected ({})", config.node1_ws_url);

    let (mut sender, mut receiver) = ws.split();
    let subscription = json!({
        "type": "subscribe",
        "topics": ["candle", "levels", "tick_volume", "status"],
    });
    sender
        .send(Message::Text(subscription.to_string().into()))
        .await
        .map_err(|error| format!("cannot send the Node 1 subscribe frame: {error}"))?;

    let mut watchdog = interval(Duration::from_secs(WATCHDOG_SECS));
    watchdog.tick().await;

    loop {
        tokio::select! {
            _ = watchdog.tick() => {
                if feed.silent_for_ms() > config.node1_heartbeat_timeout_ms {
                    return Err(format!(
                        "no Node 1 frame for {} ms (heartbeat timeout {} ms)",
                        feed.silent_for_ms(),
                        config.node1_heartbeat_timeout_ms
                    ));
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        handle_text(&text, config, feed, tx).await?;
                    }
                    Some(Ok(Message::Binary(_))) => {
                        feed.record_violation("binary frame; the contract is JSON text only");
                        return Err("binary Node 1 frame".into());
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        feed.touch();
                        sender
                            .send(Message::Pong(payload))
                            .await
                            .map_err(|error| format!("socket closed while answering a ping: {error}"))?;
                    }
                    Some(Ok(Message::Pong(_))) => feed.touch(),
                    Some(Ok(Message::Close(_))) | None => {
                        return Err("Node 1 closed the market socket".into());
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(error)) => return Err(format!("Node 1 socket error: {error}")),
                }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct WireCandle {
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    #[serde(default)]
    #[allow(dead_code)]
    source: String,
    /// v1. `None` means the producer is still v0 and the feed is not usable.
    #[serde(default)]
    closed: Option<bool>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    interval: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireLevels {
    window: String,
    poc: f64,
    vah: f64,
    val: f64,
    #[serde(default)]
    start: i64,
    #[serde(default)]
    end: i64,
    #[serde(default)]
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
struct WireTickVolume {
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    interval: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireStatus {
    #[serde(default)]
    contract_version: Option<String>,
    #[serde(default)]
    freshness: Option<WireFreshness>,
}

#[derive(Debug, Deserialize)]
struct WireFreshness {
    state: String,
    #[serde(default)]
    age_ms: u64,
    #[serde(default)]
    market_open: bool,
}

#[derive(Debug, Deserialize)]
struct WireGap {
    #[serde(default)]
    dropped: u64,
}

async fn send_event(
    tx: &mpsc::Sender<FeedEvent>,
    event: FeedEvent,
) -> Result<(), String> {
    tx.send(event)
        .await
        .map_err(|_| "the strategy runner is not receiving feed events".to_string())
}

async fn handle_text(
    text: &str,
    config: &Config,
    feed: &MarketFeed,
    tx: &mpsc::Sender<FeedEvent>,
) -> Result<(), String> {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            feed.record_violation(format!("frame is not JSON: {error}"));
            return Ok(());
        }
    };
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    if kind.is_empty() {
        feed.record_violation("frame has no `type`");
        return Ok(());
    }
    let seq = value.get("seq").and_then(Value::as_u64);
    let replay = value.get("replay").and_then(Value::as_bool).unwrap_or(false);
    feed.touch();

    match kind {
        "candle" => handle_candle(&value, seq, replay, feed, tx).await,
        "levels" => handle_levels(&value, seq, replay, feed, tx).await,
        "tick_volume" => {
            match feed.observe_seq(seq, replay, kind) {
                SeqOutcome::Ok => {}
                SeqOutcome::Duplicate => return Ok(()),
                SeqOutcome::Resync => return Err("tick_volume sequence gap".into()),
            }
            let wire: WireTickVolume = match parse_data(&value) {
                Ok(wire) => wire,
                Err(error) => {
                    feed.record_violation(error);
                    return Ok(());
                }
            };
            // `closed` already exists in v0; the v1 identity fields are
            // additive. A producer that sends them must send them correctly.
            if let Some(symbol) = wire.symbol.as_deref() {
                if !symbol.eq_ignore_ascii_case(&feed.rules.symbol) {
                    feed.record_violation(format!(
                        "tick_volume symbol '{symbol}' is not '{}'",
                        feed.rules.symbol
                    ));
                    return Err("tick_volume symbol mismatch".into());
                }
            }
            if let Some(interval) = wire.interval.as_deref() {
                if interval != feed.rules.interval {
                    feed.record_violation(format!(
                        "tick_volume interval '{interval}' is not '{}'",
                        feed.rules.interval
                    ));
                    return Err("tick_volume interval mismatch".into());
                }
            }
            Ok(())
        }
        "status" => {
            match feed.observe_seq(seq, replay, kind) {
                SeqOutcome::Ok => {}
                SeqOutcome::Duplicate => return Ok(()),
                SeqOutcome::Resync => return Err("status sequence gap".into()),
            }
            let wire: WireStatus = match parse_data(&value) {
                Ok(wire) => wire,
                Err(error) => {
                    feed.record_violation(error);
                    return Ok(());
                }
            };
            let Some(version) = wire.contract_version.as_deref() else {
                feed.record_violation("status frame has no contract_version");
                return Ok(());
            };
            let Some(freshness) = wire.freshness.as_ref() else {
                feed.record_violation("status frame has no freshness verdict");
                return Ok(());
            };
            // A contract mismatch keeps the socket open (the producer may
            // deploy the right version) but the gate stays closed.
            let _ = feed.record_status(version, freshness);
            Ok(())
        }
        "heartbeat" => Ok(()),
        "gap" => {
            let gap: WireGap = value
                .get("data")
                .cloned()
                .and_then(|data| serde_json::from_value(data).ok())
                .unwrap_or(WireGap { dropped: 0 });
            feed.record_gap(gap.dropped);
            Err("Node 1 reported a sequence gap".into())
        }
        "strategy" | "demo" | "calendar" | "audio_chunk" | "learn" | "transcript"
        | "sentiment" | "health" | "prediction" | "sentiment_req" | "predict_req" => {
            // Out of scope for the execution feed, and explicitly not a reason
            // to trade or disconnect. Node 1's v1 contract removes the first
            // two; an older producer may still send them.
            feed.record_ignored();
            Ok(())
        }
        "trade_intent" | "execution_hello" | "execution_hello_ack" | "execution_report"
        | "execution_report_ack" | "trades" | "order" => {
            feed.record_violation(format!(
                "Node 1 published execution frame '{kind}'; the feed is not a trade channel"
            ));
            Err(format!("forbidden Node 1 frame '{kind}'"))
        }
        other => {
            debug!("ignoring an unrecognised Node 1 frame ({other})");
            feed.record_ignored();
            Ok(())
        }
    }
}

fn parse_data<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, String> {
    let data = value
        .get("data")
        .cloned()
        .ok_or_else(|| "frame has no `data`".to_string())?;
    serde_json::from_value(data).map_err(|error| format!("frame data does not match v1: {error}"))
}

async fn handle_candle(
    value: &Value,
    seq: Option<u64>,
    replay: bool,
    feed: &MarketFeed,
    tx: &mpsc::Sender<FeedEvent>,
) -> Result<(), String> {
    match feed.observe_seq(seq, replay, "candle") {
        SeqOutcome::Ok => {}
        SeqOutcome::Duplicate => return Ok(()),
        SeqOutcome::Resync => return Err("candle sequence gap".into()),
    }

    let wire: WireCandle = match parse_data(value) {
        Ok(wire) => wire,
        Err(error) => {
            feed.record_violation(error);
            return Ok(());
        }
    };

    let Some(closed) = wire.closed else {
        feed.record_violation("candle frame has no v1 `closed` flag");
        return Err("candle frame has no closed flag".into());
    };
    let Some(symbol) = wire.symbol.as_deref() else {
        feed.record_violation("candle frame has no v1 `symbol`");
        return Err("candle frame has no symbol".into());
    };
    if !symbol.eq_ignore_ascii_case(&feed.rules.symbol) {
        feed.record_violation(format!(
            "candle symbol '{symbol}' is not '{}'",
            feed.rules.symbol
        ));
        return Err("candle symbol mismatch".into());
    }
    let Some(interval) = wire.interval.as_deref() else {
        feed.record_violation("candle frame has no v1 `interval`");
        return Err("candle frame has no interval".into());
    };
    if interval != feed.rules.interval {
        feed.record_violation(format!(
            "candle interval '{interval}' is not '{}'",
            feed.rules.interval
        ));
        return Err("candle interval mismatch".into());
    }
    if wire.time <= 0 || wire.time % feed.rules.interval_ms != 0 {
        feed.record_violation(format!(
            "candle time {} is not aligned to {} ms",
            wire.time, feed.rules.interval_ms
        ));
        return Err("candle time is not aligned".into());
    }
    let candle = Candle {
        time: wire.time,
        open: wire.open,
        high: wire.high,
        low: wire.low,
        close: wire.close,
        volume: wire.volume,
    };
    if !candle.is_valid() {
        feed.record_violation(format!("candle at {} is not valid OHLCV", wire.time));
        return Err("invalid candle".into());
    }

    if !closed {
        feed.update(|snapshot| {
            snapshot.last_candle_time = Some(candle.time);
        });
        return Ok(());
    }

    let previous = feed
        .inner
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .last_closed_candle_time;
    if let Some(previous) = previous {
        if candle.time <= previous {
            if replay {
                feed.update(|snapshot| {
                    snapshot.duplicates = snapshot.duplicates.saturating_add(1);
                });
                return Ok(());
            }
            feed.record_violation(format!(
                "closed candle {} arrived after {} (out of order)",
                candle.time, previous
            ));
            return Err("out-of-order closed candle".into());
        }
    }

    feed.record_closed_candle(candle.time, replay);
    send_event(
        tx,
        FeedEvent::ClosedCandle {
            candle,
            replay,
        },
    )
    .await
}

async fn handle_levels(
    value: &Value,
    seq: Option<u64>,
    replay: bool,
    feed: &MarketFeed,
    tx: &mpsc::Sender<FeedEvent>,
) -> Result<(), String> {
    match feed.observe_seq(seq, replay, "levels") {
        SeqOutcome::Ok => {}
        SeqOutcome::Duplicate => return Ok(()),
        SeqOutcome::Resync => return Err("levels sequence gap".into()),
    }
    let wire: WireLevels = match parse_data(value) {
        Ok(wire) => wire,
        Err(error) => {
            feed.record_violation(error);
            return Ok(());
        }
    };
    if wire.window.trim().is_empty()
        || !wire.poc.is_finite()
        || !wire.vah.is_finite()
        || !wire.val.is_finite()
        || wire.poc <= 0.0
        || wire.vah <= 0.0
        || wire.val <= 0.0
    {
        feed.record_violation(format!("levels frame for '{}' is invalid", wire.window));
        return Ok(());
    }
    let level = Level {
        window: wire.window,
        poc: wire.poc,
        vah: wire.vah,
        val: wire.val,
        start: wire.start,
        end: wire.end,
        timestamp: wire.timestamp,
    };
    feed.record_levels(&level);
    send_event(tx, FeedEvent::Levels { level }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            intent_source: crate::config::IntentSource::Node1Strategy,
            node1_feed_enabled: true,
            node1_ws_url: "wss://engine.example/ws".into(),
            node1_min_closed_candles: 2,
            node1_heartbeat_timeout_ms: 75_000,
            node1_max_candle_age_ms: 1_800_000,
            ..Default::default()
        }
    }

    fn feed() -> (
        MarketFeed,
        mpsc::Sender<FeedEvent>,
        mpsc::Receiver<FeedEvent>,
    ) {
        let feed = MarketFeed::new(&config());
        let (tx, rx) = mpsc::channel(64);
        (feed, tx, rx)
    }

    fn aligned_now_bucket(interval_ms: i64) -> i64 {
        let now = chrono::Utc::now().timestamp_millis();
        (now / interval_ms) * interval_ms
    }

    fn status_json(seq: u64, state: &str, market_open: bool) -> String {
        json!({
            "type": "status",
            "seq": seq,
            "data": {
                "contract_version": "1.0",
                "freshness": { "state": state, "age_ms": 500, "market_open": market_open }
            }
        })
        .to_string()
    }

    fn candle_json(time: i64, seq: Option<u64>, replay: bool, closed: bool) -> String {
        let mut value = json!({
            "type": "candle",
            "data": {
                "time": time,
                "open": 2650.0,
                "high": 2652.0,
                "low": 2649.0,
                "close": 2651.0,
                "volume": 12000.0,
                "source": "sifting",
                "closed": closed,
                "symbol": "XAUUSD",
                "interval": "15m"
            }
        });
        if let Some(seq) = seq {
            value["seq"] = json!(seq);
        }
        if replay {
            value["replay"] = json!(true);
        }
        value.to_string()
    }

    fn levels_json(seq: u64) -> String {
        json!({
            "type": "levels",
            "seq": seq,
            "data": {
                "window": "PW", "poc": 2650.0, "vah": 2660.0, "val": 2640.0,
                "start": 1, "end": 2, "timestamp": 3
            }
        })
        .to_string()
    }

    async fn handle(
        feed: &MarketFeed,
        tx: &mpsc::Sender<FeedEvent>,
        text: &str,
    ) -> Result<(), String> {
        handle_text(text, &config(), feed, tx).await
    }

    #[tokio::test]
    async fn a_v1_replay_then_live_frame_reaches_trading_ready() {
        let (feed, tx, mut rx) = feed();
        let bucket = aligned_now_bucket(900_000);

        // Replay burst: two closed candles (no seq), ascending.
        handle(&feed, &tx, &candle_json(bucket - 1_800_000, None, true, true))
            .await
            .unwrap();
        handle(&feed, &tx, &candle_json(bucket - 900_000, None, true, true))
            .await
            .unwrap();

        // The status frame carries the contract pin and freshness.
        handle(&feed, &tx, &status_json(1, "live", true))
            .await
            .unwrap();
        // A live frame proves the replay is over.
        handle(&feed, &tx, &levels_json(2)).await.unwrap();

        // Two candle events were emitted; levels is the third.
        for _ in 0..2 {
            let event = rx.recv().await.expect("candle event");
            assert!(matches!(
                event,
                FeedEvent::ClosedCandle { replay: true, .. }
            ));
        }
        let event = rx.recv().await.expect("levels event");
        assert!(matches!(event, FeedEvent::Levels { .. }));

        let snapshot = feed.snapshot();
        assert!(snapshot.contract_ok, "{snapshot:?}");
        assert!(snapshot.sync_complete, "{snapshot:?}");
        assert_eq!(snapshot.session_closed_candles, 2);
        assert!(snapshot.trading_ready, "{snapshot:?}");
        assert_eq!(snapshot.state, "live");
        assert!(feed.gate_check().is_ok());
    }

    #[tokio::test]
    async fn a_sequence_gap_forces_a_resync_and_closes_the_gate() {
        let (feed, tx, _rx) = feed();
        let bucket = aligned_now_bucket(900_000);
        handle(&feed, &tx, &status_json(1, "live", true))
            .await
            .unwrap();
        let error = handle(
            &feed,
            &tx,
            &candle_json(bucket - 900_000, Some(5), false, true),
        )
        .await
        .unwrap_err();
        assert!(error.contains("sequence"));
        let snapshot = feed.snapshot();
        assert_eq!(snapshot.gaps, 1);
        assert!(!snapshot.trading_ready);
        assert_eq!(snapshot.state, "resyncing");
    }

    #[tokio::test]
    async fn a_candle_without_the_v1_fields_is_a_contract_violation() {
        let (feed, tx, _rx) = feed();
        let raw = json!({
            "type": "candle",
            "seq": 1,
            "data": {
                "time": aligned_now_bucket(900_000) - 900_000,
                "open": 2650.0, "high": 2652.0, "low": 2649.0, "close": 2651.0,
                "volume": 12000.0, "source": "sifting"
            }
        })
        .to_string();
        let error = handle(&feed, &tx, &raw).await.unwrap_err();
        assert!(error.contains("closed"));
        assert_eq!(feed.snapshot().contract_violations, 1);
        assert!(!feed.snapshot().trading_ready);
    }

    #[tokio::test]
    async fn a_duplicate_closed_candle_is_not_emitted_twice() {
        let (feed, tx, mut rx) = feed();
        let bucket = aligned_now_bucket(900_000);
        let text = candle_json(bucket - 900_000, None, true, true);
        handle(&feed, &tx, &text).await.unwrap();
        handle(&feed, &tx, &text).await.unwrap();
        let _ = rx.recv().await.expect("first candle event");
        assert!(rx.try_recv().is_err(), "the duplicate must not emit");
        assert_eq!(feed.snapshot().duplicates, 1);
    }

    #[tokio::test]
    async fn stale_freshness_and_a_closed_market_are_not_tradable() {
        let (feed, tx, _rx) = feed();
        let bucket = aligned_now_bucket(900_000);
        handle(&feed, &tx, &status_json(1, "live", true))
            .await
            .unwrap();
        handle(&feed, &tx, &levels_json(2)).await.unwrap();
        handle(&feed, &tx, &candle_json(bucket - 1_800_000, None, true, true))
            .await
            .unwrap();
        handle(&feed, &tx, &candle_json(bucket - 900_000, None, true, true))
            .await
            .unwrap();
        // Healthy at this point (two seeds and a live frame).
        assert!(feed.snapshot().trading_ready);

        handle(&feed, &tx, &status_json(3, "stale", true))
            .await
            .unwrap();
        let snapshot = feed.snapshot();
        assert!(!snapshot.trading_ready);
        assert!(snapshot
            .not_ready_reason
            .as_deref()
            .unwrap_or("")
            .contains("stale"));

        handle(&feed, &tx, &status_json(4, "closed_market", false))
            .await
            .unwrap();
        assert!(feed
            .snapshot()
            .not_ready_reason
            .unwrap_or_default()
            .contains("closed"));
    }

    #[tokio::test]
    async fn an_execution_frame_is_refused_and_never_becomes_an_event() {
        let (feed, tx, mut rx) = feed();
        let raw = json!({
            "type": "trade_intent",
            "data": { "intent_id": "n3-1", "symbol": "XAUUSD", "side": "buy" }
        })
        .to_string();
        let error = handle(&feed, &tx, &raw).await.unwrap_err();
        assert!(error.contains("trade_intent"));
        assert!(rx.try_recv().is_err());
        assert_eq!(feed.snapshot().contract_violations, 1);
    }

    #[test]
    fn the_gate_reports_the_exact_reason_it_is_closed() {
        let feed = MarketFeed::new(&config());
        let rejection = feed.gate_check().unwrap_err();
        assert_eq!(rejection.code, "market_data_not_ready");
        assert!(rejection.message.contains("not connected"));
    }

    #[test]
    fn the_replay_size_in_messages_matches_the_contract() {
        assert_eq!(CONTRACT_REPLAY_CANDLES, 15);
    }
}
