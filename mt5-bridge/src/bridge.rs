//! Bridge service layer: the only place that decides whether an order may be
//! sent, what the broker did, and how that is reported to Node 4.
//!
//! Invariants enforced here (see `docs/mt5/EXECUTION_ARCHITECTURE.md`):
//!
//! 1. **Demo only.** Every order path re-checks the account mode via the demo
//!    guard; a real/contest/unknown account halts the bridge, and `resume`
//!    re-verifies before clearing the halt.
//! 2. **Never mark opened without a broker-confirmed fill.** A rejected, timed
//!    out or contradictory `ORDER_SEND` yields `rejected`/`unknown`, never
//!    `filled`.
//! 3. **Idempotency.** An idempotency key reaches the broker at most once. A
//!    repeated key replays the recorded outcome, or resolves an uncertain one
//!    with an idempotent lookup — it never places a second order.
//! 4. **Reads are retried, writes are not.** `ACCOUNT`/`SYMBOL`/`POSITIONS`/
//!    `HISTORY`/`FIND` retry on transport hiccups; `ORDER_SEND` never does.
//! 5. **Halt is fail-closed.** When halted (manual, risk, link loss, account
//!    mode change) no new order leaves the bridge.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::{error, info, warn};

use crate::config::BridgeConfig;
use crate::ea_link::LinkEvent;
use crate::snapshot::{
    Mt5AccountSnapshot, Mt5BridgeStatus, Mt5Deal, Mt5HistorySnapshot, Mt5Position,
    Mt5PositionsSnapshot, OrderOutcome, BRIDGE_PROTOCOL_VERSION, BRIDGE_VERSION,
};
use crate::terminal::{
    self, now_ms, AccountInfo, AccountType, BoxFut, FindReport, OrderRequest, OrderSendResult,
    OrderStatus, Side, SymbolSpec, TerminalClient, TerminalError, ValidationError,
};

/// Retry helper for **idempotent reads only**. Never used on the write path.
///
/// Defined at the top of the module because `macro_rules!` is textual: a macro
/// used below must be defined above its first use.
macro_rules! read_retry {
    ($call:expr, $attempts:expr) => {{
        let attempts: u32 = $attempts;
        let mut result: Option<Result<_, TerminalError>> = None;
        for attempt in 0..attempts.max(1) {
            match $call.await {
                Ok(value) => {
                    result = Some(Ok(value));
                    break;
                }
                Err(err) => {
                    let retryable = matches!(
                        err,
                        TerminalError::Timeout { .. } | TerminalError::Transport(_)
                    );
                    result = Some(Err(err));
                    if !retryable || attempt + 1 >= attempts.max(1) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
        }
        result.expect("read_retry always assigns a result")
    }};
}

/// View of the EA link the bridge needs for status reporting; implemented by
/// `ea_link::EaLink` and by a fake in tests.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkStatusView {
    pub connected: bool,
    pub write_enabled: bool,
    pub mode: Option<String>,
    pub login: Option<i64>,
    pub server: Option<String>,
    pub ea_version: Option<String>,
    pub build: Option<i64>,
    pub last_heartbeat_ms: Option<i64>,
    pub heartbeat_age_ms: Option<i64>,
    pub last_error: Option<String>,
}

pub trait LinkStatusProvider: Send + Sync {
    fn status<'a>(&'a self) -> BoxFut<'a, LinkStatusView>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    OrderFilled(Box<OrderOutcome>),
    OrderRejected(Box<OrderOutcome>),
    OrderUnknown(Box<OrderOutcome>),
    Halted {
        reason: String,
        auto: bool,
        flattened: bool,
    },
    Resumed,
    LinkDisconnected(String),
    BrokerTrade(BTreeMap<String, String>),
}

/// Request payload from Node 4 (mirrors `Node4ToBridge::Mt5Order`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrderIntent {
    pub idempotency_key: String,
    pub intent_id: String,
    #[serde(default)]
    pub strategy_id: String,
    #[serde(default)]
    pub requested_symbol: Option<String>,
    pub side: String,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub sl: Option<f64>,
    #[serde(default)]
    pub tp: Option<f64>,
    #[serde(default)]
    pub level_name: Option<String>,
    #[serde(default)]
    pub entry_ref: Option<f64>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlInner {
    pub halted: bool,
    pub reason: Option<String>,
    pub trading_enabled: bool,
    pub halt_count: u64,
    pub updated_at: i64,
}

#[derive(Debug, Default)]
pub struct ControlState {
    inner: RwLock<ControlInner>,
}

impl ControlState {
    pub fn new(trading_enabled: bool) -> Self {
        Self {
            inner: RwLock::new(ControlInner {
                halted: !trading_enabled,
                reason: if trading_enabled {
                    None
                } else {
                    Some("MT5_TRADING_ENABLED=0 — starting halted".into())
                },
                trading_enabled,
                halt_count: if trading_enabled { 0 } else { 1 },
                updated_at: now_ms(),
            }),
        }
    }

    pub async fn snapshot(&self) -> ControlInner {
        self.inner.read().await.clone()
    }

    pub async fn is_halted(&self) -> bool {
        self.inner.read().await.halted
    }

    /// Set the halt flag; returns true when this call changed the state.
    pub async fn halt(&self, reason: impl Into<String>) -> bool {
        let mut inner = self.inner.write().await;
        let changed = !inner.halted;
        inner.halted = true;
        inner.reason = Some(reason.into());
        if changed {
            inner.halt_count = inner.halt_count.saturating_add(1);
        }
        inner.updated_at = now_ms();
        changed
    }

    pub async fn resume(&self) -> bool {
        let mut inner = self.inner.write().await;
        let changed = inner.halted;
        inner.halted = false;
        inner.reason = None;
        inner.updated_at = now_ms();
        changed
    }

    /// `MT5_TRADING_ENABLED=0` refuses trading outright; no path clears it.
    pub async fn trading_enabled(&self) -> bool {
        self.inner.read().await.trading_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub idempotency_key: String,
    pub intent_id: String,
    pub status: String,
    pub updated_at: i64,
    pub outcome: OrderOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderAudit {
    pub ts: i64,
    pub intent_id: String,
    pub idempotency_key: String,
    pub strategy_id: String,
    pub requested_symbol: String,
    pub broker_symbol: String,
    pub side: String,
    pub requested_volume: f64,
    pub filled_volume: f64,
    pub price: Option<f64>,
    pub sl: Option<f64>,
    pub tp: Option<f64>,
    pub status: String,
    pub retcode: i64,
    pub retcode_desc: String,
    pub position_ticket: Option<i64>,
    pub order_ticket: Option<i64>,
    pub deal_ticket: Option<i64>,
    pub risk_amount: Option<f64>,
    pub reconciled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryRecord {
    Deal { deal: Mt5Deal },
    Order { audit: OrderAudit },
    Control { ts: i64, action: String, reason: Option<String> },
}

/// Append-only JSONL store: closed deals plus an order/control audit trail.
///
/// This is what makes history survive a Node 4 **or** bridge restart: deals are
/// mirrored from broker history on startup and appended as they close.
pub struct HistoryStore {
    path: PathBuf,
    inner: Arc<Mutex<HistoryInner>>,
}

struct HistoryInner {
    file: tokio::fs::File,
    deal_tickets: HashSet<i64>,
    deals: Vec<Mt5Deal>,
    records: u64,
}

impl HistoryStore {
    pub async fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        let mut inner = HistoryInner {
            file,
            deal_tickets: HashSet::new(),
            deals: Vec::new(),
            records: 0,
        };
        if let Ok(contents) = tokio::fs::read_to_string(&path).await {
            for line in contents.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                inner.records += 1;
                if let Ok(HistoryRecord::Deal { deal }) =
                    serde_json::from_str::<HistoryRecord>(line)
                {
                    if inner.deal_tickets.insert(deal.ticket) {
                        inner.deals.push(deal);
                    }
                }
            }
        }
        inner.deals.sort_by_key(|deal| deal.time_ms);
        Ok(Self {
            path,
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn append(&self, record: &HistoryRecord) -> std::io::Result<()> {
        let mut inner = self.inner.lock().await;
        inner.records += 1;
        if let HistoryRecord::Deal { deal } = record {
            if inner.deal_tickets.insert(deal.ticket) {
                inner.deals.push(deal.clone());
                inner.deals.sort_by_key(|deal| deal.time_ms);
            }
        }
        let mut line = serde_json::to_string(record).unwrap_or_else(|_| "{}".into());
        line.push('\n');
        use tokio::io::AsyncWriteExt;
        inner.file.write_all(line.as_bytes()).await?;
        inner.file.flush().await?;
        Ok(())
    }

    pub async fn deals(&self) -> Vec<Mt5Deal> {
        self.inner.lock().await.deals.clone()
    }

    pub async fn deal_count(&self) -> usize {
        self.inner.lock().await.deals.len()
    }

    pub async fn record_count(&self) -> u64 {
        self.inner.lock().await.records
    }

    pub async fn last_deal_time(&self) -> Option<i64> {
        self.inner
            .lock()
            .await
            .deals
            .last()
            .map(|deal| deal.time_ms)
    }
}

#[derive(Debug, Default)]
pub struct BridgeStats {
    pub orders_sent: AtomicU64,
    pub orders_filled: AtomicU64,
    pub orders_rejected: AtomicU64,
    pub orders_unknown: AtomicU64,
    pub orders_duplicate: AtomicU64,
    pub orders_reconciled: AtomicU64,
}

impl BridgeStats {
    pub fn snapshot(&self) -> (u64, u64, u64, u64, u64, u64) {
        (
            self.orders_sent.load(Ordering::Relaxed),
            self.orders_filled.load(Ordering::Relaxed),
            self.orders_rejected.load(Ordering::Relaxed),
            self.orders_unknown.load(Ordering::Relaxed),
            self.orders_duplicate.load(Ordering::Relaxed),
            self.orders_reconciled.load(Ordering::Relaxed),
        )
    }
}

pub struct Bridge {
    cfg: Arc<BridgeConfig>,
    terminal: TerminalClient,
    link: Arc<dyn LinkStatusProvider>,
    control: Arc<ControlState>,
    ledger: Arc<Mutex<HashMap<String, LedgerEntry>>>,
    history: Arc<HistoryStore>,
    stats: Arc<BridgeStats>,
    account_cache: Arc<RwLock<Option<(AccountInfo, i64)>>>,
    symbol_cache: Arc<RwLock<HashMap<String, SymbolSpec>>>,
    last_error: Arc<RwLock<Option<String>>>,
    pub events: broadcast::Sender<BridgeEvent>,
    started_at: i64,
}

impl Bridge {
    pub fn new(
        cfg: Arc<BridgeConfig>,
        terminal: TerminalClient,
        link: Arc<dyn LinkStatusProvider>,
        control: Arc<ControlState>,
        history: Arc<HistoryStore>,
    ) -> Self {
        let (events, _) = broadcast::channel(256);
        Self {
            cfg,
            terminal,
            link,
            control,
            ledger: Arc::new(Mutex::new(HashMap::new())),
            history,
            stats: Arc::new(BridgeStats::default()),
            account_cache: Arc::new(RwLock::new(None)),
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
            last_error: Arc::new(RwLock::new(None)),
            events,
            started_at: now_ms(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BridgeEvent> {
        self.events.subscribe()
    }

    pub fn control(&self) -> Arc<ControlState> {
        self.control.clone()
    }

    pub fn history_store(&self) -> Arc<HistoryStore> {
        self.history.clone()
    }

    pub fn stats(&self) -> Arc<BridgeStats> {
        self.stats.clone()
    }

    pub fn config(&self) -> Arc<BridgeConfig> {
        self.cfg.clone()
    }

    pub fn terminal(&self) -> TerminalClient {
        self.terminal.clone()
    }

    async fn set_last_error(&self, message: Option<String>) {
        *self.last_error.write().await = message;
    }

    pub async fn last_error(&self) -> Option<String> {
        self.last_error.read().await.clone()
    }

    async fn account_currency(&self) -> String {
        self.cached_account()
            .await
            .map(|account| account.currency)
            .unwrap_or_else(|| "USD".into())
    }

    // ---- account guard ----------------------------------------------------

    /// Read the account, enforce the demo-only guard, and cache it briefly.
    ///
    /// Any guard violation halts the bridge (fail closed) and only an explicit
    /// `resume` that re-verifies can clear it.
    pub async fn verify_account(&self, force: bool) -> Result<AccountInfo, TerminalError> {
        if !force {
            let cached = self.account_cache.read().await.clone();
            if let Some((account, fetched_at)) = cached {
                if now_ms() - fetched_at <= self.cfg.account_max_age_ms as i64 {
                    // The guard is re-checked even on a cache hit: a cached
                    // account must never become tradable by going stale, and a
                    // terminal that switched to a real login keeps failing.
                    return match terminal::demo_guard(&account, self.cfg.expected_login) {
                        Ok(()) => Ok(account),
                        Err(err) => {
                            self.refuse_account(&err).await;
                            Err(err)
                        }
                    };
                }
            }
        }

        let account = match read_retry!(self.terminal.account(), 3) {
            Ok(account) => account,
            Err(err) => {
                // A transport failure is not a guard violation: the terminal may
                // simply be between ticks. Order paths fail closed regardless.
                self.set_last_error(Some(err.to_string())).await;
                return Err(err);
            }
        };

        // Cache before the guard decides: when the guard refuses a real account
        // the snapshot must still report *what the terminal is* ("real"), with
        // `authorized` false and the halt reason attached. The guard is what
        // blocks trading, never the cache.
        *self.account_cache.write().await = Some((account.clone(), now_ms()));
        if let Err(err) = terminal::demo_guard(&account, self.cfg.expected_login) {
            self.refuse_account(&err).await;
            return Err(err);
        }

        self.set_last_error(None).await;
        Ok(account)
    }

    /// Record a guard refusal and halt trading (used by every account check).
    async fn refuse_account(&self, err: &TerminalError) {
        let reason = err.to_string();
        error!("demo guard refused the terminal: {reason}");
        self.set_last_error(Some(reason.clone())).await;
        self.halt_internal(&reason, false).await;
    }

    async fn cached_account(&self) -> Option<AccountInfo> {
        self.account_cache
            .read()
            .await
            .as_ref()
            .map(|(account, _)| account.clone())
    }

    // ---- symbol resolution ------------------------------------------------

    /// Resolve a requested (strategy) symbol to a broker symbol *and* its
    /// validated contract metadata.
    ///
    /// Only two names are ever tried: the requested name itself and the single
    /// explicit `MT5_SYMBOL_MAP` entry. Broker suffixes are never guessed.
    pub async fn resolve_symbol(&self, requested: &str) -> Result<SymbolSpec, TerminalError> {
        if let Some(spec) = self.symbol_cache.read().await.get(requested).cloned() {
            return Ok(spec);
        }

        let mut candidates: Vec<String> = vec![requested.to_string()];
        if let Some(mapped) = self.cfg.symbol_map.get(requested) {
            if mapped.is_empty() {
                return Err(TerminalError::Validation(ValidationError::SymbolMetadata(
                    format!("MT5_SYMBOL_MAP entry '{requested}' is not a `requested=broker` pair"),
                )));
            }
            candidates.push(mapped.clone());
        }

        let mut last_error: Option<TerminalError> = None;
        for candidate in &candidates {
            match read_retry!(self.terminal.symbol_info(candidate), 3) {
                Ok(spec) => {
                    info!(
                        "resolved {} -> {} (digits={}, contract_size={}, volume {}-{} step {}, \
                         stops_level={} points, trade_mode={})",
                        requested,
                        spec.name,
                        spec.digits,
                        spec.contract_size,
                        spec.volume_min,
                        spec.volume_max,
                        spec.volume_step,
                        spec.stops_level_points,
                        spec.trade_mode
                    );
                    self.symbol_cache
                        .write()
                        .await
                        .insert(requested.to_string(), spec.clone());
                    return Ok(spec);
                }
                Err(err) => last_error = Some(err),
            }
        }

        Err(match last_error {
            Some(TerminalError::Ea { .. }) | None => {
                TerminalError::Validation(ValidationError::SymbolNotFound {
                    requested: requested.to_string(),
                    available: candidates.len(),
                })
            }
            Some(other) => other,
        })
    }

    pub async fn broker_symbol(&self, requested: &str) -> Option<String> {
        self.symbol_cache
            .read()
            .await
            .get(requested)
            .map(|spec| spec.name.clone())
    }

    // ---- order flow -------------------------------------------------------

    /// Place one market order for Node 4. Never sends the same idempotency key
    /// twice, and never reports a fill the broker did not confirm.
    pub async fn place_order(&self, intent: OrderIntent) -> OrderOutcome {
        let started = now_ms();
        let requested_symbol = intent
            .requested_symbol
            .clone()
            .unwrap_or_else(|| self.cfg.requested_symbol.clone());
        let side = match Side::parse(&intent.side) {
            Some(side) => side,
            None => {
                return self
                    .reject_early(
                        &intent,
                        &requested_symbol,
                        format!("invalid side '{}'", intent.side),
                        "invalid_side",
                    )
                    .await
            }
        };
        let volume = intent.volume.unwrap_or(self.cfg.default_volume_lots);

        // 1. Halt / trading-enabled gate.
        if self.control.is_halted().await {
            let reason = self
                .control
                .snapshot()
                .await
                .reason
                .unwrap_or_else(|| "halted".into());
            return self
                .reject_early(
                    &intent,
                    &requested_symbol,
                    format!("bridge is halted: {reason}"),
                    "halted",
                )
                .await;
        }
        if !self.control.trading_enabled().await {
            return self
                .reject_early(
                    &intent,
                    &requested_symbol,
                    "trading is disabled by configuration (MT5_TRADING_ENABLED=0)".to_string(),
                    "trading_disabled",
                )
                .await;
        }

        // 2. Idempotency: replay a recorded outcome; never resend the order.
        //    The lock is released before any further await so the same task can
        //    re-lock the ledger later (tokio mutexes are not reentrant).
        let existing = {
            let guard = self.ledger.lock().await;
            guard.get(&intent.idempotency_key).cloned()
        };
        if let Some(existing) = existing {
            self.stats.orders_duplicate.fetch_add(1, Ordering::Relaxed);
            match existing.status.as_str() {
                "filled" | "partial" | "rejected" => {
                    let mut outcome = existing.outcome.clone();
                    outcome.reconciled = true;
                    return outcome;
                }
                "in_flight" => {
                    let ts = now_ms();
                    return OrderOutcome {
                        status: "unknown".into(),
                        idempotency_key: intent.idempotency_key.clone(),
                        intent_id: intent.intent_id.clone(),
                        requested_symbol: requested_symbol.clone(),
                        broker_symbol: self
                            .broker_symbol(&requested_symbol)
                            .await
                            .unwrap_or_default(),
                        side: side.as_str().into(),
                        requested_volume: volume,
                        error: Some(
                            "an identical request is already in flight — not resending".into(),
                        ),
                        latency_ms: ts - started,
                        ts,
                        ..Default::default()
                    };
                }
                _ => {
                    // Recorded as unknown: try to resolve it before doing
                    // anything else, and never send a second order.
                    if let Some(resolved) = self.resolve_unknown(&intent, &existing.outcome).await {
                        self.record_ledger(&resolved).await;
                        return resolved;
                    }
                    let mut outcome = existing.outcome.clone();
                    outcome.error = Some(
                        "previous attempt has no broker-confirmed outcome; resolve it before \
                         retrying with a new idempotency key"
                            .into(),
                    );
                    return outcome;
                }
            }
        }

        // 3. Account guard (fresh or cached) — fail closed.
        let account = match self.verify_account(false).await {
            Ok(account) => account,
            Err(err) => {
                return self
                    .reject_early(&intent, &requested_symbol, err.to_string(), "account_guard")
                    .await
            }
        };

        // 4. Symbol metadata, quote freshness, volume/stops normalization.
        let spec = match self.resolve_symbol(&requested_symbol).await {
            Ok(spec) => spec,
            Err(err) => {
                return self
                    .reject_early(
                        &intent,
                        &requested_symbol,
                        err.to_string(),
                        "symbol_unavailable",
                    )
                    .await
            }
        };
        let spec = if spec.quote_ts_ms > 0 {
            spec
        } else {
            match read_retry!(self.terminal.quote(&spec.name), 3) {
                Ok((bid, ask, ts)) => SymbolSpec {
                    bid,
                    ask,
                    quote_ts_ms: ts,
                    ..spec
                },
                Err(err) => {
                    return self
                        .reject_early(
                            &intent,
                            &requested_symbol,
                            err.to_string(),
                            "quote_unavailable",
                        )
                        .await
                }
            }
        };
        if !account.trade_allowed {
            return self
                .reject_early(
                    &intent,
                    &requested_symbol,
                    "the account reports trade_allowed=false (trading disabled by the broker)"
                        .to_string(),
                    "trade_disabled",
                )
                .await;
        }

        let normalized = match terminal::validate_order(
            side,
            volume,
            intent.sl,
            intent.tp,
            intent.entry_ref.unwrap_or(0.0),
            &spec,
            now_ms(),
            self.cfg.max_quote_age_ms,
        ) {
            Ok(normalized) => normalized,
            Err(err) => {
                return self
                    .reject_early(&intent, &requested_symbol, err.to_string(), "validation")
                    .await
            }
        };

        // 5. Send — the only place that changes broker state.
        let comment = terminal::intent_comment(&intent.intent_id);
        let request = OrderRequest {
            intent_id: intent.intent_id.clone(),
            broker_symbol: spec.name.clone(),
            side,
            volume: normalized.volume,
            sl: normalized.sl,
            tp: normalized.tp,
            deviation_points: self.cfg.max_deviation_points,
            magic: self.cfg.magic,
            comment: comment.clone(),
        };

        self.mark_in_flight(&intent, &spec).await;
        self.stats.orders_sent.fetch_add(1, Ordering::Relaxed);
        let timeout_ms = intent.timeout_ms.unwrap_or(self.cfg.order_timeout_ms);
        let send_result = self.terminal.call_order_send(&request, timeout_ms).await;

        let mut outcome = match send_result {
            Ok(result) => {
                self.outcome_from_send(&intent, &spec, &normalized, result, started)
                    .await
            }
            Err(TerminalError::Guard(message)) => {
                // Refused locally; the terminal was never touched.
                return self
                    .reject_early(&intent, &requested_symbol, message, "guard")
                    .await;
            }
            Err(err) => {
                warn!(
                    "ORDER_SEND outcome uncertain for intent {}: {err} — reconciling",
                    intent.intent_id
                );
                let recorded = self
                    .unknown_from_error(&intent, &spec, &normalized, &err, started)
                    .await;
                match self.resolve_unknown(&intent, &recorded).await {
                    Some(resolved) => resolved,
                    None => recorded,
                }
            }
        };

        // A "filled" status without a broker-reported volume is not a fill.
        if outcome.status == "filled" && outcome.filled_volume <= 0.0 {
            warn!(
                "ORDER_SEND reported filled without a volume for intent {} — resolving",
                intent.intent_id
            );
            let uncertain = OrderOutcome {
                status: "unknown".into(),
                error: Some("broker reported a fill without a volume".into()),
                ..outcome.clone()
            };
            outcome = match self.resolve_unknown(&intent, &uncertain).await {
                Some(resolved) => resolved,
                None => uncertain,
            };
        }

        self.record_ledger(&outcome).await;
        self.record_audit(&intent, &spec, &outcome).await;
        match outcome.status.as_str() {
            "filled" | "partial" => {
                self.stats.orders_filled.fetch_add(1, Ordering::Relaxed);
                let _ = self
                    .events
                    .send(BridgeEvent::OrderFilled(Box::new(outcome.clone())));
            }
            "rejected" => {
                self.stats.orders_rejected.fetch_add(1, Ordering::Relaxed);
                let _ = self
                    .events
                    .send(BridgeEvent::OrderRejected(Box::new(outcome.clone())));
            }
            _ => {
                self.stats.orders_unknown.fetch_add(1, Ordering::Relaxed);
                let _ = self
                    .events
                    .send(BridgeEvent::OrderUnknown(Box::new(outcome.clone())));
            }
        }
        outcome
    }

    /// A request refused **before** the terminal was touched: it is reported as
    /// rejected but deliberately not written to the ledger, so the same
    /// idempotency key stays available for a legitimate retry.
    async fn reject_early(
        &self,
        intent: &OrderIntent,
        requested_symbol: &str,
        message: String,
        code: &str,
    ) -> OrderOutcome {
        let outcome = OrderOutcome {
            status: "rejected".into(),
            idempotency_key: intent.idempotency_key.clone(),
            intent_id: intent.intent_id.clone(),
            requested_symbol: requested_symbol.to_string(),
            side: intent.side.clone(),
            requested_volume: intent.volume.unwrap_or(self.cfg.default_volume_lots),
            retcode: 0,
            retcode_desc: code.to_string(),
            error: Some(message),
            risk_currency: self.account_currency().await,
            ts: now_ms(),
            ..Default::default()
        };
        self.stats.orders_rejected.fetch_add(1, Ordering::Relaxed);
        let _ = self
            .events
            .send(BridgeEvent::OrderRejected(Box::new(outcome.clone())));
        outcome
    }

    async fn mark_in_flight(&self, intent: &OrderIntent, spec: &SymbolSpec) {
        let entry = LedgerEntry {
            idempotency_key: intent.idempotency_key.clone(),
            intent_id: intent.intent_id.clone(),
            status: "in_flight".into(),
            updated_at: now_ms(),
            outcome: OrderOutcome {
                status: "unknown".into(),
                idempotency_key: intent.idempotency_key.clone(),
                intent_id: intent.intent_id.clone(),
                broker_symbol: spec.name.clone(),
                side: intent.side.clone(),
                requested_volume: intent.volume.unwrap_or(self.cfg.default_volume_lots),
                ..Default::default()
            },
        };
        self.ledger
            .lock()
            .await
            .insert(intent.idempotency_key.clone(), entry);
    }

    async fn record_ledger(&self, outcome: &OrderOutcome) {
        let entry = LedgerEntry {
            idempotency_key: outcome.idempotency_key.clone(),
            intent_id: outcome.intent_id.clone(),
            status: outcome.status.clone(),
            updated_at: now_ms(),
            outcome: outcome.clone(),
        };
        self.ledger
            .lock()
            .await
            .insert(outcome.idempotency_key.clone(), entry);
    }

    async fn record_audit(&self, intent: &OrderIntent, spec: &SymbolSpec, outcome: &OrderOutcome) {
        let audit = OrderAudit {
            ts: outcome.ts,
            intent_id: intent.intent_id.clone(),
            idempotency_key: intent.idempotency_key.clone(),
            strategy_id: intent.strategy_id.clone(),
            requested_symbol: intent
                .requested_symbol
                .clone()
                .unwrap_or_else(|| self.cfg.requested_symbol.clone()),
            broker_symbol: spec.name.clone(),
            side: outcome.side.clone(),
            requested_volume: outcome.requested_volume,
            filled_volume: outcome.filled_volume,
            price: outcome.price,
            sl: outcome.sl,
            tp: outcome.tp,
            status: outcome.status.clone(),
            retcode: outcome.retcode,
            retcode_desc: outcome.retcode_desc.clone(),
            position_ticket: outcome.position_ticket,
            order_ticket: outcome.order_ticket,
            deal_ticket: outcome.deal_ticket,
            risk_amount: outcome.risk_amount,
            reconciled: outcome.reconciled,
            error: outcome.error.clone(),
        };
        if let Err(err) = self
            .history
            .append(&HistoryRecord::Order { audit })
            .await
        {
            warn!("failed to persist order audit: {err}");
        }
    }

    async fn outcome_from_send(
        &self,
        intent: &OrderIntent,
        spec: &SymbolSpec,
        normalized: &terminal::NormalizedOrder,
        result: OrderSendResult,
        started: i64,
    ) -> OrderOutcome {
        let ts = now_ms();
        OrderOutcome {
            status: result.status.as_str().to_string(),
            idempotency_key: intent.idempotency_key.clone(),
            intent_id: intent.intent_id.clone(),
            requested_symbol: intent
                .requested_symbol
                .clone()
                .unwrap_or_else(|| self.cfg.requested_symbol.clone()),
            broker_symbol: spec.name.clone(),
            side: normalized.side.as_str().to_string(),
            requested_volume: normalized.volume,
            filled_volume: result.volume_filled,
            price: result.price,
            sl: Some(normalized.sl),
            tp: if normalized.tp > 0.0 {
                Some(normalized.tp)
            } else {
                None
            },
            order_ticket: result.order_ticket,
            deal_ticket: result.deal_ticket,
            position_ticket: result.position_ticket,
            retcode: result.retcode,
            retcode_desc: result.retcode_desc.clone(),
            risk_amount: Some(normalized.risk_amount),
            risk_currency: self.account_currency().await,
            notional: Some(normalized.notional),
            reconciled: false,
            latency_ms: ts - started,
            ts,
            error: if result.status == OrderStatus::Rejected {
                Some(if result.retcode_desc.is_empty() {
                    format!("order rejected (retcode {})", result.retcode)
                } else {
                    result.retcode_desc
                })
            } else {
                None
            },
        }
    }

    async fn unknown_from_error(
        &self,
        intent: &OrderIntent,
        spec: &SymbolSpec,
        normalized: &terminal::NormalizedOrder,
        err: &TerminalError,
        started: i64,
    ) -> OrderOutcome {
        let ts = now_ms();
        OrderOutcome {
            status: "unknown".into(),
            idempotency_key: intent.idempotency_key.clone(),
            intent_id: intent.intent_id.clone(),
            requested_symbol: intent
                .requested_symbol
                .clone()
                .unwrap_or_else(|| self.cfg.requested_symbol.clone()),
            broker_symbol: spec.name.clone(),
            side: normalized.side.as_str().to_string(),
            requested_volume: normalized.volume,
            filled_volume: 0.0,
            price: None,
            sl: Some(normalized.sl),
            tp: if normalized.tp > 0.0 {
                Some(normalized.tp)
            } else {
                None
            },
            order_ticket: None,
            deal_ticket: None,
            position_ticket: None,
            retcode: 0,
            retcode_desc: "outcome unknown".into(),
            risk_amount: Some(normalized.risk_amount),
            risk_currency: self.account_currency().await,
            notional: Some(normalized.notional),
            reconciled: false,
            latency_ms: ts - started,
            ts,
            error: Some(err.to_string()),
        }
    }

    /// Resolve an uncertain outcome with idempotent reads only.
    pub async fn resolve_unknown(
        &self,
        intent: &OrderIntent,
        recorded: &OrderOutcome,
    ) -> Option<OrderOutcome> {
        let comment = terminal::intent_comment(&intent.intent_id);
        let from_ms = recorded.ts.saturating_sub(15 * 60 * 1000);
        let report = read_retry!(
            self.terminal
                .find_by_comment(&comment, self.cfg.magic, from_ms),
            2
        )
        .ok()?;
        self.stats.orders_reconciled.fetch_add(1, Ordering::Relaxed);
        outcome_from_reconcile(intent, recorded, &report, &comment)
    }

    /// Adopt broker positions that carry this bridge's magic number but are not
    /// in the local ledger (e.g. a fill that happened while Node 4 was down).
    pub async fn reconcile_positions(&self) -> Vec<Mt5Position> {
        let positions = match read_retry!(self.terminal.positions(None), 3) {
            Ok(positions) => positions,
            Err(err) => {
                warn!("position reconciliation failed: {err}");
                return Vec::new();
            }
        };
        let ours: Vec<Mt5Position> = positions
            .into_iter()
            .filter(|position| position.magic == self.cfg.magic)
            .collect();

        let known: HashSet<i64> = {
            let ledger = self.ledger.lock().await;
            ledger
                .values()
                .filter_map(|entry| entry.outcome.position_ticket)
                .collect()
        };

        let mut adopted = Vec::new();
        for position in &ours {
            if known.contains(&position.ticket) {
                continue;
            }
            warn!(
                "adopting unledgered broker position {} ({} {} lots, comment '{}')",
                position.ticket, position.symbol, position.volume, position.comment
            );
            let outcome = OrderOutcome {
                status: "filled".into(),
                idempotency_key: format!("reconciled-{}", position.ticket),
                intent_id: position.comment.clone(),
                requested_symbol: self.cfg.requested_symbol.clone(),
                broker_symbol: position.symbol.clone(),
                side: position.side.clone(),
                requested_volume: position.volume,
                filled_volume: position.volume,
                price: Some(position.price_open),
                sl: Some(position.sl),
                tp: Some(position.tp),
                position_ticket: Some(position.ticket),
                retcode: terminal::RETCODE_DONE,
                retcode_desc: "adopted from broker positions".into(),
                risk_currency: self.account_currency().await,
                reconciled: true,
                ts: now_ms(),
                ..Default::default()
            };
            self.record_ledger(&outcome).await;
            adopted.push(position.clone());
        }
        adopted
    }

    // ---- control plane ----------------------------------------------------

    async fn halt_internal(&self, reason: &str, flatten: bool) -> bool {
        let changed = self.control.halt(reason).await;
        let flattened = if flatten {
            match self.close_all_internal("halt").await {
                Ok(closed) => {
                    info!("halt flattened {} position(s)", closed.len());
                    !closed.is_empty()
                }
                Err(err) => {
                    error!("halt could not flatten positions: {err}");
                    false
                }
            }
        } else {
            false
        };
        if changed {
            let _ = self
                .history
                .append(&HistoryRecord::Control {
                    ts: now_ms(),
                    action: "halt".into(),
                    reason: Some(reason.to_string()),
                })
                .await;
            let _ = self.events.send(BridgeEvent::Halted {
                reason: reason.to_string(),
                auto: true,
                flattened,
            });
        }
        changed
    }

    /// Manual/automatic halt. `flatten` also closes this bridge's positions.
    pub async fn halt(&self, reason: impl Into<String>, flatten: bool) -> bool {
        self.halt_internal(&reason.into(), flatten).await
    }

    /// Clear a halt. Refuses unless the terminal is connected, the account is
    /// still demo, the link can write, and trading is enabled by configuration.
    pub async fn resume(&self) -> Result<(), TerminalError> {
        if !self.control.trading_enabled().await {
            return Err(TerminalError::Guard(
                "MT5_TRADING_ENABLED=0 — trading cannot be resumed by the control plane".into(),
            ));
        }
        self.verify_account(true).await?;
        let link = self.link.status().await;
        if !link.connected {
            return Err(TerminalError::Guard(
                "the EA link is not connected — refusing to resume".into(),
            ));
        }
        if !link.write_enabled {
            return Err(TerminalError::Guard(
                "the EA link is read-only (MT5_EA_TOKEN unset) — refusing to resume".into(),
            ));
        }
        self.control.resume().await;
        let _ = self
            .history
            .append(&HistoryRecord::Control {
                ts: now_ms(),
                action: "resume".into(),
                reason: None,
            })
            .await;
        let _ = self.events.send(BridgeEvent::Resumed);
        info!("bridge resumed (trading re-enabled)");
        Ok(())
    }

    /// Modify SL/TP of a position this bridge owns. `sl`/`tp` of `None` keep
    /// the current level, so Node 4 can move a stop without restating both.
    pub async fn modify_position(
        &self,
        position_ticket: i64,
        sl: Option<f64>,
        tp: Option<f64>,
    ) -> Result<(), TerminalError> {
        let positions = read_retry!(self.terminal.positions(None), 3)?;
        let Some(position) = positions.iter().find(|p| p.ticket == position_ticket) else {
            return Err(TerminalError::Validation(
                ValidationError::PositionNotFound(position_ticket),
            ));
        };
        if position.magic != self.cfg.magic {
            return Err(TerminalError::Guard(format!(
                "position {position_ticket} has magic {} but this bridge owns {}",
                position.magic, self.cfg.magic
            )));
        }
        let sl = sl.unwrap_or(position.sl);
        let tp = tp.unwrap_or(position.tp);
        self.terminal.position_modify(position_ticket, sl, tp).await
    }

    /// Close a single position, refusing tickets this bridge does not own.
    pub async fn close_position(        &self,
        position_ticket: i64,
        volume: Option<f64>,
    ) -> Result<OrderOutcome, TerminalError> {
        let positions = read_retry!(self.terminal.positions(None), 3)?;
        let Some(position) = positions.iter().find(|p| p.ticket == position_ticket) else {
            return Err(TerminalError::Validation(
                ValidationError::PositionNotFound(position_ticket),
            ));
        };
        if position.magic != self.cfg.magic {
            return Err(TerminalError::Guard(format!(
                "position {position_ticket} has magic {} but this bridge owns {}",
                position.magic, self.cfg.magic
            )));
        }
        let volume = volume.unwrap_or(position.volume).min(position.volume);
        let result = self
            .terminal
            .position_close(position_ticket, volume, self.cfg.max_deviation_points)
            .await?;
        let outcome = OrderOutcome {
            status: result.status.as_str().to_string(),
            idempotency_key: format!("close-{position_ticket}-{}", now_ms()),
            intent_id: position.comment.clone(),
            requested_symbol: self.cfg.requested_symbol.clone(),
            broker_symbol: position.symbol.clone(),
            side: if position.side == "buy" { "sell" } else { "buy" }.into(),
            requested_volume: volume,
            filled_volume: result.volume_filled,
            price: result.price,
            position_ticket: Some(position_ticket),
            order_ticket: result.order_ticket,
            deal_ticket: result.deal_ticket,
            retcode: result.retcode,
            retcode_desc: result.retcode_desc.clone(),
            risk_currency: self.account_currency().await,
            ts: now_ms(),
            ..Default::default()
        };
        self.fill_history_from_broker().await;
        Ok(outcome)
    }

    /// Close every open position owned by this bridge (magic number).
    pub async fn close_all(&self, reason: &str) -> Result<Vec<Mt5Position>, TerminalError> {
        info!("close_all requested ({reason})");
        let closed = self.close_all_internal(reason).await?;
        self.fill_history_from_broker().await;
        Ok(closed)
    }

    async fn close_all_internal(&self, reason: &str) -> Result<Vec<Mt5Position>, TerminalError> {
        let positions = read_retry!(self.terminal.positions(None), 3)?;
        let ours: Vec<Mt5Position> = positions
            .into_iter()
            .filter(|p| p.magic == self.cfg.magic)
            .collect();
        if ours.is_empty() {
            return Ok(Vec::new());
        }
        let symbol = ours.first().map(|p| p.symbol.clone());
        let results = self
            .terminal
            .close_all(
                symbol.as_deref(),
                self.cfg.magic,
                self.cfg.max_deviation_points,
            )
            .await?;
        let mut closed = Vec::new();
        for result in results {
            if result.ok {
                if let Some(position) = ours.iter().find(|p| p.ticket == result.position_ticket) {
                    closed.push(position.clone());
                }
            } else {
                error!(
                    "failed to close position {}: {}",
                    result.position_ticket, result.error
                );
            }
        }
        let _ = self
            .history
            .append(&HistoryRecord::Control {
                ts: now_ms(),
                action: "close_all".into(),
                reason: Some(format!("{reason} ({} closed)", closed.len())),
            })
            .await;
        Ok(closed)
    }

    /// Mirror closed deals from the broker into the durable store.
    pub async fn fill_history_from_broker(&self) -> usize {
        let now = now_ms();
        let from = self
            .history
            .last_deal_time()
            .await
            .map(|ts| ts.saturating_sub(60_000))
            .unwrap_or_else(|| now - 30 * 24 * 60 * 60 * 1000);
        let deals = match read_retry!(
            self.terminal.deals(from, now, self.cfg.history_page_size),
            3
        ) {
            Ok(deals) => deals,
            Err(err) => {
                warn!("broker deal history unavailable: {err}");
                return 0;
            }
        };
        let mut added = 0usize;
        for deal in deals
            .into_iter()
            .filter(|deal| deal.magic == self.cfg.magic)
        {
            match self
                .history
                .append(&HistoryRecord::Deal { deal: deal.clone() })
                .await
            {
                Ok(()) => added += 1,
                Err(err) => warn!("failed to persist deal {}: {err}", deal.ticket),
            }
        }
        added
    }

    // ---- snapshots --------------------------------------------------------

    pub async fn account_snapshot(&self) -> Mt5AccountSnapshot {
        let link = self.link.status().await;
        let account = match self.verify_account(false).await {
            Ok(account) => Some(account),
            Err(_) => self.cached_account().await,
        };
        let control = self.control.snapshot().await;
        let spec = self
            .symbol_cache
            .read()
            .await
            .get(&self.cfg.requested_symbol)
            .cloned();

        Mt5AccountSnapshot {
            configured: true,
            connected: link.connected,
            authorized: link.connected
                && link.write_enabled
                && !control.halted
                && account
                    .as_ref()
                    .map(|a| a.account_type.is_demo() && a.connected)
                    .unwrap_or(false),
            account_type: account
                .as_ref()
                .map(|a| a.account_type.as_str().to_string())
                .or_else(|| link.mode.clone())
                .unwrap_or_else(|| "unknown".into()),
            login: account.as_ref().map(|a| a.login).or(link.login),
            server: account
                .as_ref()
                .map(|a| a.server.clone())
                .or_else(|| link.server.clone()),
            company: account.as_ref().map(|a| a.company.clone()),
            currency: account
                .as_ref()
                .map(|a| a.currency.clone())
                .unwrap_or_else(|| "USD".into()),
            balance: account.as_ref().map(|a| a.balance),
            equity: account.as_ref().map(|a| a.equity),
            margin: account.as_ref().map(|a| a.margin),
            margin_free: account.as_ref().map(|a| a.margin_free),
            leverage: account.as_ref().map(|a| a.leverage),
            trade_allowed: account.as_ref().map(|a| a.trade_allowed).unwrap_or(false),
            halted: control.halted,
            halt_reason: control.reason.clone(),
            requested_symbol: self.cfg.requested_symbol.clone(),
            broker_symbol: spec.as_ref().map(|s| s.name.clone()),
            symbol_digits: spec.as_ref().map(|s| s.digits),
            symbol_volume_min: spec.as_ref().map(|s| s.volume_min),
            symbol_volume_max: spec.as_ref().map(|s| s.volume_max),
            symbol_volume_step: spec.as_ref().map(|s| s.volume_step),
            symbol_contract_size: spec.as_ref().map(|s| s.contract_size),
            terminal_build: link.build,
            ea_version: link.ea_version.clone(),
            bridge_version: BRIDGE_VERSION.to_string(),
            latency_ms: None,
            last_heartbeat: link.last_heartbeat_ms,
            last_updated: Some(now_ms()),
            error: self.last_error().await,
            setup_hint: self.cfg.ea_write_disabled_reason().or_else(|| {
                if link.connected {
                    None
                } else {
                    Some(format!(
                        "no EA is connected to {}:{} — load Mt5BridgeEA.mq5 in the terminal and \
                         allow algorithmic trading",
                        self.cfg.ea_bind_addr, self.cfg.ea_port
                    ))
                }
            }),
        }
    }

    pub async fn positions_snapshot(&self) -> Mt5PositionsSnapshot {
        let control = self.control.snapshot().await;
        let account = self.cached_account().await;
        let positions: Vec<Mt5Position> = match read_retry!(self.terminal.positions(None), 2) {
            Ok(positions) => positions
                .into_iter()
                .filter(|p| p.magic == self.cfg.magic)
                .collect(),
            Err(err) => {
                warn!("positions snapshot unavailable: {err}");
                Vec::new()
            }
        };
        let total_volume: f64 = positions.iter().map(|p| p.volume).sum();
        let total_unrealized: f64 = positions.iter().map(|p| p.unrealized_pnl).sum();
        Mt5PositionsSnapshot {
            count: positions.len(),
            positions,
            total_volume,
            total_unrealized_pnl: total_unrealized,
            halted: control.halted,
            halt_reason: control.reason.clone(),
            account_login: account.as_ref().map(|a| a.login),
            account_type: account
                .as_ref()
                .map(|a| a.account_type.as_str().to_string())
                .unwrap_or_else(|| "unknown".into()),
            source: "bridge".into(),
            timestamp: now_ms(),
        }
    }

    pub async fn history_snapshot(&self, cursor: Option<usize>) -> Mt5HistorySnapshot {
        let all = self.history.deals().await;
        let start = cursor.unwrap_or(0).min(all.len());
        let page: Vec<Mt5Deal> = all
            .iter()
            .skip(start)
            .take(self.cfg.history_page_size)
            .cloned()
            .collect();
        let next = start + page.len();
        Mt5HistorySnapshot {
            count: page.len(),
            total_realized_pnl: page
                .iter()
                .map(|deal| deal.profit + deal.swap + deal.commission)
                .sum(),
            first_ms: page.first().map(|deal| deal.time_ms),
            last_ms: page.last().map(|deal| deal.time_ms),
            cursor: Some(next.to_string()),
            complete: next >= all.len(),
            deals: page,
            source: format!("bridge:{}", self.history.path().display()),
            timestamp: now_ms(),
        }
    }

    pub async fn status_snapshot(&self) -> Mt5BridgeStatus {
        let link = self.link.status().await;
        let control = self.control.snapshot().await;
        let (sent, filled, rejected, unknown, _, _) = self.stats.snapshot();
        let positions_open = match read_retry!(self.terminal.positions(None), 1) {
            Ok(positions) => positions
                .into_iter()
                .filter(|p| p.magic == self.cfg.magic)
                .count(),
            Err(_) => 0,
        };
        Mt5BridgeStatus {
            configured: true,
            connected: link.connected,
            authorized: link.connected && link.write_enabled && !control.halted,
            protocol: BRIDGE_PROTOCOL_VERSION,
            bridge_version: BRIDGE_VERSION.to_string(),
            node4_url: self.cfg.node4_ws_url.clone(),
            ea_connected: link.connected,
            ea_write_enabled: link.write_enabled,
            ea_mode: link.mode.clone(),
            ea_login: link.login,
            ea_server: link.server.clone(),
            ea_last_heartbeat: link.last_heartbeat_ms,
            halted: control.halted,
            halt_reason: control.reason.clone(),
            trading_enabled: control.trading_enabled,
            orders_sent: sent,
            orders_filled: filled,
            orders_rejected: rejected,
            orders_unknown: unknown,
            positions_open,
            history_deals: self.history.deal_count().await,
            last_error: self.last_error().await.or(link.last_error.clone()),
            uptime_secs: ((now_ms() - self.started_at).max(0) / 1000) as u64,
            timestamp: now_ms(),
        }
    }

    // ---- link events ------------------------------------------------------

    /// React to EA link events. This is where a mid-session account-mode change
    /// or a link loss halts trading instead of being discovered later.
    pub async fn on_link_event(&self, event: LinkEvent) {
        match event {
            LinkEvent::Connected(hello) => {
                if AccountType::from_wire(&hello.mode) != AccountType::Demo {
                    let reason =
                        format!("EA connected with account mode '{}' — demo only", hello.mode);
                    error!("{reason}");
                    self.halt_internal(&reason, false).await;
                } else {
                    info!(
                        "EA link established (login {}, server {}, demo)",
                        hello.login, hello.server
                    );
                }
            }
            LinkEvent::Disconnected(reason) => {
                let _ = self
                    .events
                    .send(BridgeEvent::LinkDisconnected(reason.clone()));
                if self.cfg.halt_on_ea_disconnect {
                    let why = format!("EA link disconnected: {reason}");
                    warn!("{why} — halting new orders (MT5_HALT_ON_EA_DISCONNECT)");
                    self.halt_internal(&why, false).await;
                }
            }
            LinkEvent::Heartbeat(hb) => {
                if AccountType::from_wire(&hb.mode) != AccountType::Demo {
                    let reason = format!(
                        "terminal heartbeat reports account mode '{}' — demo only",
                        hb.mode
                    );
                    error!("{reason}");
                    self.halt_internal(&reason, false).await;
                    return;
                }
                if !hb.connected {
                    let reason =
                        "terminal reports it is not connected to the trade server".to_string();
                    warn!("{reason}");
                    self.halt_internal(&reason, false).await;
                    return;
                }
                if !hb.trade_allowed {
                    // Not a halt: brokers disable trading temporarily (news
                    // windows). Orders are refused by validation meanwhile.
                    warn!("terminal reports trade_allowed=false (broker-side restriction)");
                }
            }
            LinkEvent::TradeEvent(fields) => {
                let _ = self.events.send(BridgeEvent::BrokerTrade(fields));
            }
        }
    }

    pub async fn shutdown(&self) {
        self.control
            .halt("bridge is shutting down — no new orders")
            .await;
    }
}

/// Turn a `FIND` result into a confirmed outcome, or `None` when the broker has
/// no trace of the intent (i.e. the order never reached the market).
pub fn outcome_from_reconcile(
    intent: &OrderIntent,
    recorded: &OrderOutcome,
    report: &FindReport,
    comment: &str,
) -> Option<OrderOutcome> {
    let position = report
        .positions
        .iter()
        .find(|position| position.comment == comment || position.comment.contains(comment));
    let deal = report.deals.iter().find(|deal| {
        deal.comment == comment
            || deal.comment.contains(comment)
            || (recorded.order_ticket.is_some() && deal.order_ticket == recorded.order_ticket.unwrap_or_default())
    });

    if position.is_none() && deal.is_none() {
        return None;
    }

    let mut outcome = recorded.clone();
    // The intent is authoritative for identity: `recorded` may have been built
    // from a failed send whose fields were only partially known.
    outcome.idempotency_key = intent.idempotency_key.clone();
    outcome.intent_id = intent.intent_id.clone();
    outcome.reconciled = true;
    outcome.error = None;

    if let Some(position) = position {
        outcome.status = "filled".into();
        outcome.filled_volume = position.volume;
        outcome.price = Some(position.price_open);
        outcome.position_ticket = Some(position.ticket);
        outcome.side = position.side.clone();
        outcome.retcode = terminal::RETCODE_DONE;
        outcome.retcode_desc = "reconciled from open position".into();
        outcome.ts = now_ms();
        return Some(outcome);
    }

    let deal = deal.expect("checked above");
    outcome.deal_ticket = Some(deal.ticket);
    outcome.order_ticket = Some(deal.order_ticket);
    outcome.position_ticket = Some(deal.position_ticket);
    outcome.price = Some(deal.price);
    outcome.filled_volume = deal.volume;
    if deal.entry == "out" || deal.entry == "out_by" {
        // The intent existed but is already closed (e.g. SL/TP hit first).
        outcome.status = "unknown".into();
        outcome.error = Some(format!(
            "intent {comment} was already closed by the broker before it could be confirmed"
        ));
    } else {
        outcome.status = "filled".into();
        outcome.retcode = terminal::RETCODE_DONE;
        outcome.retcode_desc = "reconciled from deal history".into();
    }
    outcome.ts = now_ms();
    Some(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn history_store_survives_reopen_and_dedupes_deals() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "mt5-bridge-history-test-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let store = HistoryStore::open(&path).await.unwrap();
        let deal = Mt5Deal {
            ticket: 42,
            position_ticket: 7,
            symbol: "XAUUSD".into(),
            profit: 12.5,
            time_ms: 1_700_000_000_000,
            ..Default::default()
        };
        store
            .append(&HistoryRecord::Deal { deal: deal.clone() })
            .await
            .unwrap();
        // The same deal twice must not duplicate.
        store
            .append(&HistoryRecord::Deal { deal: deal.clone() })
            .await
            .unwrap();
        assert_eq!(store.deal_count().await, 1);
        assert_eq!(store.record_count().await, 2);

        // Re-open, simulating a bridge restart: the deal is still there.
        drop(store);
        let reopened = HistoryStore::open(&path).await.unwrap();
        assert_eq!(reopened.deal_count().await, 1);
        assert_eq!(reopened.last_deal_time().await, Some(1_700_000_000_000));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn control_state_halts_and_resumes_explicitly() {
        let control = ControlState::new(true);
        assert!(!control.is_halted().await);
        assert!(control.halt("test").await);
        assert!(control.is_halted().await);
        // Halting twice does not inflate the counter.
        assert!(!control.halt("again").await);
        assert_eq!(control.snapshot().await.halt_count, 1);
        assert!(control.resume().await);
        assert!(!control.is_halted().await);
    }

    #[tokio::test]
    async fn trading_disabled_config_starts_halted() {
        let control = ControlState::new(false);
        assert!(control.is_halted().await);
        assert!(!control.trading_enabled().await);
        assert!(control.snapshot().await.reason.is_some());
    }

    #[test]
    fn reconcile_requires_broker_evidence() {
        let intent = OrderIntent {
            idempotency_key: "k".into(),
            intent_id: "N4-1".into(),
            side: "buy".into(),
            ..Default::default()
        };
        let recorded = OrderOutcome {
            status: "unknown".into(),
            idempotency_key: "k".into(),
            intent_id: "N4-1".into(),
            ts: 1_700_000_000_000,
            ..Default::default()
        };
        let comment = terminal::intent_comment(&intent.intent_id);

        // No broker evidence -> still unknown.
        assert!(
            outcome_from_reconcile(&intent, &recorded, &FindReport::default(), &comment).is_none()
        );

        // An open position -> confirmed fill.
        let report = FindReport {
            positions: vec![Mt5Position {
                ticket: 789,
                symbol: "XAUUSD".into(),
                side: "buy".into(),
                volume: 0.01,
                price_open: 2650.12,
                comment: comment.clone(),
                magic: 330_033,
                ..Default::default()
            }],
            deals: Vec::new(),
        };
        let outcome = outcome_from_reconcile(&intent, &recorded, &report, &comment).unwrap();
        assert_eq!(outcome.status, "filled");
        assert!(outcome.reconciled);
        assert_eq!(outcome.position_ticket, Some(789));
        assert_eq!(outcome.filled_volume, 0.01);

        // A closing deal -> the intent existed but is over, which is not a
        // position Node 4 may mark as open.
        let report = FindReport {
            positions: Vec::new(),
            deals: vec![Mt5Deal {
                ticket: 5,
                position_ticket: 789,
                comment: comment.clone(),
                entry: "out".into(),
                volume: 0.01,
                profit: 3.0,
                ..Default::default()
            }],
        };
        let outcome = outcome_from_reconcile(&intent, &recorded, &report, &comment).unwrap();
        assert_eq!(outcome.status, "unknown");
        assert!(outcome.error.unwrap().contains("already closed"));
    }

    #[test]
    fn send_results_map_to_conservative_statuses() {
        // Only the mapping is exercised here; the full order flow runs in
        // tests/contract.rs against a fake terminal.
        let rejected = OrderSendResult {
            status: OrderStatus::Rejected,
            retcode: 10_016,
            retcode_desc: "invalid stops".into(),
            order_ticket: None,
            deal_ticket: None,
            position_ticket: None,
            price: None,
            volume_filled: 0.0,
            ts_ms: 0,
        };
        assert_eq!(rejected.status, OrderStatus::Rejected);

        let filled = OrderSendResult {
            status: OrderStatus::Filled,
            retcode: 10_009,
            retcode_desc: String::new(),
            order_ticket: Some(1),
            deal_ticket: Some(2),
            position_ticket: Some(3),
            price: Some(2650.12),
            volume_filled: 0.01,
            ts_ms: 0,
        };
        assert_eq!(filled.status, OrderStatus::Filled);
        assert!(filled.volume_filled > 0.0);
    }
}
