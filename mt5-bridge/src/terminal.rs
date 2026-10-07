//! MT5 terminal access: transport trait, typed client, and the pure
//! validation/normalization layer that runs before any order is placed.
//!
//! The bridge never talks to the terminal directly. It talks to an
//! [`TerminalTransport`], which in production is the EA link
//! (`crate::ea_link::EaLink`) and in tests/dev is the fake terminal
//! (`crate::fake::FakeTerminal`). That indirection is what makes the contract
//! tests in `tests/contract.rs` possible without a broker.
//!
//! Everything broker-facing is **fail closed**: unknown account modes, missing
//! symbol metadata, stale quotes, off-step volumes, invalid stops and
//! insufficient stop distances are rejected before `ORDER_SEND` leaves the
//! process.

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::proto::method;
use crate::snapshot::{Mt5Deal, Mt5Position};

/// Boxed, `Send` future used instead of an `async-trait` dependency.
pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Raw EA response: scalar fields plus optional list items.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EaResponse {
    pub fields: BTreeMap<String, String>,
    pub items: Vec<BTreeMap<String, String>>,
}

impl EaResponse {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str())
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        self.get(key).map(|s| s.to_string())
    }

    pub fn require_str(&self, key: &str, method: &str) -> Result<String, TerminalError> {
        self.get(key).map(|s| s.to_string()).ok_or_else(|| {
            TerminalError::Protocol(format!("{method} response is missing '{key}'"))
        })
    }

    pub fn require_f64(&self, key: &str, method: &str) -> Result<f64, TerminalError> {
        match self.get(key).and_then(|v| v.parse::<f64>().ok()) {
            Some(v) if v.is_finite() => Ok(v),
            _ => Err(TerminalError::Protocol(format!(
                "{method} response is missing a finite '{key}'"
            ))),
        }
    }

    pub fn require_i64(&self, key: &str, method: &str) -> Result<i64, TerminalError> {
        self.get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or_else(|| TerminalError::Protocol(format!("{method} response is missing '{key}'")))
    }

    pub fn opt_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.parse::<f64>().ok())
    }

    pub fn opt_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.parse::<i64>().ok())
    }

    pub fn opt_bool(&self, key: &str) -> Option<bool> {
        self.get(key).map(|v| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "ok"
            )
        })
    }

    fn item_f64(item: &BTreeMap<String, String>, key: &str) -> f64 {
        item.get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    }

    fn item_i64(item: &BTreeMap<String, String>, key: &str) -> i64 {
        item.get(key).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0)
    }

    fn item_str(item: &BTreeMap<String, String>, key: &str) -> String {
        item.get(key).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerminalError {
    /// No authenticated EA is connected — every order path must fail closed.
    NotConnected(String),
    Timeout { method: String, timeout_ms: u64 },
    Transport(String),
    Protocol(String),
    /// The EA answered `ERR`; `code` is the MT5 error/retcode when present.
    Ea { code: i64, message: String },
    /// A safety guard (demo-only, login match, write permission) refused.
    Guard(String),
    Validation(ValidationError),
}

impl fmt::Display for TerminalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TerminalError::NotConnected(what) => write!(f, "terminal not connected: {what}"),
            TerminalError::Timeout { method, timeout_ms } => {
                write!(f, "{method} timed out after {timeout_ms}ms")
            }
            TerminalError::Transport(msg) => write!(f, "terminal transport error: {msg}"),
            TerminalError::Protocol(msg) => write!(f, "EA protocol error: {msg}"),
            TerminalError::Ea { code, message } => {
                write!(f, "EA rejected the request (code {code}): {message}")
            }
            TerminalError::Guard(msg) => write!(f, "safety guard refused: {msg}"),
            TerminalError::Validation(err) => write!(f, "order validation failed: {err}"),
        }
    }
}

impl std::error::Error for TerminalError {}

impl From<ValidationError> for TerminalError {
    fn from(value: ValidationError) -> Self {
        TerminalError::Validation(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    SymbolNotFound {
        requested: String,
        available: usize,
    },
    SymbolMetadata(String),
    MarketDisabled {
        trade_mode: i64,
    },
    TradeModeNotFull {
        trade_mode: i64,
    },
    VolumeNotPositive(f64),
    VolumeBelowMin {
        volume: f64,
        min: f64,
    },
    VolumeAboveMax {
        volume: f64,
        max: f64,
    },
    VolumeNotOnStep {
        volume: f64,
        step: f64,
    },
    VolumeIndivisible {
        volume: f64,
        step: f64,
        min: f64,
    },
    InvalidPrice(String),
    MissingStopLoss,
    StopsTooClose {
        side: String,
        distance: f64,
        required: f64,
    },
    StopOnWrongSide {
        which: String,
        side: String,
        stop: f64,
        entry: f64,
    },
    StaleQuote {
        age_ms: i64,
        max_age_ms: u64,
    },
    PositionNotFound(i64),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::SymbolNotFound { requested, available } => write!(
                f,
                "symbol {requested} is not available in the terminal and has no explicit \
                 MT5_SYMBOL_MAP entry (tried {available} candidate name(s); broker suffixes are \
                 never guessed)"
            ),
            ValidationError::SymbolMetadata(msg) => write!(f, "unusable symbol metadata: {msg}"),
            ValidationError::MarketDisabled { trade_mode } => write!(
                f,
                "symbol trade mode {trade_mode} does not allow trading (0 = disabled, 3 = close only)"
            ),
            ValidationError::TradeModeNotFull { trade_mode } => write!(
                f,
                "symbol trade mode {trade_mode} is not FULL (4); the strategy needs both directions"
            ),
            ValidationError::VolumeNotPositive(v) => write!(f, "volume {v} is not positive"),
            ValidationError::VolumeBelowMin { volume, min } => {
                write!(f, "volume {volume} is below the broker minimum {min}")
            }
            ValidationError::VolumeAboveMax { volume, max } => {
                write!(f, "volume {volume} is above the broker maximum {max}")
            }
            ValidationError::VolumeNotOnStep { volume, step } => {
                write!(f, "volume {volume} is not a multiple of the volume step {step}")
            }
            ValidationError::VolumeIndivisible { volume, step, min } => write!(
                f,
                "volume {volume} cannot be expressed with volume step {step} at minimum {min}"
            ),
            ValidationError::InvalidPrice(msg) => write!(f, "invalid price: {msg}"),
            ValidationError::MissingStopLoss => write!(f, "stop loss is required for this venue"),
            ValidationError::StopsTooClose {
                side,
                distance,
                required,
            } => write!(
                f,
                "{side} stop distance {distance:.5} is inside the broker stops level {required:.5}"
            ),
            ValidationError::StopOnWrongSide {
                which,
                side,
                stop,
                entry,
            } => write!(
                f,
                "{which} {stop} is on the wrong side of the {side} entry {entry}"
            ),
            ValidationError::StaleQuote { age_ms, max_age_ms } => write!(
                f,
                "quote is {age_ms}ms old (max {max_age_ms}ms) — refusing to trade a stale market"
            ),
            ValidationError::PositionNotFound(ticket) => {
                write!(f, "position {ticket} is not open on this account")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Terminal connection + account state (EA `ACCOUNT` / `HELLO`).
#[derive(Debug, Clone, PartialEq)]
pub struct AccountInfo {
    pub login: i64,
    pub server: String,
    pub company: String,
    pub account_type: AccountType,
    pub currency: String,
    pub balance: f64,
    pub equity: f64,
    pub margin: f64,
    pub margin_free: f64,
    pub leverage: i64,
    pub trade_allowed: bool,
    pub connected: bool,
    pub terminal_build: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountType {
    Demo,
    Real,
    Contest,
    Unknown,
}

impl AccountType {
    pub fn from_wire(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "demo" | "demod" | "virtual" => AccountType::Demo,
            "real" | "live" => AccountType::Real,
            "contest" => AccountType::Contest,
            _ => AccountType::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AccountType::Demo => "demo",
            AccountType::Real => "real",
            AccountType::Contest => "contest",
            AccountType::Unknown => "unknown",
        }
    }

    pub fn is_demo(&self) -> bool {
        matches!(self, AccountType::Demo)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSpec {
    pub name: String,
    pub digits: i32,
    pub point: f64,
    pub tick_size: f64,
    pub tick_value: f64,
    pub contract_size: f64,
    pub volume_min: f64,
    pub volume_max: f64,
    pub volume_step: f64,
    pub trade_mode: i64,
    pub stops_level_points: i64,
    pub freeze_level_points: i64,
    pub bid: f64,
    pub ask: f64,
    pub spread_points: f64,
    pub quote_ts_ms: i64,
}

impl SymbolSpec {
    /// Minimum allowed stop distance from the entry in price terms.
    pub fn min_stop_distance(&self) -> f64 {
        (self.stops_level_points.max(0) as f64) * self.point
    }

    /// Money risk of `volume` lots over `distance` price units, in account
    /// currency. Uses the broker's tick value when available (the correct MT5
    /// formula), falling back to contract size for synthetic specs.
    pub fn risk_for_distance(&self, distance: f64, volume: f64) -> f64 {
        let distance = distance.abs();
        if self.tick_size > 0.0 && self.tick_value > 0.0 {
            (distance / self.tick_size) * self.tick_value * volume
        } else {
            distance * self.contract_size * volume
        }
    }

    pub fn notional(&self, price: f64, volume: f64) -> f64 {
        price * self.contract_size * volume
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn parse(value: &str) -> Option<Side> {
        match value.trim().to_ascii_lowercase().as_str() {
            "buy" | "long" => Some(Side::Buy),
            "sell" | "short" => Some(Side::Sell),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }

    pub fn from_position_type(value: i64) -> Option<Side> {
        // POSITION_TYPE_BUY = 0, POSITION_TYPE_SELL = 1.
        match value {
            0 => Some(Side::Buy),
            1 => Some(Side::Sell),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Filled,
    Partial,
    Rejected,
    Unknown,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Filled => "filled",
            OrderStatus::Partial => "partial",
            OrderStatus::Rejected => "rejected",
            OrderStatus::Unknown => "unknown",
        }
    }
}

/// MT5 trade server retcodes that mean "the broker did it".
pub const RETCODE_DONE: i64 = 10_009;
pub const RETCODE_DONE_PARTIAL: i64 = 10_010;
pub const RETCODE_PLACED: i64 = 10_008;

/// Map an MT5 retcode onto a fill status. Anything that is not an explicit
/// success is a rejection — never an assumed fill.
pub fn retcode_status(retcode: i64) -> OrderStatus {
    match retcode {
        RETCODE_DONE | RETCODE_PLACED => OrderStatus::Filled,
        RETCODE_DONE_PARTIAL => OrderStatus::Partial,
        _ => OrderStatus::Rejected,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderRequest {
    pub intent_id: String,
    pub broker_symbol: String,
    pub side: Side,
    pub volume: f64,
    pub sl: f64,
    pub tp: f64,
    pub deviation_points: u32,
    pub magic: i64,
    /// Broker order/position comment; carries the Node 4 intent id so an
    /// uncertain outcome can be resolved by an idempotent lookup.
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedOrder {
    pub side: Side,
    pub volume: f64,
    pub sl: f64,
    pub tp: f64,
    pub entry_ref: f64,
    pub sl_distance: f64,
    pub tp_distance: f64,
    pub risk_amount: f64,
    pub notional: f64,
    pub digits: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderSendResult {
    pub status: OrderStatus,
    pub retcode: i64,
    pub retcode_desc: String,
    pub order_ticket: Option<i64>,
    pub deal_ticket: Option<i64>,
    pub position_ticket: Option<i64>,
    pub price: Option<f64>,
    pub volume_filled: f64,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionCloseResult {
    pub position_ticket: i64,
    pub ok: bool,
    pub volume: f64,
    pub price: f64,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FindReport {
    pub positions: Vec<Mt5Position>,
    pub deals: Vec<Mt5Deal>,
}

/// Abstraction over "something that can answer EA methods".
pub trait TerminalTransport: Send + Sync {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: &'a [(&'a str, String)],
        timeout_ms: u64,
    ) -> BoxFut<'a, Result<EaResponse, TerminalError>>;

    fn describe(&self) -> String;
}

/// Typed, validating client used by the bridge service layer.
#[derive(Clone)]
pub struct TerminalClient {
    transport: Arc<dyn TerminalTransport>,
    read_timeout_ms: u64,
    write_timeout_ms: u64,
}

impl TerminalClient {
    pub fn new(
        transport: Arc<dyn TerminalTransport>,
        read_timeout_ms: u64,
        write_timeout_ms: u64,
    ) -> Self {
        Self {
            transport,
            read_timeout_ms,
            write_timeout_ms,
        }
    }

    pub fn describe(&self) -> String {
        self.transport.describe()
    }

    async fn call(
        &self,
        method: &str,
        params: Vec<(&str, String)>,
        timeout_ms: u64,
    ) -> Result<EaResponse, TerminalError> {
        self.transport.call(method, &params, timeout_ms).await
    }

    async fn read(&self, method: &str, params: Vec<(&str, String)>) -> Result<EaResponse, TerminalError> {
        self.call(method, params, self.read_timeout_ms).await
    }

    pub async fn account(&self) -> Result<AccountInfo, TerminalError> {
        let resp = self.read(method::ACCOUNT, Vec::new()).await?;
        account_from_response(&resp)
    }

    pub async fn symbol_info(&self, symbol: &str) -> Result<SymbolSpec, TerminalError> {
        let resp = self
            .read(method::SYMBOL, vec![("symbol", symbol.to_string())])
            .await?;
        symbol_from_response(&resp)
    }

    pub async fn quote(&self, symbol: &str) -> Result<(f64, f64, i64), TerminalError> {
        let resp = self
            .read(method::QUOTE, vec![("symbol", symbol.to_string())])
            .await?;
        Ok((
            resp.require_f64("bid", method::QUOTE)?,
            resp.require_f64("ask", method::QUOTE)?,
            resp.opt_i64("ts").unwrap_or(0),
        ))
    }

    pub async fn positions(&self, symbol: Option<&str>) -> Result<Vec<Mt5Position>, TerminalError> {
        let params = match symbol {
            Some(symbol) => vec![("symbol", symbol.to_string())],
            None => Vec::new(),
        };
        let resp = self.read(method::POSITIONS, params).await?;
        Ok(resp.items.iter().map(position_from_item).collect())
    }

    pub async fn deals(
        &self,
        from_ms: i64,
        to_ms: i64,
        limit: usize,
    ) -> Result<Vec<Mt5Deal>, TerminalError> {
        let resp = self
            .read(
                method::HISTORY,
                vec![
                    ("from", from_ms.to_string()),
                    ("to", to_ms.to_string()),
                    ("limit", limit.to_string()),
                ],
            )
            .await?;
        Ok(resp.items.iter().map(deal_from_item).collect())
    }

    pub async fn is_symbol_available(&self, symbol: &str) -> Result<bool, TerminalError> {
        match self.symbol_info(symbol).await {
            Ok(_) => Ok(true),
            Err(TerminalError::Ea { code, .. }) if code == 43_001 => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Send a market order with the configured write timeout.
    ///
    /// The caller decides what an uncertain outcome means; this layer never
    /// retries a write.
    pub async fn order_send(&self, req: &OrderRequest) -> Result<OrderSendResult, TerminalError> {
        self.call_order_send(req, self.write_timeout_ms).await
    }

    /// Send a market order with an explicit timeout (Node 4 may request a
    /// shorter or longer one than the bridge default).
    pub async fn call_order_send(
        &self,
        req: &OrderRequest,
        timeout_ms: u64,
    ) -> Result<OrderSendResult, TerminalError> {
        let params = vec![
            ("intent", req.intent_id.clone()),
            ("symbol", req.broker_symbol.clone()),
            ("side", req.side.as_str().to_string()),
            ("volume", format!("{:.8}", req.volume)),
            ("sl", format!("{:.8}", req.sl)),
            ("tp", format!("{:.8}", req.tp)),
            ("deviation", req.deviation_points.to_string()),
            ("magic", req.magic.to_string()),
            ("comment", req.comment.clone()),
        ];
        let resp = self.call(method::ORDER_SEND, params, timeout_ms).await?;
        Ok(order_result_from_response(&resp))
    }

    pub async fn position_modify(
        &self,
        ticket: i64,
        sl: f64,
        tp: f64,
    ) -> Result<(), TerminalError> {
        let resp = self
            .call(
                method::POS_MODIFY,
                vec![
                    ("ticket", ticket.to_string()),
                    ("sl", format!("{sl:.8}")),
                    ("tp", format!("{tp:.8}")),
                ],
                self.write_timeout_ms,
            )
            .await?;
        let retcode = resp.opt_i64("retcode").unwrap_or(0);
        if retcode != RETCODE_DONE {
            return Err(TerminalError::Ea {
                code: retcode,
                message: resp
                    .get_str("retcode_desc")
                    .unwrap_or_else(|| "position modify failed".into()),
            });
        }
        Ok(())
    }

    pub async fn position_close(
        &self,
        ticket: i64,
        volume: f64,
        deviation_points: u32,
    ) -> Result<OrderSendResult, TerminalError> {
        let resp = self
            .call(
                method::POS_CLOSE,
                vec![
                    ("ticket", ticket.to_string()),
                    ("volume", format!("{volume:.8}")),
                    ("deviation", deviation_points.to_string()),
                ],
                self.write_timeout_ms,
            )
            .await?;
        Ok(order_result_from_response(&resp))
    }

    pub async fn close_all(
        &self,
        symbol: Option<&str>,
        magic: i64,
        deviation_points: u32,
    ) -> Result<Vec<PositionCloseResult>, TerminalError> {
        let mut params = vec![
            ("deviation", deviation_points.to_string()),
            ("magic", magic.to_string()),
        ];
        if let Some(symbol) = symbol {
            params.push(("symbol", symbol.to_string()));
        }
        let resp = self
            .call(method::CLOSE_ALL, params, self.write_timeout_ms)
            .await?;
        Ok(resp
            .items
            .iter()
            .map(|item| PositionCloseResult {
                position_ticket: EaResponse::item_i64(item, "position"),
                ok: item
                    .get("ok")
                    .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
                    .unwrap_or(false),
                volume: EaResponse::item_f64(item, "volume"),
                price: EaResponse::item_f64(item, "price"),
                error: EaResponse::item_str(item, "error"),
            })
            .collect())
    }

    pub async fn order_cancel(&self, ticket: i64) -> Result<(), TerminalError> {
        let resp = self
            .call(
                method::ORDER_CANCEL,
                vec![("ticket", ticket.to_string())],
                self.write_timeout_ms,
            )
            .await?;
        let retcode = resp.opt_i64("retcode").unwrap_or(0);
        if retcode != RETCODE_DONE {
            return Err(TerminalError::Ea {
                code: retcode,
                message: resp
                    .get_str("retcode_desc")
                    .unwrap_or_else(|| "order cancel failed".into()),
            });
        }
        Ok(())
    }

    /// Idempotent read used to resolve an uncertain order outcome.
    pub async fn find_by_comment(
        &self,
        comment: &str,
        magic: i64,
        from_ms: i64,
    ) -> Result<FindReport, TerminalError> {
        let resp = self
            .read(
                method::FIND,
                vec![
                    ("comment", comment.to_string()),
                    ("magic", magic.to_string()),
                    ("from", from_ms.to_string()),
                ],
            )
            .await?;
        let mut report = FindReport::default();
        for item in &resp.items {
            match item.get("kind").map(|s| s.as_str()) {
                Some("position") => report.positions.push(position_from_item(item)),
                Some("deal") => report.deals.push(deal_from_item(item)),
                _ => {}
            }
        }
        Ok(report)
    }

    pub async fn ping(&self) -> Result<i64, TerminalError> {
        let started = now_ms();
        let resp = self.read(method::PING, Vec::new()).await?;
        Ok(resp.opt_i64("ts").unwrap_or_else(|| now_ms()) - started)
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Parse an `ACCOUNT` response.
pub fn account_from_response(resp: &EaResponse) -> Result<AccountInfo, TerminalError> {
    Ok(AccountInfo {
        login: resp.require_i64("login", method::ACCOUNT)?,
        server: resp.get_str("server").unwrap_or_default(),
        company: resp.get_str("company").unwrap_or_default(),
        account_type: AccountType::from_wire(resp.get("mode").unwrap_or("unknown")),
        currency: resp.get_str("currency").unwrap_or_else(|| "USD".into()),
        balance: resp.require_f64("balance", method::ACCOUNT)?,
        equity: resp.opt_f64("equity").unwrap_or(0.0),
        margin: resp.opt_f64("margin").unwrap_or(0.0),
        margin_free: resp.opt_f64("margin_free").unwrap_or(0.0),
        leverage: resp.opt_i64("leverage").unwrap_or(0),
        trade_allowed: resp.opt_bool("trade_allowed").unwrap_or(false),
        connected: resp.opt_bool("connected").unwrap_or(true),
        terminal_build: resp.opt_i64("build").unwrap_or(0),
    })
}

/// Parse a `SYMBOL` response into a validated spec.
pub fn symbol_from_response(resp: &EaResponse) -> Result<SymbolSpec, TerminalError> {
    let name = resp.require_str("name", method::SYMBOL)?;
    let digits = resp.opt_i64("digits").unwrap_or(0) as i32;
    let point = resp.require_f64("point", method::SYMBOL)?;
    if point <= 0.0 {
        return Err(TerminalError::Validation(ValidationError::SymbolMetadata(
            format!("{name} reports point={point}"),
        )));
    }
    let volume_step = resp.opt_f64("volume_step").unwrap_or(0.01);
    let volume_min = resp.opt_f64("volume_min").unwrap_or(0.01);
    let volume_max = resp.opt_f64("volume_max").unwrap_or(0.0);
    if volume_min <= 0.0 || volume_step <= 0.0 || volume_max <= 0.0 {
        return Err(TerminalError::Validation(ValidationError::SymbolMetadata(
            format!(
                "{name} reports volume_min={volume_min}, volume_max={volume_max}, \
                 volume_step={volume_step}"
            ),
        )));
    }
    let tick_size = resp.opt_f64("tick_size").unwrap_or(point);
    let contract_size = resp.opt_f64("contract_size").unwrap_or(0.0);
    if contract_size <= 0.0 {
        return Err(TerminalError::Validation(ValidationError::SymbolMetadata(
            format!("{name} reports contract_size={contract_size}"),
        )));
    }

    Ok(SymbolSpec {
        name,
        digits: digits.clamp(0, 8),
        point,
        tick_size: if tick_size > 0.0 { tick_size } else { point },
        tick_value: resp.opt_f64("tick_value").unwrap_or(0.0),
        contract_size,
        volume_min,
        volume_max,
        volume_step,
        trade_mode: resp.opt_i64("trade_mode").unwrap_or(-1),
        stops_level_points: resp.opt_i64("stops_level").unwrap_or(0),
        freeze_level_points: resp.opt_i64("freeze_level").unwrap_or(0),
        bid: resp.opt_f64("bid").unwrap_or(0.0),
        ask: resp.opt_f64("ask").unwrap_or(0.0),
        spread_points: resp.opt_f64("spread_points").unwrap_or(0.0),
        quote_ts_ms: resp.opt_i64("ts").unwrap_or(0),
    })
}

pub fn position_from_item(item: &BTreeMap<String, String>) -> Mt5Position {
    let side = item
        .get("side")
        .and_then(|v| Side::parse(v))
        .map(|s| s.as_str().to_string())
        .or_else(|| {
            item.get("type")
                .and_then(|v| v.parse::<i64>().ok())
                .and_then(Side::from_position_type)
                .map(|s| s.as_str().to_string())
        })
        .unwrap_or_else(|| "unknown".into());
    let profit = EaResponse::item_f64(item, "profit");
    Mt5Position {
        ticket: EaResponse::item_i64(item, "ticket"),
        symbol: EaResponse::item_str(item, "symbol"),
        side,
        volume: EaResponse::item_f64(item, "volume"),
        price_open: EaResponse::item_f64(item, "price_open"),
        sl: EaResponse::item_f64(item, "sl"),
        tp: EaResponse::item_f64(item, "tp"),
        profit,
        swap: EaResponse::item_f64(item, "swap"),
        comment: EaResponse::item_str(item, "comment"),
        magic: EaResponse::item_i64(item, "magic"),
        time_ms: EaResponse::item_i64(item, "time"),
        current_price: item
            .get("current_price")
            .and_then(|v| v.parse::<f64>().ok()),
        unrealized_pnl: item
            .get("unrealized_pnl")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(profit),
    }
}

pub fn deal_from_item(item: &BTreeMap<String, String>) -> Mt5Deal {
    let side = item
        .get("side")
        .and_then(|v| Side::parse(v))
        .map(|s| s.as_str().to_string())
        .or_else(|| {
            item.get("type")
                .and_then(|v| v.parse::<i64>().ok())
                .map(|t| match t {
                    0 => "buy".to_string(),
                    _ => "sell".to_string(),
                })
        })
        .unwrap_or_else(|| "unknown".into());
    let entry = match EaResponse::item_i64(item, "entry") {
        0 => "in",
        1 => "out",
        2 => "inout",
        3 => "out_by",
        _ => "",
    }
    .to_string();
    Mt5Deal {
        ticket: EaResponse::item_i64(item, "ticket"),
        order_ticket: EaResponse::item_i64(item, "order"),
        position_ticket: EaResponse::item_i64(item, "position"),
        symbol: EaResponse::item_str(item, "symbol"),
        side,
        volume: EaResponse::item_f64(item, "volume"),
        price: EaResponse::item_f64(item, "price"),
        profit: EaResponse::item_f64(item, "profit"),
        swap: EaResponse::item_f64(item, "swap"),
        commission: EaResponse::item_f64(item, "commission"),
        comment: EaResponse::item_str(item, "comment"),
        magic: EaResponse::item_i64(item, "magic"),
        time_ms: EaResponse::item_i64(item, "time"),
        entry: item
            .get("entry_name")
            .cloned()
            .unwrap_or(entry),
        reason: EaResponse::item_str(item, "reason"),
    }
}

pub fn order_result_from_response(resp: &EaResponse) -> OrderSendResult {
    let retcode = resp.opt_i64("retcode").unwrap_or(0);
    let status = resp
        .get("status")
        .and_then(|s| match s {
            "filled" => Some(OrderStatus::Filled),
            "partial" => Some(OrderStatus::Partial),
            "rejected" => Some(OrderStatus::Rejected),
            "unknown" => Some(OrderStatus::Unknown),
            _ => None,
        })
        .unwrap_or_else(|| retcode_status(retcode));
    OrderSendResult {
        status,
        retcode,
        retcode_desc: resp.get_str("retcode_desc").unwrap_or_default(),
        order_ticket: resp.opt_i64("order").filter(|v| *v > 0),
        deal_ticket: resp.opt_i64("deal").filter(|v| *v > 0),
        position_ticket: resp.opt_i64("position").filter(|v| *v > 0),
        price: resp.opt_f64("price").filter(|v| *v > 0.0),
        volume_filled: resp.opt_f64("volume").unwrap_or(0.0),
        ts_ms: resp.opt_i64("ts").unwrap_or_else(now_ms),
    }
}

/// Demo-only guard. Refuses every non-demo account, a mismatched login, and a
/// terminal that is not connected to a trade server.
pub fn demo_guard(
    account: &AccountInfo,
    expected_login: Option<i64>,
) -> Result<(), TerminalError> {
    if !account.account_type.is_demo() {
        return Err(TerminalError::Guard(format!(
            "account {} on {} reports mode '{}' — this bridge executes Deriv MT5 *demo* \
             accounts only",
            account.login,
            account.server,
            account.account_type.as_str()
        )));
    }
    if !account.connected {
        return Err(TerminalError::Guard(
            "the terminal is not connected to the trade server".into(),
        ));
    }
    if let Some(expected) = expected_login {
        if account.login != expected {
            return Err(TerminalError::Guard(format!(
                "terminal is logged into account {} but MT5_LOGIN is {}",
                account.login, expected
            )));
        }
    }
    Ok(())
}

fn round_to_digits(value: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits.clamp(0, 8));
    (value * factor).round() / factor
}

fn round_to_step(value: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return value;
    }
    let steps = (value / step + 1e-9).floor();
    let rounded = steps * step;
    (rounded * 1e8).round() / 1e8
}

/// Round a price to the symbol's precision.
pub fn normalize_price(price: f64, digits: i32) -> f64 {
    round_to_digits(price, digits)
}

/// Normalize a lot volume against the broker's min/max/step.
///
/// The volume is floored onto the step grid (never rounded up, so a request can
/// never exceed the intended size), then rejected if the result is below the
/// broker minimum or still off-grid.
pub fn normalize_volume(volume: f64, spec: &SymbolSpec) -> Result<f64, ValidationError> {
    if !volume.is_finite() || volume <= 0.0 {
        return Err(ValidationError::VolumeNotPositive(volume));
    }
    // Check the minimum against the *requested* volume, before any grid
    // flooring: 0.001 lots against a 0.01 minimum is "below the minimum", and
    // saying so is more useful than "not a multiple of the step".
    if volume + 1e-9 < spec.volume_min {
        return Err(ValidationError::VolumeBelowMin {
            volume,
            min: spec.volume_min,
        });
    }
    if volume > spec.volume_max {
        return Err(ValidationError::VolumeAboveMax {
            volume,
            max: spec.volume_max,
        });
    }
    let stepped = round_to_step(volume, spec.volume_step);
    let on_grid = ((volume - stepped).abs() <= spec.volume_step * 1e-6)
        || (stepped > 0.0 && ((volume / spec.volume_step).fract().abs() < 1e-6));
    if !on_grid {
        // Floor onto the grid instead of trading a larger size than intended.
        if stepped < spec.volume_min {
            return Err(ValidationError::VolumeIndivisible {
                volume,
                step: spec.volume_step,
                min: spec.volume_min,
            });
        }
        return Ok(stepped);
    }
    if stepped + 1e-9 < spec.volume_min {
        return Err(ValidationError::VolumeBelowMin {
            volume: stepped,
            min: spec.volume_min,
        });
    }
    if stepped <= 0.0 {
        return Err(ValidationError::VolumeBelowMin {
            volume: stepped,
            min: spec.volume_min,
        });
    }
    Ok(stepped)
}

/// Reject a quote older than the configured maximum age.
pub fn check_quote_fresh(
    quote_ts_ms: i64,
    now_ms_value: i64,
    max_age_ms: u64,
) -> Result<(), ValidationError> {
    if quote_ts_ms <= 0 {
        return Err(ValidationError::StaleQuote {
            age_ms: -1,
            max_age_ms,
        });
    }
    let age_ms = now_ms_value.saturating_sub(quote_ts_ms);
    if age_ms < 0 {
        // Clock skew between terminal and bridge: treat as fresh rather than
        // blocking trading on a timezone artefact, but never as negative age.
        return Ok(());
    }
    if age_ms as u64 > max_age_ms {
        return Err(ValidationError::StaleQuote { age_ms, max_age_ms });
    }
    Ok(())
}

/// Validate and normalize the stops of a market order.
pub fn validate_and_normalize_stops(
    side: Side,
    entry: f64,
    sl: Option<f64>,
    tp: Option<f64>,
    spec: &SymbolSpec,
) -> Result<(f64, f64, f64, f64), ValidationError> {
    if !entry.is_finite() || entry <= 0.0 {
        return Err(ValidationError::InvalidPrice(format!(
            "entry reference {entry}"
        )));
    }
    let required = spec.min_stop_distance();
    let sl = sl.ok_or(ValidationError::MissingStopLoss)?;
    if !sl.is_finite() || sl <= 0.0 {
        return Err(ValidationError::InvalidPrice(format!("stop loss {sl}")));
    }
    let tp = match tp {
        Some(tp) if tp.is_finite() && tp > 0.0 => tp,
        Some(tp) => return Err(ValidationError::InvalidPrice(format!("take profit {tp}"))),
        None => 0.0,
    };

    let sl_distance = (entry - sl).abs();
    let side_str = side.as_str();
    match side {
        Side::Buy => {
            if sl >= entry {
                return Err(ValidationError::StopOnWrongSide {
                    which: "stop loss".into(),
                    side: side_str.into(),
                    stop: sl,
                    entry,
                });
            }
            if tp > 0.0 && tp <= entry {
                return Err(ValidationError::StopOnWrongSide {
                    which: "take profit".into(),
                    side: side_str.into(),
                    stop: tp,
                    entry,
                });
            }
        }
        Side::Sell => {
            if sl <= entry {
                return Err(ValidationError::StopOnWrongSide {
                    which: "stop loss".into(),
                    side: side_str.into(),
                    stop: sl,
                    entry,
                });
            }
            if tp > 0.0 && tp >= entry {
                return Err(ValidationError::StopOnWrongSide {
                    which: "take profit".into(),
                    side: side_str.into(),
                    stop: tp,
                    entry,
                });
            }
        }
    }

    if sl_distance + 1e-9 < required {
        return Err(ValidationError::StopsTooClose {
            side: side_str.into(),
            distance: sl_distance,
            required,
        });
    }
    let tp_distance = if tp > 0.0 { (tp - entry).abs() } else { 0.0 };
    if tp > 0.0 && tp_distance + 1e-9 < required {
        return Err(ValidationError::StopsTooClose {
            side: side_str.into(),
            distance: tp_distance,
            required,
        });
    }

    Ok((
        normalize_price(sl, spec.digits),
        if tp > 0.0 {
            normalize_price(tp, spec.digits)
        } else {
            0.0
        },
        sl_distance,
        tp_distance,
    ))
}

/// Full pre-trade validation: trade mode, quote freshness, volume, stops, and
/// the resulting risk in account currency.
pub fn validate_order(
    side: Side,
    volume: f64,
    sl: Option<f64>,
    tp: Option<f64>,
    entry_ref: f64,
    spec: &SymbolSpec,
    now_ms_value: i64,
    max_quote_age_ms: u64,
) -> Result<NormalizedOrder, ValidationError> {
    if spec.trade_mode == 0 {
        return Err(ValidationError::MarketDisabled {
            trade_mode: spec.trade_mode,
        });
    }
    if spec.trade_mode == 3 {
        return Err(ValidationError::MarketDisabled {
            trade_mode: spec.trade_mode,
        });
    }
    if spec.trade_mode != 4 {
        return Err(ValidationError::TradeModeNotFull {
            trade_mode: spec.trade_mode,
        });
    }
    check_quote_fresh(spec.quote_ts_ms, now_ms_value, max_quote_age_ms)?;

    let reference = if entry_ref.is_finite() && entry_ref > 0.0 {
        entry_ref
    } else if side == Side::Buy && spec.ask > 0.0 {
        spec.ask
    } else if spec.bid > 0.0 {
        spec.bid
    } else {
        return Err(ValidationError::InvalidPrice(
            "no entry reference and no usable quote".into(),
        ));
    };

    let normalized_volume = normalize_volume(volume, spec)?;
    let (normalized_sl, normalized_tp, sl_distance, tp_distance) =
        validate_and_normalize_stops(side, reference, sl, tp, spec)?;

    Ok(NormalizedOrder {
        side,
        volume: normalized_volume,
        sl: normalized_sl,
        tp: normalized_tp,
        entry_ref: normalize_price(reference, spec.digits),
        sl_distance,
        tp_distance,
        risk_amount: spec.risk_for_distance(sl_distance, normalized_volume),
        notional: spec.notional(reference, normalized_volume),
        digits: spec.digits,
    })
}

/// Build the broker comment that carries the Node 4 intent id, so an uncertain
/// outcome can always be resolved by an idempotent `FIND`.
///
/// MT5 comments are limited (typically 31 characters), so the id is truncated
/// and the comment is sanitized to a compact, space-free token.
pub fn intent_comment(intent_id: &str) -> String {
    let sanitized: String = intent_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let mut comment = String::with_capacity(31);
    for ch in sanitized.chars() {
        if comment.len() >= 31 {
            break;
        }
        comment.push(ch);
    }
    if comment.is_empty() {
        comment.push_str("N3");
    }
    comment
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xauusd_spec() -> SymbolSpec {
        SymbolSpec {
            name: "XAUUSD".into(),
            digits: 2,
            point: 0.01,
            tick_size: 0.01,
            tick_value: 1.0,
            contract_size: 100.0,
            volume_min: 0.01,
            volume_max: 50.0,
            volume_step: 0.01,
            trade_mode: 4,
            stops_level_points: 100,
            freeze_level_points: 0,
            bid: 2649.9,
            ask: 2650.1,
            spread_points: 20.0,
            quote_ts_ms: 1_700_000_000_000,
        }
    }

    fn demo_account() -> AccountInfo {
        AccountInfo {
            login: 123_456,
            server: "Deriv-Demo".into(),
            company: "Deriv".into(),
            account_type: AccountType::Demo,
            currency: "USD".into(),
            balance: 10_000.0,
            equity: 10_000.0,
            margin: 0.0,
            margin_free: 10_000.0,
            leverage: 100,
            trade_allowed: true,
            connected: true,
            terminal_build: 4755,
        }
    }

    #[test]
    fn account_type_mapping_is_conservative() {
        assert_eq!(AccountType::from_wire("Demo"), AccountType::Demo);
        assert_eq!(AccountType::from_wire("demo"), AccountType::Demo);
        assert_eq!(AccountType::from_wire("real"), AccountType::Real);
        assert_eq!(AccountType::from_wire("REAL"), AccountType::Real);
        assert_eq!(AccountType::from_wire("contest"), AccountType::Contest);
        // Anything unrecognised must NOT be treated as demo.
        assert_eq!(AccountType::from_wire(""), AccountType::Unknown);
        assert_eq!(AccountType::from_wire("virtual-real"), AccountType::Unknown);
        assert!(!AccountType::Unknown.is_demo());
    }

    #[test]
    fn demo_guard_rejects_real_contest_unknown_and_mismatched_logins() {
        assert!(demo_guard(&demo_account(), Some(123_456)).is_ok());

        for account_type in [AccountType::Real, AccountType::Contest, AccountType::Unknown] {
            let account = AccountInfo {
                account_type,
                ..demo_account()
            };
            let err = demo_guard(&account, Some(123_456)).unwrap_err();
            assert!(
                matches!(err, TerminalError::Guard(_)),
                "expected guard refusal for {account_type:?}"
            );
        }

        let err = demo_guard(&demo_account(), Some(999_999)).unwrap_err();
        assert!(err.to_string().contains("MT5_LOGIN"));

        let disconnected = AccountInfo {
            connected: false,
            ..demo_account()
        };
        assert!(demo_guard(&disconnected, None).is_err());
    }

    #[test]
    fn volume_is_floored_onto_the_step_grid_and_bounds_are_enforced() {
        let spec = xauusd_spec();
        assert_eq!(normalize_volume(0.01, &spec).unwrap(), 0.01);
        assert_eq!(normalize_volume(0.03, &spec).unwrap(), 0.03);
        // 0.014 -> 0.01 (floored, never rounded up to a bigger size).
        assert_eq!(normalize_volume(0.014, &spec).unwrap(), 0.01);
        // Exactly the minimum is fine; below it is not.
        assert!(matches!(
            normalize_volume(0.001, &spec).unwrap_err(),
            ValidationError::VolumeBelowMin { .. }
        ));
        assert!(matches!(
            normalize_volume(100.0, &spec).unwrap_err(),
            ValidationError::VolumeAboveMax { .. }
        ));
        assert!(matches!(
            normalize_volume(0.0, &spec).unwrap_err(),
            ValidationError::VolumeNotPositive(_)
        ));

        // A coarser step rejects an indistinguishable request.
        let coarse = SymbolSpec {
            volume_step: 0.1,
            ..xauusd_spec()
        };
        assert_eq!(normalize_volume(0.5, &coarse).unwrap(), 0.5);
        assert!(matches!(
            normalize_volume(0.05, &coarse).unwrap_err(),
            ValidationError::VolumeBelowMin { .. } | ValidationError::VolumeIndivisible { .. }
        ));
    }

    #[test]
    fn stops_must_be_on_the_correct_side_and_respect_the_stops_level() {
        let spec = xauusd_spec();
        let now = spec.quote_ts_ms;

        // Buy: SL 2.00 below, TP 6.00 above -> valid, risk = $2.00/oz * 0.01 lots.
        let ok = validate_order(
            Side::Buy,
            0.01,
            Some(2648.10),
            Some(2656.10),
            2650.10,
            &spec,
            now,
            3_000,
        )
        .unwrap();
        assert_eq!(ok.volume, 0.01);
        assert_eq!(ok.sl, 2648.10);
        assert_eq!(ok.tp, 2656.10);
        assert!((ok.sl_distance - 2.0).abs() < 1e-9);
        // 2.00 / 0.01 tick = 200 ticks * $1 tick value * 0.01 lots = $2.00.
        assert!((ok.risk_amount - 2.0).abs() < 1e-9);
        assert!((ok.notional - (2650.10 * 100.0 * 0.01)).abs() < 1e-6);

        // Stop loss above a buy entry is refused.
        assert!(matches!(
            validate_order(
                Side::Buy,
                0.01,
                Some(2651.0),
                Some(2660.0),
                2650.10,
                &spec,
                now,
                3_000
            )
            .unwrap_err(),
            ValidationError::StopOnWrongSide { .. }
        ));

        // Sell with SL below the entry is refused.
        assert!(matches!(
            validate_order(
                Side::Sell,
                0.01,
                Some(2649.0),
                Some(2640.0),
                2650.10,
                &spec,
                now,
                3_000
            )
            .unwrap_err(),
            ValidationError::StopOnWrongSide { .. }
        ));

        // A stop inside the broker stops level (100 points = $1.00) is refused.
        assert!(matches!(
            validate_order(
                Side::Buy,
                0.01,
                Some(2649.50),
                Some(2660.0),
                2650.10,
                &spec,
                now,
                3_000
            )
            .unwrap_err(),
            ValidationError::StopsTooClose { .. }
        ));

        // A missing stop loss is refused (this venue always requires one).
        assert_eq!(
            validate_order(Side::Buy, 0.01, None, Some(2660.0), 2650.10, &spec, now, 3_000)
                .unwrap_err(),
            ValidationError::MissingStopLoss
        );
    }

    #[test]
    fn stale_quotes_and_closed_markets_are_refused() {
        let spec = xauusd_spec();
        assert!(matches!(
            validate_order(
                Side::Buy,
                0.01,
                Some(2648.0),
                Some(2656.0),
                2650.10,
                &spec,
                spec.quote_ts_ms + 10_000,
                3_000
            )
            .unwrap_err(),
            ValidationError::StaleQuote { .. }
        ));

        // A quote without a timestamp is never considered fresh.
        let no_ts = SymbolSpec {
            quote_ts_ms: 0,
            ..xauusd_spec()
        };
        assert!(check_quote_fresh(no_ts.quote_ts_ms, 1_700_000_000_000, 3_000).is_err());

        for trade_mode in [0, 1, 2, 3] {
            let restricted = SymbolSpec {
                trade_mode,
                ..xauusd_spec()
            };
            assert!(validate_order(
                Side::Buy,
                0.01,
                Some(2648.0),
                Some(2656.0),
                2650.10,
                &restricted,
                spec.quote_ts_ms,
                3_000
            )
            .is_err());
        }
    }

    #[test]
    fn retcodes_map_to_statuses_without_assuming_success() {
        assert_eq!(retcode_status(10_009), OrderStatus::Filled);
        assert_eq!(retcode_status(10_010), OrderStatus::Partial);
        assert_eq!(retcode_status(10_008), OrderStatus::Filled);
        for retcode in [10_004, 10_006, 10_016, 10_018, 10_019, 10_030, 10_031, 0] {
            assert_eq!(
                retcode_status(retcode),
                OrderStatus::Rejected,
                "retcode {retcode} must never be treated as a fill"
            );
        }
    }

    #[test]
    fn intent_comments_are_space_free_broker_safe_tokens() {
        assert_eq!(intent_comment("N3-1759700000000-PW PoC"), "N3-1759700000000-PWPoC");
        let long = intent_comment(&"a".repeat(80));
        assert!(long.len() <= 31);
        assert_eq!(intent_comment("!!! ???"), "N3");
    }

    #[test]
    fn order_result_never_reports_a_fill_without_a_volume() {
        let mut fields = BTreeMap::new();
        fields.insert("status".into(), "filled".into());
        fields.insert("retcode".into(), "10009".into());
        fields.insert("position".into(), "789".into());
        fields.insert("price".into(), "2650.12".into());
        fields.insert("volume".into(), "0.01".into());
        let resp = EaResponse {
            fields,
            items: Vec::new(),
        };
        let result = order_result_from_response(&resp);
        assert_eq!(result.status, OrderStatus::Filled);
        assert_eq!(result.position_ticket, Some(789));
        assert_eq!(result.volume_filled, 0.01);

        // A `filled` retcode with no volume still parses as filled but carries
        // zero filled volume, which `OrderOutcome::is_confirmed_fill` rejects.
        let mut fields = BTreeMap::new();
        fields.insert("retcode".into(), "10009".into());
        let resp = EaResponse {
            fields,
            items: Vec::new(),
        };
        let result = order_result_from_response(&resp);
        assert_eq!(result.status, OrderStatus::Filled);
        assert_eq!(result.volume_filled, 0.0);
    }

    #[test]
    fn symbol_metadata_without_contract_size_is_refused() {
        let mut fields = BTreeMap::new();
        fields.insert("name".into(), "XAUUSD".into());
        fields.insert("point".into(), "0.01".into());
        fields.insert("digits".into(), "2".into());
        fields.insert("volume_min".into(), "0.01".into());
        fields.insert("volume_max".into(), "50".into());
        fields.insert("volume_step".into(), "0.01".into());
        let resp = EaResponse {
            fields,
            items: Vec::new(),
        };
        assert!(symbol_from_response(&resp).is_err());
    }
}
