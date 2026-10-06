//! In-memory fake MT5 terminal.
//!
//! Used by `MT5_SIM_TERMINAL=1` (development without a terminal) and by the
//! contract tests in `tests/contract.rs`, which is how the acceptance criteria
//! from the design document are exercised without a broker: accepted orders,
//! rejected orders, duplicate request ids, bridge disconnect, stale quotes and
//! account-mode mismatch.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::bridge::{LinkStatusProvider, LinkStatusView};
use crate::proto::method;
use crate::snapshot::{Mt5Deal, Mt5Position};
use crate::terminal::{
    now_ms, AccountType, BoxFut, EaResponse, SymbolSpec, TerminalError, TerminalTransport,
};

pub const UNKNOWN_SYMBOL_CODE: i64 = 43_001;

/// How the fake answers the next `ORDER_SEND`.
#[derive(Debug, Clone, PartialEq)]
pub enum OrderMode {
    /// Fill immediately at the current quote.
    Fill,
    /// Fill, but answer with a transport timeout — the classic "did it go
    /// through?" case that must be resolved by reconciliation.
    FillThenTimeout,
    /// Fill but report `filled` without a volume, which is contradictory and
    /// must not be treated as a confirmation.
    FillWithoutVolume,
    /// Partially fill.
    Partial { volume: f64 },
    /// Reject with an MT5 retcode.
    Reject { retcode: i64, description: String },
}

#[derive(Debug, Clone)]
pub struct FakeConfig {
    pub account_type: AccountType,
    pub trade_mode: i64,
    pub trade_allowed: bool,
    pub terminal_connected: bool,
    pub quote_age_ms: i64,
    pub order_mode: OrderMode,
    pub latency_ms: u64,
    /// When set, every write method is refused with this message (models a
    /// read-only EA link, i.e. a missing `MT5_EA_TOKEN`).
    pub refuse_writes: Option<String>,
    /// Symbols the terminal offers (name → spec).
    pub symbols: HashMap<String, SymbolSpec>,
}

impl Default for FakeConfig {
    fn default() -> Self {
        let mut symbols = HashMap::new();
        symbols.insert("XAUUSD".to_string(), xauusd_spec("XAUUSD"));
        symbols.insert("XAUUSD.a".to_string(), xauusd_spec("XAUUSD.a"));
        Self {
            account_type: AccountType::Demo,
            trade_mode: 4,
            trade_allowed: true,
            terminal_connected: true,
            quote_age_ms: 250,
            order_mode: OrderMode::Fill,
            latency_ms: 0,
            refuse_writes: None,
            symbols,
        }
    }
}

/// A terminal that is not reachable at all (no EA session connected). Every
/// call fails closed with `NotConnected`.
pub struct DisconnectedTerminal;

impl TerminalTransport for DisconnectedTerminal {
    fn call<'a>(
        &'a self,
        method: &'a str,
        _params: &'a [(&'a str, String)],
        _timeout_ms: u64,
    ) -> BoxFut<'a, Result<EaResponse, TerminalError>> {
        Box::pin(async move {
            Err(TerminalError::NotConnected(format!(
                "no EA session is connected (asked for {method})"
            )))
        })
    }

    fn describe(&self) -> String {
        "disconnected-terminal".to_string()
    }
}

pub fn xauusd_spec(name: &str) -> SymbolSpec {
    SymbolSpec {
        name: name.to_string(),
        digits: 2,
        point: 0.01,
        tick_size: 0.01,
        // $1 per tick per lot: 1 lot = 100 oz, a $0.01 move = $1.
        tick_value: 1.0,
        contract_size: 100.0,
        volume_min: 0.01,
        volume_max: 50.0,
        volume_step: 0.01,
        trade_mode: 4,
        stops_level_points: 100,
        freeze_level_points: 0,
        bid: 2649.90,
        ask: 2650.10,
        spread_points: 20.0,
        quote_ts_ms: 0,
    }
}

struct FakeInner {
    account_type: AccountType,
    trade_mode: i64,
    trade_allowed: bool,
    terminal_connected: bool,
    quote_age_ms: i64,
    order_mode: OrderMode,
    latency_ms: u64,
    /// When set, every write method is refused with this message (models a
    /// read-only EA link, i.e. a missing `MT5_EA_TOKEN`).
    refuse_writes: Option<String>,
    symbols: HashMap<String, SymbolSpec>,
    positions: Vec<Mt5Position>,
    deals: Vec<Mt5Deal>,
    next_ticket: i64,
    /// Every order the bridge actually handed to the terminal — the "broker
    /// side" record used to assert that duplicates never reach the market.
    orders_sent: Vec<BTreeMap<String, String>>,
}

/// `FakeConfig::trade_mode` is the terminal-wide trade-mode knob tests use; it
/// is applied to every symbol so `SYMBOL` answers with it.
fn with_trade_mode(
    mut symbols: HashMap<String, SymbolSpec>,
    trade_mode: i64,
) -> HashMap<String, SymbolSpec> {
    for spec in symbols.values_mut() {
        spec.trade_mode = trade_mode;
    }
    symbols
}

pub struct FakeTerminal {
    inner: Mutex<FakeInner>,
    calls: AtomicU64,
}

impl FakeTerminal {
    pub fn new(config: FakeConfig) -> Arc<Self> {
        let inner = FakeInner {
            account_type: config.account_type,
            trade_mode: config.trade_mode,
            trade_allowed: config.trade_allowed,
            terminal_connected: config.terminal_connected,
            quote_age_ms: config.quote_age_ms,
            order_mode: config.order_mode,
            latency_ms: config.latency_ms,
            refuse_writes: config.refuse_writes,
            symbols: with_trade_mode(config.symbols, config.trade_mode),
            positions: Vec::new(),
            deals: Vec::new(),
            next_ticket: 1_000,
            orders_sent: Vec::new(),
        };
        Arc::new(Self {
            inner: Mutex::new(inner),
            calls: AtomicU64::new(0),
        })
    }

    pub fn demo() -> Arc<Self> {
        Self::new(FakeConfig::default())
    }

    /// A terminal logged into a real account (used to prove the guard fails).
    pub fn real_account() -> Arc<Self> {
        Self::new(FakeConfig {
            account_type: AccountType::Real,
            ..FakeConfig::default()
        })
    }

    pub async fn set_account_type(&self, account_type: AccountType) {
        self.inner.lock().await.account_type = account_type;
    }

    pub async fn set_order_mode(&self, mode: OrderMode) {
        self.inner.lock().await.order_mode = mode;
    }

    pub async fn set_quote_age_ms(&self, age: i64) {
        self.inner.lock().await.quote_age_ms = age;
    }

    pub async fn set_trade_mode(&self, trade_mode: i64) {
        let mut inner = self.inner.lock().await;
        inner.trade_mode = trade_mode;
        for spec in inner.symbols.values_mut() {
            spec.trade_mode = trade_mode;
        }
    }

    pub async fn add_position(&self, position: Mt5Position) {
        self.inner.lock().await.positions.push(position);
    }

    pub async fn positions(&self) -> Vec<Mt5Position> {
        self.inner.lock().await.positions.clone()
    }

    pub async fn deals(&self) -> Vec<Mt5Deal> {
        self.inner.lock().await.deals.clone()
    }

    pub async fn orders_sent(&self) -> Vec<BTreeMap<String, String>> {
        self.inner.lock().await.orders_sent.clone()
    }

    pub async fn order_send_count(&self) -> usize {
        self.inner.lock().await.orders_sent.len()
    }
}

fn map(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

fn b(value: bool) -> String {
    if value {
        "1".into()
    } else {
        "0".into()
    }
}

fn position_fields(position: &Mt5Position) -> BTreeMap<String, String> {
    map(&[
        ("kind", "position".into()),
        ("ticket", position.ticket.to_string()),
        ("symbol", position.symbol.clone()),
        ("side", position.side.clone()),
        ("volume", format!("{:.8}", position.volume)),
        ("price_open", format!("{:.8}", position.price_open)),
        ("sl", format!("{:.8}", position.sl)),
        ("tp", format!("{:.8}", position.tp)),
        ("profit", format!("{:.8}", position.profit)),
        ("swap", format!("{:.8}", position.swap)),
        ("comment", position.comment.clone()),
        ("magic", position.magic.to_string()),
        ("time", position.time_ms.to_string()),
        (
            "current_price",
            format!("{:.8}", position.current_price.unwrap_or(position.price_open)),
        ),
        (
            "unrealized_pnl",
            format!("{:.8}", position.unrealized_pnl),
        ),
    ])
}

fn deal_fields(deal: &Mt5Deal) -> BTreeMap<String, String> {
    map(&[
        ("kind", "deal".into()),
        ("ticket", deal.ticket.to_string()),
        ("order", deal.order_ticket.to_string()),
        ("position", deal.position_ticket.to_string()),
        ("symbol", deal.symbol.clone()),
        ("side", deal.side.clone()),
        ("volume", format!("{:.8}", deal.volume)),
        ("price", format!("{:.8}", deal.price)),
        ("profit", format!("{:.8}", deal.profit)),
        ("swap", format!("{:.8}", deal.swap)),
        ("commission", format!("{:.8}", deal.commission)),
        ("comment", deal.comment.clone()),
        ("magic", deal.magic.to_string()),
        ("time", deal.time_ms.to_string()),
        (
            "entry",
            match deal.entry.as_str() {
                "in" => "0",
                "out" => "1",
                "inout" => "2",
                _ => "1",
            }
            .to_string(),
        ),
        ("entry_name", deal.entry.clone()),
        ("reason", deal.reason.clone()),
    ])
}

impl FakeTerminal {
    async fn handle(
        &self,
        method: &str,
        params: &[(&str, String)],
    ) -> Result<EaResponse, TerminalError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let latency = { self.inner.lock().await.latency_ms };
        if latency > 0 {
            tokio::time::sleep(Duration::from_millis(latency)).await;
        }
        let param = |key: &str| -> Option<String> {
            params
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
        };

        // A read-only link refuses writes before they reach the terminal.
        {
            let inner = self.inner.lock().await;
            if let Some(reason) = inner.refuse_writes.clone() {
                if crate::ea_link::is_write_method(method) {
                    return Err(TerminalError::Guard(reason));
                }
            }
        }

        match method {
            method::PING => Ok(EaResponse {
                fields: map(&[("ts", now_ms().to_string())]),
                items: Vec::new(),
            }),
            method::ACCOUNT => {
                let inner = self.inner.lock().await;
                Ok(EaResponse {
                    fields: map(&[
                        ("login", "123456".to_string()),
                        ("server", "Deriv-Demo".to_string()),
                        ("company", "Deriv Investments".to_string()),
                        ("mode", inner.account_type.as_str().to_string()),
                        ("currency", "USD".to_string()),
                        ("balance", "10000.00".to_string()),
                        ("equity", "10000.00".to_string()),
                        ("margin", "0.00".to_string()),
                        ("margin_free", "10000.00".to_string()),
                        ("leverage", "100".to_string()),
                        ("trade_allowed", b(inner.trade_allowed)),
                        ("connected", b(inner.terminal_connected)),
                        ("build", "4755".to_string()),
                    ]),
                    items: Vec::new(),
                })
            }
            method::SYMBOL => {
                let name = param("symbol").unwrap_or_default();
                let inner = self.inner.lock().await;
                match inner.symbols.get(&name) {
                    Some(spec) => Ok(EaResponse {
                        fields: map(&[
                            ("name", spec.name.clone()),
                            ("digits", spec.digits.to_string()),
                            ("point", format!("{:.8}", spec.point)),
                            ("tick_size", format!("{:.8}", spec.tick_size)),
                            ("tick_value", format!("{:.8}", spec.tick_value)),
                            ("contract_size", format!("{:.8}", spec.contract_size)),
                            ("volume_min", format!("{:.8}", spec.volume_min)),
                            ("volume_max", format!("{:.8}", spec.volume_max)),
                            ("volume_step", format!("{:.8}", spec.volume_step)),
                            ("trade_mode", spec.trade_mode.to_string()),
                            ("stops_level", spec.stops_level_points.to_string()),
                            ("freeze_level", spec.freeze_level_points.to_string()),
                            ("bid", format!("{:.8}", spec.bid)),
                            ("ask", format!("{:.8}", spec.ask)),
                            ("spread_points", format!("{:.8}", spec.spread_points)),
                            (
                                "ts",
                                (now_ms() - inner.quote_age_ms).to_string(),
                            ),
                        ]),
                        items: Vec::new(),
                    }),
                    None => Err(TerminalError::Ea {
                        code: UNKNOWN_SYMBOL_CODE,
                        message: format!("unknown symbol {name}"),
                    }),
                }
            }
            method::QUOTE => {
                let name = param("symbol").unwrap_or_default();
                let inner = self.inner.lock().await;
                let Some(spec) = inner.symbols.get(&name) else {
                    return Err(TerminalError::Ea {
                        code: UNKNOWN_SYMBOL_CODE,
                        message: format!("unknown symbol {name}"),
                    });
                };
                Ok(EaResponse {
                    fields: map(&[
                        ("bid", format!("{:.8}", spec.bid)),
                        ("ask", format!("{:.8}", spec.ask)),
                        ("ts", (now_ms() - inner.quote_age_ms).to_string()),
                    ]),
                    items: Vec::new(),
                })
            }
            method::POSITIONS => {
                let symbol = param("symbol");
                let inner = self.inner.lock().await;
                let items = inner
                    .positions
                    .iter()
                    .filter(|p| symbol.as_deref().map(|s| s == p.symbol).unwrap_or(true))
                    .map(position_fields)
                    .collect();
                Ok(EaResponse {
                    fields: map(&[("count", inner.positions.len().to_string())]),
                    items,
                })
            }
            method::ORDERS => Ok(EaResponse {
                fields: map(&[("count", "0".to_string())]),
                items: Vec::new(),
            }),
            method::HISTORY => {
                let from = param("from").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                let to = param("to")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(i64::MAX);
                let limit = param("limit")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(100);
                let inner = self.inner.lock().await;
                let items: Vec<BTreeMap<String, String>> = inner
                    .deals
                    .iter()
                    .filter(|d| d.time_ms >= from && d.time_ms <= to)
                    .take(limit)
                    .map(deal_fields)
                    .collect();
                Ok(EaResponse {
                    fields: map(&[("count", items.len().to_string())]),
                    items,
                })
            }
            method::FIND => {
                let comment = param("comment").unwrap_or_default();
                let magic = param("magic")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let from = param("from")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let inner = self.inner.lock().await;
                let mut items: Vec<BTreeMap<String, String>> = inner
                    .positions
                    .iter()
                    .filter(|p| {
                        p.magic == magic
                            && p.time_ms >= from
                            && (p.comment.contains(&comment) || comment.is_empty())
                    })
                    .map(position_fields)
                    .collect();
                items.extend(
                    inner
                        .deals
                        .iter()
                        .filter(|d| {
                            d.magic == magic
                                && d.time_ms >= from
                                && (d.comment.contains(&comment) || comment.is_empty())
                        })
                        .map(deal_fields),
                );
                Ok(EaResponse {
                    fields: map(&[("count", items.len().to_string())]),
                    items,
                })
            }
            method::ORDER_SEND => {
                let mut inner = self.inner.lock().await;
                let symbol = param("symbol").unwrap_or_default();
                let side = param("side").unwrap_or_default();
                let volume: f64 = param("volume")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0);
                let sl: f64 = param("sl").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                let tp: f64 = param("tp").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                let magic = param("magic")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let comment = param("comment").unwrap_or_default();
                let intent = param("intent").unwrap_or_default();

                // The "broker side" record: duplicates must never appear here
                // more than once for the same intent.
                inner.orders_sent.push(map(&[
                    ("intent", intent),
                    ("symbol", symbol.clone()),
                    ("side", side.clone()),
                    ("volume", format!("{volume:.8}")),
                    ("sl", format!("{sl:.8}")),
                    ("tp", format!("{tp:.8}")),
                    ("magic", magic.to_string()),
                    ("comment", comment.clone()),
                ]));

                let spec = match inner.symbols.get(&symbol) {
                    Some(spec) => spec.clone(),
                    None => {
                        return Ok(EaResponse {
                            fields: map(&[
                                ("status", "rejected".to_string()),
                                ("retcode", "10047".to_string()),
                                ("retcode_desc", "invalid symbol".to_string()),
                            ]),
                            items: Vec::new(),
                        })
                    }
                };

                match inner.order_mode.clone() {
                    OrderMode::Reject {
                        retcode,
                        description,
                    } => Ok(EaResponse {
                        fields: map(&[
                            ("status", "rejected".to_string()),
                            ("retcode", retcode.to_string()),
                            ("retcode_desc", description),
                        ]),
                        items: Vec::new(),
                    }),
                    mode => {
                        let fill_price = if side == "sell" { spec.bid } else { spec.ask };
                        let position_ticket = inner.next_ticket;
                        let order_ticket = inner.next_ticket + 1;
                        let deal_ticket = inner.next_ticket + 2;
                        inner.next_ticket += 3;
                        let filled = match mode {
                            OrderMode::Partial { volume: filled } => filled.min(volume),
                            _ => volume,
                        };

                        inner.positions.push(Mt5Position {
                            ticket: position_ticket,
                            symbol: spec.name.clone(),
                            side: side.clone(),
                            volume: filled,
                            price_open: fill_price,
                            sl,
                            tp,
                            profit: 0.0,
                            swap: 0.0,
                            comment: comment.clone(),
                            magic,
                            time_ms: now_ms(),
                            current_price: Some(fill_price),
                            unrealized_pnl: 0.0,
                        });
                        inner.deals.push(Mt5Deal {
                            ticket: deal_ticket,
                            order_ticket,
                            position_ticket,
                            symbol: spec.name.clone(),
                            side: side.clone(),
                            volume: filled,
                            price: fill_price,
                            profit: 0.0,
                            swap: 0.0,
                            commission: 0.0,
                            comment: comment.clone(),
                            magic,
                            time_ms: now_ms(),
                            entry: "in".into(),
                            reason: "market".into(),
                        });

                        match inner.order_mode {
                            OrderMode::FillThenTimeout => Err(TerminalError::Timeout {
                                method: method::ORDER_SEND.to_string(),
                                timeout_ms: 15_000,
                            }),
                            OrderMode::FillWithoutVolume => Ok(EaResponse {
                                fields: map(&[
                                    ("status", "filled".to_string()),
                                    ("retcode", "10009".to_string()),
                                    ("position", position_ticket.to_string()),
                                    ("order", order_ticket.to_string()),
                                    ("deal", deal_ticket.to_string()),
                                    ("price", format!("{fill_price:.8}")),
                                ]),
                                items: Vec::new(),
                            }),
                            _ => Ok(EaResponse {
                                fields: map(&[
                                    (
                                        "status",
                                        if matches!(mode, OrderMode::Partial { .. }) {
                                            "partial"
                                        } else {
                                            "filled"
                                        }
                                        .to_string(),
                                    ),
                                    ("retcode", "10009".to_string()),
                                    ("position", position_ticket.to_string()),
                                    ("order", order_ticket.to_string()),
                                    ("deal", deal_ticket.to_string()),
                                    ("price", format!("{fill_price:.8}")),
                                    ("volume", format!("{filled:.8}")),
                                    ("ts", now_ms().to_string()),
                                ]),
                                items: Vec::new(),
                            }),
                        }
                    }
                }
            }
            method::POS_MODIFY => {
                let ticket = param("ticket")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let sl: f64 = param("sl").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                let tp: f64 = param("tp").and_then(|v| v.parse().ok()).unwrap_or(0.0);
                let mut inner = self.inner.lock().await;
                let Some(position) = inner.positions.iter_mut().find(|p| p.ticket == ticket)
                else {
                    return Ok(EaResponse {
                        fields: map(&[
                            ("retcode", "10013".to_string()),
                            ("retcode_desc", "position not found".to_string()),
                        ]),
                        items: Vec::new(),
                    });
                };
                position.sl = sl;
                position.tp = tp;
                Ok(EaResponse {
                    fields: map(&[("retcode", "10009".to_string())]),
                    items: Vec::new(),
                })
            }
            method::POS_CLOSE => {
                let ticket = param("ticket")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let requested: f64 = param("volume")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0);
                let mut inner = self.inner.lock().await;
                let Some(index) = inner.positions.iter().position(|p| p.ticket == ticket) else {
                    return Ok(EaResponse {
                        fields: map(&[
                            ("status", "rejected".to_string()),
                            ("retcode", "10013".to_string()),
                            ("retcode_desc", "position not found".to_string()),
                        ]),
                        items: Vec::new(),
                    });
                };
                let position = inner.positions[index].clone();
                let volume = if requested > 0.0 {
                    requested.min(position.volume)
                } else {
                    position.volume
                };
                let close_price = if position.side == "buy" {
                    inner.symbols[&position.symbol].bid
                } else {
                    inner.symbols[&position.symbol].ask
                };
                let profit = if position.side == "buy" {
                    (close_price - position.price_open) * volume * 100.0
                } else {
                    (position.price_open - close_price) * volume * 100.0
                };
                let deal_ticket = inner.next_ticket;
                inner.next_ticket += 1;
                let remaining = position.volume - volume;
                if remaining <= 1e-9 {
                    inner.positions.remove(index);
                } else if let Some(p) = inner.positions.get_mut(index) {
                    p.volume = remaining;
                }
                inner.deals.push(Mt5Deal {
                    ticket: deal_ticket,
                    order_ticket: deal_ticket - 1,
                    position_ticket: position.ticket,
                    symbol: position.symbol.clone(),
                    side: if position.side == "buy" { "sell" } else { "buy" }.into(),
                    volume,
                    price: close_price,
                    profit,
                    swap: 0.0,
                    commission: 0.0,
                    comment: position.comment.clone(),
                    magic: position.magic,
                    time_ms: now_ms(),
                    entry: "out".into(),
                    reason: "client".into(),
                });
                Ok(EaResponse {
                    fields: map(&[
                        ("status", "filled".to_string()),
                        ("retcode", "10009".to_string()),
                        ("position", position.ticket.to_string()),
                        ("deal", deal_ticket.to_string()),
                        ("price", format!("{close_price:.8}")),
                        ("volume", format!("{volume:.8}")),
                    ]),
                    items: Vec::new(),
                })
            }
            method::CLOSE_ALL => {
                let magic = param("magic")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let symbol = param("symbol");
                let mut inner = self.inner.lock().await;
                let targets: Vec<Mt5Position> = inner
                    .positions
                    .iter()
                    .filter(|p| p.magic == magic)
                    .filter(|p| symbol.as_deref().map(|s| s == p.symbol).unwrap_or(true))
                    .cloned()
                    .collect();
                let mut items = Vec::new();
                for position in targets {
                    let close_price = if position.side == "buy" {
                        inner.symbols[&position.symbol].bid
                    } else {
                        inner.symbols[&position.symbol].ask
                    };
                    let deal_ticket = inner.next_ticket;
                    inner.next_ticket += 1;
                    inner.positions.retain(|p| p.ticket != position.ticket);
                    inner.deals.push(Mt5Deal {
                        ticket: deal_ticket,
                        order_ticket: deal_ticket - 1,
                        position_ticket: position.ticket,
                        symbol: position.symbol.clone(),
                        side: if position.side == "buy" { "sell" } else { "buy" }.into(),
                        volume: position.volume,
                        price: close_price,
                        profit: 0.0,
                        swap: 0.0,
                        commission: 0.0,
                        comment: position.comment.clone(),
                        magic: position.magic,
                        time_ms: now_ms(),
                        entry: "out".into(),
                        reason: "close_all".into(),
                    });
                    items.push(map(&[
                        ("position", position.ticket.to_string()),
                        ("ok", "1".to_string()),
                        ("volume", format!("{:.8}", position.volume)),
                        ("price", format!("{close_price:.8}")),
                    ]));
                }
                Ok(EaResponse {
                    fields: map(&[("count", items.len().to_string())]),
                    items,
                })
            }
            method::ORDER_CANCEL => Ok(EaResponse {
                fields: map(&[("retcode", "10009".to_string())]),
                items: Vec::new(),
            }),
            other => Err(TerminalError::Protocol(format!(
                "fake terminal does not implement {other}"
            ))),
        }
    }
}

impl TerminalTransport for FakeTerminal {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: &'a [(&'a str, String)],
        _timeout_ms: u64,
    ) -> BoxFut<'a, Result<EaResponse, TerminalError>> {
        Box::pin(self.handle(method, params))
    }

    fn describe(&self) -> String {
        "fake-terminal".to_string()
    }
}

/// Link status provider for SIM mode and tests.
pub struct FakeLink {
    status: Mutex<LinkStatusView>,
}

impl FakeLink {
    pub fn new(status: LinkStatusView) -> Arc<Self> {
        Arc::new(Self {
            status: Mutex::new(status),
        })
    }

    /// A connected, writable, demo link.
    pub fn connected_demo() -> Arc<Self> {
        Self::new(LinkStatusView {
            connected: true,
            write_enabled: true,
            mode: Some("demo".into()),
            login: Some(123_456),
            server: Some("Deriv-Demo".into()),
            ea_version: Some("1.0.0-fake".into()),
            build: Some(4755),
            last_heartbeat_ms: Some(now_ms()),
            heartbeat_age_ms: Some(0),
            last_error: None,
        })
    }

    pub async fn set_connected(&self, connected: bool) {
        self.status.lock().await.connected = connected;
    }

    pub async fn set_write_enabled(&self, write_enabled: bool) {
        self.status.lock().await.write_enabled = write_enabled;
    }

    pub async fn set_mode(&self, mode: &str) {
        self.status.lock().await.mode = Some(mode.to_string());
    }
}

impl LinkStatusProvider for FakeLink {
    fn status<'a>(&'a self) -> BoxFut<'a, LinkStatusView> {
        Box::pin(async move { self.status.lock().await.clone() })
    }
}
