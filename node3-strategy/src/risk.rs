use crate::config::Config;
use crate::types::ProjectedOrder;

/// Strategy-side price projection only.
///
/// Node 3 computes the stop and target dictated by the strategy. It does not
/// choose a broker, account, stake, lot size, deviation, or order filling mode;
/// those account-aware responsibilities belong exclusively to Node 4.
pub struct RiskProjector {
    config: Config,
    atr: f64,
}

impl RiskProjector {
    pub fn new(config: Config) -> Self {
        Self { config, atr: 3.0 }
    }

    pub fn update_atr(&mut self, atr: f64) {
        if atr.is_finite() && atr > 0.0 {
            self.atr = atr;
        }
    }

    /// Compute `(stop_loss, take_profit, sl_pips, tp_pips, risk_reward)`.
    pub fn compute_details(&self, entry: f64, side: &str) -> (f64, f64, f64, f64, f64) {
        let safe_entry = if entry.is_finite() && entry > 0.0 {
            entry
        } else {
            2_650.0
        };
        let atr_pct = (self.atr / safe_entry).clamp(0.001, 0.05);
        let sl_pips = self.config.sl_min_pips
            + (self.config.sl_max_pips - self.config.sl_min_pips) * (atr_pct * 20.0).min(1.0);
        let requested_tp_pips = self.config.tp_min_pips
            + (self.config.tp_max_pips - self.config.tp_min_pips) * (atr_pct * 20.0).min(1.0);

        let sl_distance = sl_pips * 0.01;
        let mut tp_distance = requested_tp_pips * 0.01;
        let raw_rr = tp_distance / sl_distance.max(0.0001);
        if raw_rr < self.config.rr_min {
            tp_distance = sl_distance * self.config.rr_min;
        } else if raw_rr > self.config.rr_max {
            tp_distance = sl_distance * self.config.rr_max;
        }

        let effective_tp_pips = tp_distance * 100.0;
        let effective_rr = tp_distance / sl_distance.max(0.0001);
        let (stop_loss, take_profit) = match side {
            "sell" => (safe_entry + sl_distance, safe_entry - tp_distance),
            _ => (safe_entry - sl_distance, safe_entry + tp_distance),
        };

        (
            stop_loss,
            take_profit,
            sl_pips,
            effective_tp_pips,
            effective_rr,
        )
    }

    pub fn projected_order(&self, entry: f64, side: &str) -> ProjectedOrder {
        let (stop_loss, take_profit, sl_pips, tp_pips, risk_reward) =
            self.compute_details(entry, side);
        ProjectedOrder {
            side: side.to_string(),
            entry,
            stop_loss,
            take_profit,
            sl_pips,
            tp_pips,
            risk_reward,
            sizing: "node4_managed".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_and_sell_projections_have_correct_sides() {
        let projector = RiskProjector::new(Config::default());
        let buy = projector.projected_order(2_650.0, "buy");
        let sell = projector.projected_order(2_650.0, "sell");

        assert!(buy.stop_loss < buy.entry && buy.take_profit > buy.entry);
        assert!(sell.stop_loss > sell.entry && sell.take_profit < sell.entry);
        assert!((2.0..=3.0).contains(&buy.risk_reward));
        assert_eq!(buy.sizing, "node4_managed");
    }
}
