use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::config::Config;
use crate::types::{
    ActivityLogEntry, DerivAccountSnapshot, DerivOpenContract, DiagnosticsSnapshot,
    EngineWorkDiagnostics, OpenTradesSnapshot, ScanningSnapshot, TradeEvent, VpCandle, WsFrame,
};

const MAX_RECENT_EVENTS: usize = 50;
const MAX_RECENT_TRADES: usize = 50;
const CANDLE_BUFFER_CAPACITY: usize = 30;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct DiagnosticsInner {
    scanning: ScanningSnapshot,
    node3_open_trades: Vec<TradeEvent>,
    recent_trades: VecDeque<TradeEvent>,
    deriv_account: DerivAccountSnapshot,
    work: EngineWorkDiagnostics,
    recent_events: VecDeque<ActivityLogEntry>,
}

#[derive(Clone)]
pub struct DiagnosticsHub {
    inner: Arc<RwLock<DiagnosticsInner>>,
    tx: broadcast::Sender<WsFrame>,
    ws_clients: Arc<AtomicUsize>,
}

impl DiagnosticsHub {
    pub fn new(config: &Config) -> Self {
        let (tx, _) = broadcast::channel(256);
        let started_at = now_ms();
        let deriv_configured = config.deriv_demo_api.is_some();

        let scanning = ScanningSnapshot {
            active_setups_count: 0,
            total_levels_tracked: 0,
            proximity_dollars: 0.50,
            invalidation_dollars: 2.00,
            volume_threshold: config.volume_threshold,
            current_volume: None,
            volume_confirmed: false,
            last_price: None,
            atr: 3.0,
            atr_pips: 300.0,
            setups: Vec::new(),
            timestamp: started_at,
        };

        let deriv_account = DerivAccountSnapshot {
            configured: deriv_configured,
            connected: false,
            authorized: false,
            account_id: None,
            account_type: if deriv_configured {
                "demo".into()
            } else {
                "unconfigured".into()
            },
            balance: None,
            currency: "USD".into(),
            open_trades_count: 0,
            total_open_stake: 0.0,
            total_unrealized_pnl: 0.0,
            open_trades: Vec::new(),
            last_updated: None,
            error: if deriv_configured {
                None
            } else {
                Some("DERIV_DEMO_API not configured".into())
            },
        };

        let work = EngineWorkDiagnostics {
            service: "xauusd-node3-execution".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            venue: format!("{:?}", config.execution_venue()),
            started_at,
            uptime_secs: 0,
            node1_ws_url: config.node1_ws_url.clone(),
            node1_connected: false,
            node1_state: "starting".into(),
            node1_reconnect_count: 0,
            last_node1_msg_ts: None,
            last_candle_ts: None,
            last_levels_ts: None,
            candles_received: 0,
            levels_received: 0,
            ws_messages_received: 0,
            candle_buffer_len: 0,
            candle_buffer_capacity: CANDLE_BUFFER_CAPACITY,
            last_price: None,
            last_candle: None,
            atr: 3.0,
            atr_pips: 300.0,
            current_sl_pips: config.sl_min_pips,
            current_tp_pips: config.tp_min_pips,
            current_rr: (config.tp_min_pips / config.sl_min_pips.max(1.0))
                .clamp(config.rr_min, config.rr_max),
            volume_threshold: config.volume_threshold,
            last_candle_volume: None,
            volume_ratio: None,
            sl_min_pips: config.sl_min_pips,
            sl_max_pips: config.sl_max_pips,
            tp_min_pips: config.tp_min_pips,
            tp_max_pips: config.tp_max_pips,
            rr_min: config.rr_min,
            rr_max: config.rr_max,
            order_size: config.order_size,
            breaks_detected: 0,
            breaks_invalidated: 0,
            retests_rejected_low_volume: 0,
            signals_confirmed: 0,
            trades_executed: 0,
            trades_failed: 0,
            last_signal_ts: None,
            last_error: None,
            ws_clients_connected: 0,
        };

        let mut recent_events = VecDeque::with_capacity(MAX_RECENT_EVENTS);
        recent_events.push_front(ActivityLogEntry {
            timestamp: started_at,
            level: "info".into(),
            category: "system".into(),
            message: format!(
                "Node 3 started (venue: {:?}, vol threshold: {:.0})",
                config.execution_venue(),
                config.volume_threshold
            ),
        });

        Self {
            inner: Arc::new(RwLock::new(DiagnosticsInner {
                scanning,
                node3_open_trades: Vec::new(),
                recent_trades: VecDeque::with_capacity(MAX_RECENT_TRADES),
                deriv_account,
                work,
                recent_events,
            })),
            tx,
            ws_clients: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsFrame> {
        self.tx.subscribe()
    }

    pub fn client_connected(&self) -> usize {
        self.ws_clients.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn client_disconnected(&self) -> usize {
        let prev = self.ws_clients.fetch_sub(1, Ordering::SeqCst);
        prev.saturating_sub(1)
    }

    pub fn connected_clients(&self) -> usize {
        self.ws_clients.load(Ordering::SeqCst)
    }

    fn push_event_locked(
        inner: &mut DiagnosticsInner,
        level: &str,
        category: &str,
        message: String,
    ) -> ActivityLogEntry {
        let entry = ActivityLogEntry {
            timestamp: now_ms(),
            level: level.into(),
            category: category.into(),
            message,
        };
        inner.recent_events.push_front(entry.clone());
        while inner.recent_events.len() > MAX_RECENT_EVENTS {
            inner.recent_events.pop_back();
        }
        entry
    }

    fn build_open_trades_locked(inner: &DiagnosticsInner, ts: i64) -> OpenTradesSnapshot {
        let total_open_count =
            inner.node3_open_trades.len() + inner.deriv_account.open_trades.len();
        OpenTradesSnapshot {
            node3_open_trades: inner.node3_open_trades.clone(),
            deriv_open_trades: inner.deriv_account.open_trades.clone(),
            recent_trades: inner.recent_trades.iter().cloned().collect(),
            total_open_count,
            timestamp: ts,
        }
    }

    fn build_snapshot_locked(
        inner: &DiagnosticsInner,
        ws_clients: usize,
    ) -> DiagnosticsSnapshot {
        let ts = now_ms();
        let mut work = inner.work.clone();
        work.uptime_secs = ((ts - work.started_at).max(0) / 1000) as u64;
        work.ws_clients_connected = ws_clients;

        DiagnosticsSnapshot {
            timestamp: ts,
            scanning: inner.scanning.clone(),
            open_trades: Self::build_open_trades_locked(inner, ts),
            deriv_account: inner.deriv_account.clone(),
            work,
            recent_events: inner.recent_events.iter().cloned().collect(),
        }
    }

    pub async fn snapshot(&self) -> DiagnosticsSnapshot {
        let inner = self.inner.read().await;
        Self::build_snapshot_locked(&inner, self.connected_clients())
    }

    pub async fn scanning_snapshot(&self) -> ScanningSnapshot {
        let inner = self.inner.read().await;
        inner.scanning.clone()
    }

    pub async fn open_trades_snapshot(&self) -> OpenTradesSnapshot {
        let inner = self.inner.read().await;
        Self::build_open_trades_locked(&inner, now_ms())
    }

    pub async fn deriv_snapshot(&self) -> DerivAccountSnapshot {
        let inner = self.inner.read().await;
        inner.deriv_account.clone()
    }

    pub async fn push_event(&self, level: &str, category: &str, message: impl Into<String>) {
        let (entry, snap) = {
            let mut inner = self.inner.write().await;
            let entry = Self::push_event_locked(&mut inner, level, category, message.into());
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (entry, snap)
        };
        let _ = self.tx.send(WsFrame::DiagnosticEvent { data: entry });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn set_node1_state(&self, connected: bool, state: &str, increment_reconnect: bool) {
        let snap = {
            let mut inner = self.inner.write().await;
            inner.work.node1_connected = connected;
            inner.work.node1_state = state.into();
            if increment_reconnect {
                inner.work.node1_reconnect_count += 1;
            }
            Self::push_event_locked(
                &mut inner,
                if connected { "info" } else { "warn" },
                "node1",
                format!("Node 1 stream state: {state}"),
            );
            Self::build_snapshot_locked(&inner, self.connected_clients())
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_ws_message(&self) {
        let mut inner = self.inner.write().await;
        inner.work.ws_messages_received += 1;
        inner.work.last_node1_msg_ts = Some(now_ms());
    }

    pub async fn record_levels_update(
        &self,
        window: &str,
        poc: f64,
        vah: f64,
        val: f64,
        scanning: ScanningSnapshot,
    ) {
        let (scanning_clone, snap) = {
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.work.levels_received += 1;
            inner.work.last_levels_ts = Some(ts);
            inner.work.last_node1_msg_ts = Some(ts);
            inner.scanning = scanning.clone();
            Self::push_event_locked(
                &mut inner,
                "info",
                "levels",
                format!("Updated {window} levels (PoC: {poc:.2}, VaH: {vah:.2}, VaL: {val:.2})"),
            );
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (scanning, snap)
        };
        let _ = self.tx.send(WsFrame::Scanning {
            data: scanning_clone,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_candle_update(
        &self,
        candle: &VpCandle,
        atr: f64,
        sl_pips: f64,
        tp_pips: f64,
        rr: f64,
        candle_buffer_len: usize,
        breaks_delta: u64,
        invalidated_delta: u64,
        low_vol_delta: u64,
        signals_delta: u64,
        eval_events: Vec<(String, String)>,
        scanning: ScanningSnapshot,
    ) {
        let (scanning_clone, open_trades_snap, snap) = {
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.work.candles_received += 1;
            inner.work.last_candle_ts = Some(ts);
            inner.work.last_node1_msg_ts = Some(ts);
            inner.work.candle_buffer_len = candle_buffer_len;
            inner.work.last_price = Some(candle.close);
            inner.work.last_candle = Some(candle.clone());
            inner.work.atr = atr;
            inner.work.atr_pips = atr * 100.0;
            inner.work.current_sl_pips = sl_pips;
            inner.work.current_tp_pips = tp_pips;
            inner.work.current_rr = rr;
            inner.work.last_candle_volume = Some(candle.volume);
            if inner.work.volume_threshold > 0.0 {
                inner.work.volume_ratio = Some(candle.volume / inner.work.volume_threshold);
            }
            inner.work.breaks_detected += breaks_delta;
            inner.work.breaks_invalidated += invalidated_delta;
            inner.work.retests_rejected_low_volume += low_vol_delta;
            inner.work.signals_confirmed += signals_delta;
            inner.scanning = scanning.clone();

            for (lvl, msg) in eval_events {
                Self::push_event_locked(&mut inner, &lvl, "scanner", msg);
            }

            // Update Node 3 tracked open trades against the latest candle price and SL/TP levels.
            let mut closed_indices = Vec::new();
            for (idx, trade) in inner.node3_open_trades.iter_mut().enumerate() {
                trade.current_price = Some(candle.close);
                let price_diff = if trade.side == "buy" {
                    candle.close - trade.entry
                } else {
                    trade.entry - trade.close_ref_entry()
                };
                trade.unrealized_pnl = Some(price_diff * trade.size * 100.0);

                let sl_hit = if trade.side == "buy" {
                    trade.sl > 0.0 && candle.low <= trade.sl
                } else {
                    trade.sl > 0.0 && candle.high >= trade.sl
                };
                let tp_hit = if trade.side == "buy" {
                    trade.tp > 0.0 && candle.high >= trade.tp
                } else {
                    trade.tp > 0.0 && candle.low <= trade.tp
                };

                if sl_hit {
                    trade.status = "sl_hit".into();
                    trade.closed_at = Some(ts);
                    closed_indices.push(idx);
                } else if tp_hit {
                    trade.status = "tp_hit".into();
                    trade.closed_at = Some(ts);
                    closed_indices.push(idx);
                }
            }

            for idx in closed_indices.into_iter().rev() {
                let closed_trade = inner.node3_open_trades.remove(idx);
                let msg = format!(
                    "Trade {} ({} {}) closed via {} @ {:.2}",
                    closed_trade.trade_id,
                    closed_trade.side.to_uppercase(),
                    closed_trade.symbol,
                    closed_trade.status,
                    candle.close
                );
                // Also update matching entry in recent_trades if present
                if let Some(existing) = inner
                    .recent_trades
                    .iter_mut()
                    .find(|t| t.trade_id == closed_trade.trade_id)
                {
                    *existing = closed_trade;
                }
                Self::push_event_locked(&mut inner, "info", "execution", msg);
            }

            let open_trades_snap = Self::build_open_trades_locked(&inner, ts);
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (scanning, open_trades_snap, snap)
        };

        let _ = self.tx.send(WsFrame::Scanning {
            data: scanning_clone,
        });
        let _ = self.tx.send(WsFrame::OpenTrades {
            data: open_trades_snap,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_trade_opened(&self, trade: TradeEvent) {
        let (trade_clone, open_trades_snap, snap) = {
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.work.trades_executed += 1;
            inner.work.last_signal_ts = Some(ts);

            inner.node3_open_trades.push(trade.clone());
            inner.recent_trades.push_front(trade.clone());
            while inner.recent_trades.len() > MAX_RECENT_TRADES {
                inner.recent_trades.pop_back();
            }

            Self::push_event_locked(
                &mut inner,
                "signal",
                "execution",
                format!(
                    "Executed {} {} @ {:.2} (SL: {:.2}, TP: {:.2}, ID: {})",
                    trade.side.to_uppercase(),
                    trade.symbol,
                    trade.entry,
                    trade.sl,
                    trade.tp,
                    trade.trade_id
                ),
            );

            let open_trades_snap = Self::build_open_trades_locked(&inner, ts);
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (trade, open_trades_snap, snap)
        };

        let _ = self.tx.send(WsFrame::Trades { data: trade_clone });
        let _ = self.tx.send(WsFrame::OpenTrades {
            data: open_trades_snap,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn record_trade_failed(&self, err_msg: &str) {
        let snap = {
            let mut inner = self.inner.write().await;
            inner.work.trades_failed += 1;
            inner.work.last_error = Some(err_msg.to_string());
            Self::push_event_locked(
                &mut inner,
                "error",
                "execution",
                format!("Order execution failed: {err_msg}"),
            );
            Self::build_snapshot_locked(&inner, self.connected_clients())
        };
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn update_deriv_status(
        &self,
        connected: bool,
        authorized: bool,
        account_id: Option<String>,
        balance: Option<f64>,
        currency: Option<String>,
        error: Option<String>,
    ) {
        let (deriv_snap, snap) = {
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.deriv_account.connected = connected;
            inner.deriv_account.authorized = authorized;
            if let Some(acc) = account_id {
                inner.deriv_account.account_id = Some(acc);
            }
            if let Some(bal) = balance {
                inner.deriv_account.balance = Some(bal);
            }
            if let Some(curr) = currency {
                inner.deriv_account.currency = curr;
            }
            inner.deriv_account.error = error.clone();
            inner.deriv_account.last_updated = Some(ts);

            if let Some(err) = &error {
                inner.work.last_error = Some(err.clone());
                Self::push_event_locked(&mut inner, "warn", "deriv", format!("Deriv status: {err}"));
            } else if authorized {
                let auth_msg = format!(
                    "Deriv Demo authorized ({}, balance: {:.2} {})",
                    inner
                        .deriv_account
                        .account_id
                        .as_deref()
                        .unwrap_or("demo"),
                    inner.deriv_account.balance.unwrap_or(0.0),
                    inner.deriv_account.currency
                );
                Self::push_event_locked(&mut inner, "info", "deriv", auth_msg);
            }

            let deriv_snap = inner.deriv_account.clone();
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (deriv_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn update_deriv_balance(
        &self,
        balance: f64,
        currency: Option<String>,
        account_id: Option<String>,
    ) {
        let (deriv_snap, snap) = {
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.deriv_account.connected = true;
            inner.deriv_account.authorized = true;
            inner.deriv_account.balance = Some(balance);
            if let Some(curr) = currency {
                inner.deriv_account.currency = curr;
            }
            if let Some(acc) = account_id {
                inner.deriv_account.account_id = Some(acc);
            }
            inner.deriv_account.error = None;
            inner.deriv_account.last_updated = Some(ts);

            let deriv_snap = inner.deriv_account.clone();
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (deriv_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn update_deriv_portfolio(&self, contracts: Vec<DerivOpenContract>) {
        let (deriv_snap, open_trades_snap, snap) = {
            let mut inner = self.inner.write().await;
            let ts = now_ms();
            inner.deriv_account.open_trades = contracts;
            Self::recompute_deriv_totals(&mut inner.deriv_account);
            inner.deriv_account.last_updated = Some(ts);

            let deriv_snap = inner.deriv_account.clone();
            let open_trades_snap = Self::build_open_trades_locked(&inner, ts);
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (deriv_snap, open_trades_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::OpenTrades {
            data: open_trades_snap,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    pub async fn upsert_deriv_contract(&self, contract: DerivOpenContract, is_closed: bool) {
        let (deriv_snap, open_trades_snap, snap) = {
            let mut inner = self.inner.write().await;
            let ts = now_ms();

            if is_closed {
                inner
                    .deriv_account
                    .open_trades
                    .retain(|c| c.contract_id != contract.contract_id);

                if let Some(pos) = inner
                    .node3_open_trades
                    .iter()
                    .position(|t| t.trade_id == contract.contract_id)
                {
                    let mut closed = inner.node3_open_trades.remove(pos);
                    closed.status = contract.status.clone();
                    closed.current_price = contract.current_spot;
                    closed.unrealized_pnl = Some(contract.profit);
                    closed.closed_at = Some(ts);
                    if let Some(existing) = inner
                        .recent_trades
                        .iter_mut()
                        .find(|t| t.trade_id == closed.trade_id)
                    {
                        *existing = closed;
                    }
                }

                Self::push_event_locked(
                    &mut inner,
                    "info",
                    "deriv",
                    format!(
                        "Deriv contract #{} closed ({}, PnL: {:+.2} {})",
                        contract.contract_id,
                        contract.status,
                        contract.profit,
                        contract.currency
                    ),
                );
            } else {
                if let Some(existing) = inner
                    .deriv_account
                    .open_trades
                    .iter_mut()
                    .find(|c| c.contract_id == contract.contract_id)
                {
                    *existing = contract.clone();
                } else {
                    inner.deriv_account.open_trades.push(contract.clone());
                }

                // Sync with node3_open_trades if this contract was placed by Node 3
                if let Some(n3) = inner
                    .node3_open_trades
                    .iter_mut()
                    .find(|t| t.trade_id == contract.contract_id)
                {
                    if let Some(spot) = contract.current_spot {
                        n3.current_price = Some(spot);
                    }
                    n3.unrealized_pnl = Some(contract.profit);
                }
            }

            Self::recompute_deriv_totals(&mut inner.deriv_account);
            inner.deriv_account.last_updated = Some(ts);

            let deriv_snap = inner.deriv_account.clone();
            let open_trades_snap = Self::build_open_trades_locked(&inner, ts);
            let snap = Self::build_snapshot_locked(&inner, self.connected_clients());
            (deriv_snap, open_trades_snap, snap)
        };

        let _ = self.tx.send(WsFrame::DerivAccount { data: deriv_snap });
        let _ = self.tx.send(WsFrame::OpenTrades {
            data: open_trades_snap,
        });
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }

    fn recompute_deriv_totals(deriv: &mut DerivAccountSnapshot) {
        deriv.open_trades_count = deriv.open_trades.len();
        deriv.total_open_stake = deriv.open_trades.iter().map(|c| c.buy_price).sum();
        deriv.total_unrealized_pnl = deriv.open_trades.iter().map(|c| c.profit).sum();
    }

    pub async fn broadcast_tick(&self) {
        if self.connected_clients() == 0 {
            return;
        }
        let snap = self.snapshot().await;
        let _ = self.tx.send(WsFrame::Diagnostics { data: snap });
    }
}

trait TradeEntryExt {
    fn close_ref_entry(&self) -> f64;
}

impl TradeEntryExt for TradeEvent {
    fn close_ref_entry(&self) -> f64 {
        self.entry
    }
}
