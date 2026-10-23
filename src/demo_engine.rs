//! Demo (paper) execution engine — 1:1000 leverage, 0.10 spread, 0.01–1.00 lots.
//!
//! This is a *simulator*, not an execution venue. It holds no credential, opens
//! no socket and has no order route: it fills the strategy's decisions against
//! its own last price, charges the spread on the way in and out, and tracks
//! balance, equity and margin so the desk can watch the strategy trade XAUUSD
//! without a broker. Node4 remains the only real execution path, and nothing
//! here can reach it.
//!
//! Model:
//! * mid price comes from the live SiftingIO candle stream (1 tick = 1 update);
//! * `bid = mid - spread/2`, `ask = mid + spread/2` (0.10 by default);
//! * a buy fills at the ask and exits at the bid, a sell fills at the bid and
//!   exits at the ask, so a round trip pays the full spread;
//! * contract size is 100 oz per lot, so 0.01 lots = 1 oz (=$1 per $1 move);
//! * margin = lots * 100 * price / 1000;
//! * the position closes at its stop, its target, on the opposite signal, or
//!   when the margin level falls under `stop_out_pct`.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::strategy::{Action, CONTRACT_SIZE, SYMBOL, StrategyDecision, round_lots};

const TRADE_HISTORY: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoParams {
    pub enabled: bool,
    pub start_balance: f64,
    /// 1000 = 1:1000. Margin is notional / leverage.
    pub leverage: f64,
    /// Full bid/ask spread in price units.
    pub spread: f64,
    pub min_lots: f64,
    pub max_lots: f64,
    pub lot_step: f64,
    pub stop_out_pct: f64,
    /// Seconds to wait after a *stop-driven* close (stop loss or stop out)
    /// before a new position may be opened. Without it a stop-out is followed
    /// by an identical re-entry at the same price on the next evaluation.
    pub cooldown_secs: i64,
}

impl Default for DemoParams {
    fn default() -> Self {
        Self {
            enabled: true,
            start_balance: 10_000.0,
            leverage: 1000.0,
            spread: 0.10,
            min_lots: 0.01,
            max_lots: 1.0,
            lot_step: 0.01,
            stop_out_pct: 50.0,
            cooldown_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DemoQuote {
    pub bid: f64,
    pub ask: f64,
    pub mid: f64,
    pub spread: f64,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoPosition {
    pub id: u64,
    pub side: Action,
    pub lots: f64,
    pub entry: f64,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub opened_at: i64,
    /// Cash at risk between entry and stop, fixed at open time.
    pub risk_usd: f64,
    pub margin: f64,
    pub unrealized_pnl: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoTrade {
    pub id: u64,
    pub side: Action,
    pub lots: f64,
    pub entry: f64,
    pub exit: f64,
    pub opened_at: i64,
    pub closed_at: i64,
    pub pnl: f64,
    pub r_multiple: f64,
    pub exit_reason: String,
}

/// What one `apply` call did, for logging and for the broadcaster.
#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    pub opened: Option<u64>,
    pub closed: Option<u64>,
    pub note: String,
}

impl ApplyReport {
    pub fn changed(&self) -> bool {
        self.opened.is_some() || self.closed.is_some()
    }
}

pub struct DemoEngine {
    pub params: DemoParams,
    balance: f64,
    quote: DemoQuote,
    position: Option<DemoPosition>,
    trades: VecDeque<DemoTrade>,
    next_id: u64,
    peak_equity: f64,
    max_drawdown_pct: f64,
    last_ts: i64,
    /// `last_ts`-style epoch ms before which no new position is opened.
    blocked_until: i64,
    note: String,
}

impl DemoEngine {
    pub fn new(params: DemoParams, start_price: f64, now_ms: i64) -> Self {
        let mut engine = Self {
            quote: DemoQuote {
                bid: 0.0,
                ask: 0.0,
                mid: 0.0,
                spread: params.spread,
                ts: now_ms,
            },
            balance: round2(params.start_balance.max(0.0)),
            params,
            position: None,
            trades: VecDeque::new(),
            next_id: 0,
            peak_equity: 0.0,
            max_drawdown_pct: 0.0,
            last_ts: now_ms,
            blocked_until: 0,
            note: "waiting for the first quote".to_string(),
        };
        engine.peak_equity = engine.balance;
        if start_price.is_finite() && start_price > 0.0 {
            let _ = engine.set_price(start_price, now_ms);
        }
        engine
    }

    pub fn quote(&self) -> DemoQuote {
        self.quote
    }

    /// Test-only accessors: the running engine is read through `snapshot()`
    /// (`/demo` and the `demo` WS topic), exactly like a client reads it.
    #[cfg(test)]
    pub fn balance(&self) -> f64 {
        self.balance
    }

    #[cfg(test)]
    pub fn position(&self) -> Option<&DemoPosition> {
        self.position.as_ref()
    }

    #[cfg(test)]
    pub fn trades(&self) -> Vec<DemoTrade> {
        self.trades.iter().rev().cloned().collect()
    }

    /// Mark the account to a new price and run the stop / target / stop-out
    /// checks. Returns the trade that was closed by this update, if any.
    pub fn set_price(&mut self, price: f64, ts: i64) -> Option<DemoTrade> {
        if !price.is_finite() || price <= 0.0 {
            return None;
        }
        self.quote = make_quote(price, self.params.spread, ts);
        self.last_ts = self.last_ts.max(ts);
        let closed = self.check_exits(ts);
        self.update_drawdown();
        closed
    }

    /// Feed one strategy decision to the simulator.
    pub fn apply(&mut self, decision: &StrategyDecision, now_ms: i64) -> ApplyReport {
        let mut report = ApplyReport::default();
        if !self.params.enabled {
            report.note = "demo engine disabled".into();
            return report;
        }
        if self.quote.mid <= 0.0 {
            report.note = "no quote yet".into();
            return report;
        }
        if decision.action == Action::Flat {
            report.note = "strategy is flat".into();
            return report;
        }

        let lots = round_lots(
            decision.lots,
            self.params.min_lots,
            self.params.max_lots,
            self.params.lot_step,
        );
        if lots <= 0.0 {
            report.note = "strategy size is below the minimum lot".into();
            return report;
        }

        let current_side = self.position.as_ref().map(|p| p.side);
        if current_side == Some(decision.action) {
            report.note = "same side already open — holding".into();
            return report;
        }
        if let Some(side) = current_side {
            let exit = if side == Action::Buy {
                self.quote.bid
            } else {
                self.quote.ask
            };
            if let Some(trade) = self.close_position(exit, now_ms, "signal_flip") {
                report.closed = Some(trade.id);
            }
        }

        // A stop-driven close starts a cooldown, so the engine cannot
        // immediately re-enter at the price that just took it out.
        if now_ms < self.blocked_until {
            report.note = format!(
                "cooldown: no new position for another {}s",
                (self.blocked_until - now_ms) / 1000
            );
            return report;
        }

        let side = decision.action;
        let entry = if side == Action::Buy {
            self.quote.ask
        } else {
            self.quote.bid
        };
        let margin = lots * CONTRACT_SIZE * entry / self.params.leverage.max(1.0);
        let free_margin = self.equity() - self.margin_used();
        if margin > free_margin {
            report.note = format!(
                "insufficient free margin: {:.2} needed, {:.2} free",
                margin, free_margin
            );
            return report;
        }

        self.next_id += 1;
        let id = self.next_id;
        let risk_usd = match decision.stop_loss {
            Some(stop) => round2((entry - stop).abs() * lots * CONTRACT_SIZE),
            None => 0.0,
        };
        let reason = decision
            .reasons
            .iter()
            .take(3)
            .map(|f| f.name.clone())
            .collect::<Vec<_>>()
            .join(" + ");
        let reason = if reason.is_empty() {
            format!("{} signal", side.as_str())
        } else {
            reason
        };

        self.position = Some(DemoPosition {
            id,
            side,
            lots,
            entry: round2(entry),
            stop_loss: decision.stop_loss.map(round2),
            take_profit: decision.take_profit.map(round2),
            opened_at: now_ms,
            risk_usd,
            margin: round2(margin),
            unrealized_pnl: 0.0,
            reason,
        });
        let unrealized = self.unrealized();
        if let Some(position) = self.position.as_mut() {
            position.unrealized_pnl = round2(unrealized);
        }
        self.note = format!(
            "opened {} {:.2} lot(s) at {:.2}",
            side.as_str(),
            lots,
            entry
        );
        report.opened = Some(id);
        report.note = self.note.clone();
        report
    }

    pub fn equity(&self) -> f64 {
        round2(self.balance + self.unrealized())
    }

    fn unrealized(&self) -> f64 {
        match self.position.as_ref() {
            Some(position) => {
                let exit = if position.side == Action::Buy {
                    self.quote.bid
                } else {
                    self.quote.ask
                };
                pnl_for(position.side, position.lots, position.entry, exit)
            }
            None => 0.0,
        }
    }

    fn margin_used(&self) -> f64 {
        match self.position.as_ref() {
            Some(position) => {
                position.lots * CONTRACT_SIZE * position.entry / self.params.leverage.max(1.0)
            }
            None => 0.0,
        }
    }

    fn margin_level(&self) -> f64 {
        let used = self.margin_used();
        if used > 0.0 {
            self.equity() / used * 100.0
        } else {
            0.0
        }
    }

    fn update_drawdown(&mut self) {
        let equity = self.equity();
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
        if self.peak_equity > 0.0 {
            let drawdown = (self.peak_equity - equity) / self.peak_equity * 100.0;
            if drawdown > self.max_drawdown_pct {
                self.max_drawdown_pct = drawdown;
            }
        }
    }

    fn check_exits(&mut self, ts: i64) -> Option<DemoTrade> {
        let Some(position) = self.position.as_ref() else {
            return None;
        };
        let side = position.side;
        let stop = position.stop_loss;
        let target = position.take_profit;
        let exit = if side == Action::Buy {
            self.quote.bid
        } else {
            self.quote.ask
        };

        if let Some(stop) = stop {
            let hit = if side == Action::Buy {
                exit <= stop
            } else {
                exit >= stop
            };
            if hit {
                // A gap through the stop fills at the market, not at the stop.
                let fill = if side == Action::Buy {
                    exit.min(stop)
                } else {
                    exit.max(stop)
                };
                return self.close_position(fill, ts, "stop_loss");
            }
        }
        if let Some(target) = target {
            let hit = if side == Action::Buy {
                exit >= target
            } else {
                exit <= target
            };
            if hit {
                return self.close_position(target, ts, "take_profit");
            }
        }
        // Margin call: equity has fallen below `stop_out_pct` of the margin.
        // The level can be negative after a gap, so there is no positivity
        // guard here — a position always consumes margin.
        let level = self.margin_level();
        if level < self.params.stop_out_pct {
            return self.close_position(exit, ts, "stop_out");
        }
        None
    }

    fn close_position(&mut self, price: f64, ts: i64, reason: &str) -> Option<DemoTrade> {
        let position = self.position.take()?;
        let exit = round2(price);
        let pnl = round2(pnl_for(position.side, position.lots, position.entry, exit));
        let r_multiple = if position.risk_usd > 0.0 {
            round2(pnl / position.risk_usd)
        } else {
            0.0
        };
        // Only a stop-driven exit imposes a cooldown: a signal flip is a new
        // decision and may reverse immediately, and a target is a clean exit.
        if reason == "stop_loss" || reason == "stop_out" {
            self.blocked_until = ts + self.params.cooldown_secs.max(0) * 1000;
        }
        self.balance = round2(self.balance + pnl);
        let trade = DemoTrade {
            id: position.id,
            side: position.side,
            lots: position.lots,
            entry: position.entry,
            exit,
            opened_at: position.opened_at,
            closed_at: ts,
            pnl,
            r_multiple,
            exit_reason: reason.to_string(),
        };
        self.note = format!(
            "closed {} {:.2} lot(s) at {:.2} ({:+.2} USD, {})",
            trade.side.as_str(),
            trade.lots,
            trade.exit,
            trade.pnl,
            trade.exit_reason
        );
        self.trades.push_back(trade.clone());
        while self.trades.len() > TRADE_HISTORY {
            self.trades.pop_front();
        }
        Some(trade)
    }

    pub fn stats(&self) -> serde_json::Value {
        let mut wins = 0usize;
        let mut losses = 0usize;
        let mut gross_profit = 0.0;
        let mut gross_loss = 0.0;
        let mut r_sum = 0.0;
        for trade in &self.trades {
            if trade.pnl >= 0.0 {
                wins += 1;
                gross_profit += trade.pnl;
            } else {
                losses += 1;
                gross_loss += -trade.pnl;
            }
            r_sum += trade.r_multiple;
        }
        let count = self.trades.len();
        let profit_factor = if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else if gross_profit > 0.0 {
            99.0
        } else {
            0.0
        };
        serde_json::json!({
            // Named `closed_trades` so it can never be confused with an order
            // topic: Node1 publishes market data, not fills.
            "closed_trades": count,
            "wins": wins,
            "losses": losses,
            "win_rate": if count > 0 { round2(wins as f64 / count as f64 * 100.0) } else { 0.0 },
            "gross_profit": round2(gross_profit),
            "gross_loss": round2(gross_loss),
            "net_pnl": round2(gross_profit - gross_loss),
            "profit_factor": round2(profit_factor),
            "avg_r": if count > 0 { round2(r_sum / count as f64) } else { 0.0 },
            "max_drawdown_pct": round2(self.max_drawdown_pct),
        })
    }

    /// `/demo` payload and the `demo` WS topic.
    pub fn snapshot(&self) -> serde_json::Value {
        let equity = self.equity();
        let margin = self.margin_used();
        let unrealized = self.unrealized();

        let position = match self.position.as_ref() {
            Some(position) => serde_json::json!({
                "id": position.id,
                "side": position.side,
                "lots": position.lots,
                "entry": position.entry,
                "stop_loss": position.stop_loss,
                "take_profit": position.take_profit,
                "opened_at": position.opened_at,
                "risk_usd": position.risk_usd,
                "margin": position.margin,
                "unrealized_pnl": round2(unrealized),
                "reason": position.reason,
            }),
            None => serde_json::Value::Null,
        };

        let recent: Vec<DemoTrade> = self.trades.iter().rev().take(50).cloned().collect();

        serde_json::json!({
            "enabled": self.params.enabled,
            "mode": "demo",
            "symbol": SYMBOL,
            "leverage": self.params.leverage,
            "quote": {
                "bid": self.quote.bid,
                "ask": self.quote.ask,
                "mid": self.quote.mid,
                "spread": self.quote.spread,
                "ts": self.quote.ts,
            },
            "account": {
                "currency": "USD",
                "balance": round2(self.balance),
                "equity": equity,
                "margin": round2(margin),
                "free_margin": round2(equity - margin),
                "margin_level": round2(self.margin_level()),
                "unrealized_pnl": round2(unrealized),
                "peak_equity": round2(self.peak_equity),
                "max_drawdown_pct": round2(self.max_drawdown_pct),
            },
            "params": {
                "start_balance": self.params.start_balance,
                "leverage": self.params.leverage,
                "spread": self.params.spread,
                "min_lots": self.params.min_lots,
                "max_lots": self.params.max_lots,
                "lot_step": self.params.lot_step,
                "stop_out_pct": self.params.stop_out_pct,
                "cooldown_secs": self.params.cooldown_secs,
            },
            "position": position,
            "stats": self.stats(),
            "recent_trades": recent,
            "note": self.note,
            "ts": self.last_ts,
        })
    }
}

fn make_quote(mid: f64, spread: f64, ts: i64) -> DemoQuote {
    let spread = spread.max(0.0);
    let half = spread / 2.0;
    DemoQuote {
        bid: round2(mid - half),
        ask: round2(mid + half),
        mid: round2(mid),
        spread: round2(spread),
        ts,
    }
}

fn pnl_for(side: Action, lots: f64, entry: f64, exit: f64) -> f64 {
    (exit - entry) * side.sign() * lots * CONTRACT_SIZE
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(
        action: Action,
        lots: f64,
        price: f64,
        stop: Option<f64>,
        target: Option<f64>,
    ) -> StrategyDecision {
        StrategyDecision {
            ts: 1_700_000_000_000,
            symbol: "XAUUSD".into(),
            action,
            score: 0.8,
            confidence: 0.8,
            price,
            atr: 2.0,
            regime: "trend_up".into(),
            volatility: "normal".into(),
            stop_loss: stop,
            take_profit: target,
            reward_risk: Some(2.0),
            lots,
            risk_usd: 0.0,
            reasons: Vec::new(),
            notes: Vec::new(),
            next_event_ts: None,
        }
    }

    fn engine() -> DemoEngine {
        DemoEngine::new(DemoParams::default(), 3400.0, 1_700_000_000_000)
    }

    #[test]
    fn a_buy_fills_at_the_ask_and_exits_at_the_bid() {
        let mut engine = engine();
        let quote = engine.quote();
        assert!((quote.ask - 3400.05).abs() < 1e-9, "ask = {}", quote.ask);
        assert!((quote.bid - 3399.95).abs() < 1e-9, "bid = {}", quote.bid);

        let report = engine.apply(
            &decision(Action::Buy, 0.10, 3400.0, Some(3390.0), Some(3420.0)),
            1_700_000_000_000,
        );
        assert!(report.opened.is_some(), "{}", report.note);
        let position = engine.position().unwrap();
        assert_eq!(position.side, Action::Buy);
        assert!((position.entry - 3400.05).abs() < 1e-9);

        // Unchanged mid: a round trip costs exactly the spread (10 oz * 0.10),
        // and neither the stop nor the target is anywhere near the price.
        assert!(
            engine.set_price(3400.0, 1_700_000_015_000).is_none(),
            "nothing should close at an unchanged mid"
        );
        assert!(
            (engine.equity() - 9999.0).abs() < 1e-6,
            "{}",
            engine.equity()
        );

        let report = engine.apply(
            &decision(Action::Sell, 0.10, 3400.0, Some(3410.0), Some(3380.0)),
            1_700_000_030_000,
        );
        assert!(report.closed.is_some());
        let trade = &engine.trades()[0];
        assert_eq!(trade.exit_reason, "signal_flip");
        assert!((trade.pnl + 1.0).abs() < 1e-6, "pnl = {}", trade.pnl);
        assert!((engine.balance() - 9999.0).abs() < 1e-6);
        // …and the flip left a short open at the bid.
        assert_eq!(engine.position().unwrap().side, Action::Sell);
    }

    #[test]
    fn margin_is_notional_over_one_thousand() {
        let mut engine = engine();
        engine.apply(
            &decision(Action::Buy, 1.0, 3400.0, Some(3390.0), Some(3420.0)),
            1_700_000_000_000,
        );
        let margin = engine.margin_used();
        // 1 lot * 100 oz * 3400.05 / 1000
        assert!((margin - 340.005).abs() < 1e-6, "margin = {margin}");
        // Entering at the ask costs the half spread immediately: 1 lot * 100
        // oz * 0.10 = $10 of floating loss against the bid.
        assert!(
            (engine.equity() - 9990.0).abs() < 1e-6,
            "equity = {}",
            engine.equity()
        );
    }

    #[test]
    fn lot_size_is_forced_onto_the_0_01_to_1_range() {
        let mut capped = engine();
        capped.apply(
            &decision(Action::Buy, 5.0, 3400.0, Some(3395.0), Some(3410.0)),
            1_700_000_000_000,
        );
        assert!((capped.position().unwrap().lots - 1.0).abs() < 1e-9);

        let mut floored = engine();
        floored.apply(
            &decision(Action::Buy, 0.001, 3400.0, Some(3395.0), Some(3410.0)),
            1_700_000_000_000,
        );
        assert!((floored.position().unwrap().lots - 0.01).abs() < 1e-9);
    }

    #[test]
    fn a_stop_closes_at_the_market_when_price_gaps_through_it() {
        let mut engine = engine();
        engine.apply(
            &decision(Action::Buy, 0.10, 3400.0, Some(3395.0), Some(3420.0)),
            1_700_000_000_000,
        );
        let closed = engine.set_price(3394.0, 1_700_000_015_000).expect("closed");
        assert_eq!(closed.exit_reason, "stop_loss");
        assert!(closed.exit <= 3395.0, "gap fill at {}", closed.exit);
        assert!(closed.pnl < 0.0);
        assert!(engine.position().is_none());
        assert!((engine.balance() - (10_000.0 + closed.pnl)).abs() < 1e-6);
    }

    #[test]
    fn a_target_pays_the_reward_risk() {
        let mut engine = engine();
        engine.apply(
            &decision(Action::Buy, 0.10, 3400.0, Some(3390.0), Some(3410.0)),
            1_700_000_000_000,
        );
        let closed = engine.set_price(3411.0, 1_700_000_015_000).expect("closed");
        assert_eq!(closed.exit_reason, "take_profit");
        assert!((closed.exit - 3410.0).abs() < 1e-9);
        // (3410.00 - 3400.05) * 10 oz
        assert!((closed.pnl - 99.5).abs() < 1e-6, "pnl = {}", closed.pnl);
        assert!(closed.r_multiple > 0.0);
    }

    #[test]
    fn a_flat_decision_never_opens_and_keeps_what_is_open() {
        let mut engine = engine();
        engine.apply(
            &decision(Action::Buy, 0.10, 3400.0, Some(3395.0), Some(3420.0)),
            1_700_000_000_000,
        );
        let report = engine.apply(
            &decision(Action::Flat, 0.0, 3400.0, None, None),
            1_700_000_015_000,
        );
        assert!(report.opened.is_none());
        assert!(engine.position().is_some());
    }

    #[test]
    fn a_stop_out_closes_a_position_the_account_cannot_carry() {
        let mut engine = engine();
        engine.apply(
            &decision(Action::Buy, 1.0, 3400.0, None, None),
            1_700_000_000_000,
        );
        let closed = engine.set_price(3200.0, 1_700_000_015_000).expect("closed");
        assert_eq!(closed.exit_reason, "stop_out");
        assert!(engine.position().is_none());
    }

    #[test]
    fn the_snapshot_carries_the_demo_configuration_the_frontend_reads() {
        let engine = engine();
        let snapshot = engine.snapshot();
        assert_eq!(snapshot["mode"], "demo");
        assert_eq!(snapshot["symbol"], "XAUUSD");
        assert_eq!(snapshot["leverage"], 1000.0);
        assert_eq!(snapshot["quote"]["spread"], 0.1);
        assert_eq!(snapshot["params"]["min_lots"], 0.01);
        assert_eq!(snapshot["params"]["max_lots"], 1.0);
        assert_eq!(snapshot["account"]["balance"], 10_000.0);
        assert_eq!(snapshot["position"], serde_json::Value::Null);
        assert_eq!(snapshot["stats"]["closed_trades"], 0);
    }
}
