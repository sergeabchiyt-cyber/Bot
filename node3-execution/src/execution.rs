use anyhow::{bail, Result};
use std::sync::Arc;
use tracing::{info, warn};
use crate::config::{Config, ExecutionVenue};
use crate::execution_chelsea::ChelseaExecution;
use crate::execution_deriv::DerivExecution;
use crate::execution_mt5::{Mt5BridgeLink, Mt5Execution};
use crate::types::{ProjectedOrder, TradeEvent};

pub struct ExecutionManager {
    pub config: Config,
    pub deriv: Option<DerivExecution>,
    pub chelsea: Option<ChelseaExecution>,
    /// Deriv MT5 **demo** executor (broker-confirmed fills only). Present when
    /// the MT5 venue is configured, so a halted venue still reports its state
    /// through the same object the strategy uses.
    pub mt5: Option<Mt5Execution>,
    pub atr: f64,
}

impl ExecutionManager {
    /// Build an execution manager. `shared_link` lets the caller pass the
    /// `Mt5BridgeLink` owned by `DiagnosticsHub`, so the venue the strategy uses
    /// and the link `/mt5/*` reports on are the same object. When it is `None`
    /// and the MT5 venue is configured, a private link is created (tests).
    pub fn new(config: Config) -> Self {
        Self::new_with_link(config, None)
    }

    pub fn new_with_link(config: Config, shared_link: Option<Arc<Mt5BridgeLink>>) -> Self {
        let venue = config.execution_venue();
        info!("Node 3 Execution Venue: {:?}", venue);
        if let Some(err) = config.venue_selection_error() {
            // Fail closed: a contradictory venue configuration must never be
            // resolved silently towards whichever credential happens to exist.
            warn!("Execution venue configuration error: {err}");
        }

        let deriv = match venue {
            ExecutionVenue::DerivDemo => Some(DerivExecution::new(&config)),
            _ => None,
        };
        let chelsea = match venue {
            ExecutionVenue::ChelseaLive => Some(ChelseaExecution::new(&config)),
            _ => None,
        };
        // The link exists whenever MT5 credentials are configured, regardless
        // of which venue is selected: `/mt5/*` and the kill switch must keep
        // working (and must be able to flatten) even if the strategy is not
        // currently trading MT5.
        let mt5 = if config.mt5_configured() {
            let link = shared_link.unwrap_or_else(|| Arc::new(Mt5BridgeLink::new(&config)));
            Some(Mt5Execution::new(&config, link))
        } else {
            None
        };

        Self {
            config,
            deriv,
            chelsea,
            mt5,
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

        // Fail closed on a contradictory configuration instead of falling
        // through to signal-only execution.
        if let Some(err) = self.config.venue_selection_error() {
            bail!("execution venue configuration error: {err}");
        }
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
            ExecutionVenue::DerivMt5Demo => {
                let mt5 = self
                    .mt5
                    .as_ref()
                    .expect("MT5 demo executor missing despite venue selection");
                // `size` is a status-quo parameter for the other venues; the MT5
                // venue sizes in **lots** from MT5_VOLUME_LOTS and reports the
                // broker's confirmed fill (never the requested price).
                mt5.place_order(side, self.config.mt5_volume_lots, entry, sl, tp, level_name)
                    .await?
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
