use anyhow::Result;
use tracing::{info, warn};
use crate::config::{Config, ExecutionVenue};
use crate::execution_chelsea::ChelseaExecution;
use crate::execution_deriv::DerivExecution;
use crate::types::TradeEvent;

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

    pub fn compute_sl_tp(&self, entry: f64, side: &str) -> (f64, f64) {
        let atr_pct = (self.atr / entry).clamp(0.001, 0.05);
        let sl_pips = self.config.sl_min_pips
            + (self.config.sl_max_pips - self.config.sl_min_pips) * (atr_pct * 20.0).min(1.0);
        let tp_pips = self.config.tp_min_pips
            + (self.config.tp_max_pips - self.config.tp_min_pips) * (atr_pct * 20.0).min(1.0);

        let sl_dist = sl_pips * 0.01;
        let mut tp_dist = tp_pips * 0.01;
        let rr = tp_dist / sl_dist;

        if rr < self.config.rr_min {
            tp_dist = sl_dist * self.config.rr_min;
        } else if rr > self.config.rr_max {
            tp_dist = sl_dist * self.config.rr_max;
        }

        match side {
            "buy" => (entry - sl_dist, entry + tp_dist),
            "sell" => (entry + sl_dist, entry - tp_dist),
            _ => (entry - sl_dist, entry + tp_dist),
        }
    }

    pub async fn execute(
        &self,
        side: &str,
        size: f64,
        entry: f64,
        level_name: &str,
    ) -> Result<TradeEvent> {
        let (sl, tp) = self.compute_sl_tp(entry, side);
        let venue = self.config.execution_venue();

        info!(
            "Executing {} trade on level '{}' @ {:.3} (SL: {:.3}, TP: {:.3}, Venue: {:?})",
            side, level_name, entry, sl, tp, venue
        );

        match venue {
            ExecutionVenue::DerivDemo => {
                let deriv = self.deriv.as_ref().expect("Deriv demo executor missing");
                deriv.place_order(side, size, sl, tp).await
            }
            ExecutionVenue::ChelseaLive => {
                let chelsea = self.chelsea.as_ref().expect("Chelsea live executor missing");
                chelsea.place_order(side, size, sl, tp).await
            }
            ExecutionVenue::None => {
                warn!("No execution venue credentials provided — simulated signal only");
                Ok(TradeEvent {
                    trade_id: format!("sim-{}", chrono::Utc::now().timestamp_millis()),
                    symbol: "XAUUSD".into(),
                    side: side.into(),
                    size,
                    entry,
                    sl,
                    tp,
                    status: "signal_only".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                })
            }
        }
    }
}
