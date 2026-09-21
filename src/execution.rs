use anyhow::Result;
use crate::config::{Config, ExecutionVenue};
use crate::execution_deriv::DerivExecution;
use crate::execution_chelsea::ChelseaExecution;
use crate::types::{TradeEvent, VpLevels};

pub struct ExecutionManager {
    pub config: Config,
    pub deriv: Option<DerivExecution>,
    pub chelsea: Option<ChelseaExecution>,
    pub atr: f64,
}

impl ExecutionManager {
    pub fn new(config: Config) -> Self {
        let venue = config.execution_venue();
        tracing::info!("Execution venue resolved to: {:?}", venue);

        let deriv = match venue {
            ExecutionVenue::DerivDemo => Some(DerivExecution::new(&config)),
            _ => None,
        };
        let chelsea = match venue {
            ExecutionVenue::ChelseaLive => Some(ChelseaExecution::new(&config)),
            _ => None,
        };

        Self { config, deriv, chelsea, atr: 3.0 }
    }

    pub fn compute_sl_tp(&self, entry: f64, side: &str) -> (f64, f64) {
        let atr_pct = (self.atr / entry).clamp(0.001, 0.05);
        let sl_pips = self.config.sl_min_pips
            + (self.config.sl_max_pips - self.config.sl_min_pips) * atr_pct.min(1.0);
        let tp_pips = self.config.tp_min_pips
            + (self.config.tp_max_pips - self.config.tp_min_pips) * atr_pct.min(1.0);

        let sl_dist = sl_pips * 0.01;
        let tp_dist = tp_pips * 0.01;
        let rr = tp_dist / sl_dist;

        let (final_sl, final_tp) = if rr < self.config.rr_min {
            (sl_dist, sl_dist * self.config.rr_min)
        } else if rr > self.config.rr_max {
            (sl_dist, sl_dist * self.config.rr_max)
        } else {
            (sl_dist, tp_dist)
        };

        match side {
            "buy" => (entry - final_sl, entry + final_tp),
            "sell" => (entry + final_sl, entry - final_tp),
            _ => (entry - final_sl, entry + final_tp),
        }
    }

    pub async fn execute(
        &self,
        side: &str,
        size: f64,
        entry: f64,
        _level: &VpLevels,
    ) -> Result<TradeEvent> {
        let (sl, tp) = self.compute_sl_tp(entry, side);

        match self.config.execution_venue() {
            ExecutionVenue::DerivDemo => {
                let deriv = self.deriv.as_ref().expect("deriv executor missing");
                deriv.place_order(side, size, sl, tp).await
            }
            ExecutionVenue::ChelseaLive => {
                let chelsea = self.chelsea.as_ref().expect("chelsea executor missing");
                chelsea.place_order(side, size, sl, tp).await
            }
            ExecutionVenue::None => {
                tracing::warn!("No execution venue configured — signal only: {} {} @ {}", side, size, entry);
                Ok(TradeEvent {
                    trade_id: format!("signal-{}", chrono::Utc::now().timestamp()),
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