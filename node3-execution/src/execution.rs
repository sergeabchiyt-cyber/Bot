use anyhow::Result;
use tracing::{info, warn};
use crate::config::{Config, ExecutionVenue};
use crate::execution_chelsea::ChelseaExecution;
use crate::execution_deriv::DerivExecution;
use crate::types::{ProjectedOrder, TradeEvent};

pub struct ExecutionManager {
    pub config: Config,
    pub deriv: Option<DerivExecution>,
    pub chelsea: Option<ChelseaExecution>,
    pub atr: f64,
}

impl ExecutionManager {
    pub fn new(config: Config) -> Self {
        let venue = config.execution_venue();
        info!("Node 3 Execution Venue: {:?}", venue);

        let deriv = match venue {
            ExecutionVenue::DerivDemo => Some(DerivExecution::new(&config)),
            _ => None,
        };
        let chelsea = match venue {
            ExecutionVenue::ChelseaLive => Some(ChelseaExecution::new(&config)),
            _ => None,
        };

        Self {
            config,
            deriv,
            chelsea,
            atr: 3.0,
        }
    }

    pub fn update_atr(&mut self, atr: f64) {
        if atr.is_finite() && atr > 0.0 {
            self.atr = atr;
        }
    }

    /// Compute `(sl, tp, sl_pips, tp_pips, rr)` for a given entry price and side.
    pub fn compute_sl_tp_details(&self, entry: f64, side: &str) -> (f64, f64, f64, f64, f64) {
        let safe_entry = if entry.is_finite() && entry > 0.0 {
            entry
        } else {
            2650.0
        };
        let atr_pct = (self.atr / safe_entry).clamp(0.001, 0.05);
        let sl_pips = self.config.sl_min_pips
            + (self.config.sl_max_pips - self.config.sl_min_pips) * (atr_pct * 20.0).min(1.0);
        let tp_pips = self.config.tp_min_pips
            + (self.config.tp_max_pips - self.config.tp_min_pips) * (atr_pct * 20.0).min(1.0);

        let sl_dist = sl_pips * 0.01;
        let mut tp_dist = tp_pips * 0.01;
        let raw_rr = tp_dist / sl_dist.max(0.0001);

        if raw_rr < self.config.rr_min {
            tp_dist = sl_dist * self.config.rr_min;
        } else if raw_rr > self.config.rr_max {
            tp_dist = sl_dist * self.config.rr_max;
        }

        let effective_tp_pips = tp_dist * 100.0;
        let effective_rr = tp_dist / sl_dist.max(0.0001);

        let (sl, tp) = match side {
            "buy" => (safe_entry - sl_dist, safe_entry + tp_dist),
            "sell" => (safe_entry + sl_dist, safe_entry - tp_dist),
            _ => (safe_entry - sl_dist, safe_entry + tp_dist),
        };

        (sl, tp, sl_pips, effective_tp_pips, effective_rr)
    }

    pub fn compute_sl_tp(&self, entry: f64, side: &str) -> (f64, f64) {
        let (sl, tp, _, _, _) = self.compute_sl_tp_details(entry, side);
        (sl, tp)
    }

    pub fn projected_order(&self, entry: f64, side: &str) -> ProjectedOrder {
        let (sl, tp, sl_pips, tp_pips, rr) = self.compute_sl_tp_details(entry, side);
        ProjectedOrder {
            side: side.to_string(),
            entry,
            sl,
            tp,
            sl_pips,
            tp_pips,
            rr,
            size: self.config.order_size,
        }
    }

    pub async fn execute(
        &self,
        side: &str,
        size: f64,
        entry: f64,
        level_name: &str,
    ) -> Result<TradeEvent> {
        let (sl, tp, _, _, rr) = self.compute_sl_tp_details(entry, side);
        let venue = self.config.execution_venue();

        info!(
            "Executing {} trade on level '{}' @ {:.3} (SL: {:.3}, TP: {:.3}, RR: {:.2}, Venue: {:?})",
            side, level_name, entry, sl, tp, rr, venue
        );

        let mut trade = match venue {
            ExecutionVenue::DerivDemo => {
                let deriv = self.deriv.as_ref().expect("Deriv demo executor missing");
                // `entry` is the reference the relative barrier is measured
                // from if Deriv ever rejects the barrier-less shape and asks
                // for a Higher/Lower style contract.
                let mut t = deriv.place_order(side, size, entry, sl, tp).await?;
                if t.entry <= 0.0 {
                    t.entry = entry;
                }
                t
            }
            ExecutionVenue::ChelseaLive => {
                let chelsea = self.chelsea.as_ref().expect("Chelsea live executor missing");
                let mut t = chelsea.place_order(side, size, sl, tp).await?;
                if t.entry <= 0.0 {
                    t.entry = entry;
                }
                t
            }
            ExecutionVenue::None => {
                warn!("No execution venue credentials provided — simulated signal only");
                TradeEvent {
                    trade_id: format!("sim-{}", chrono::Utc::now().timestamp_millis()),
                    symbol: "XAUUSD".into(),
                    side: side.into(),
                    size,
                    entry,
                    sl,
                    tp,
                    status: "signal_only".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    level_name: None,
                    venue: None,
                    rr: None,
                    current_price: None,
                    unrealized_pnl: None,
                    closed_at: None,
                }
            }
        };

        trade.level_name = Some(level_name.to_string());
        trade.venue = Some(format!("{:?}", venue));
        trade.rr = Some(rr);
        trade.current_price = Some(entry);
        trade.unrealized_pnl = Some(0.0);

        Ok(trade)
    }
}
