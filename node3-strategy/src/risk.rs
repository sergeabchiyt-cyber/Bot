use crate::config::Config;
use crate::types::ProjectedOrder;

/// One pip is one cent on XAUUSD (decision D9): $2.00 of stop is 200 pips.
pub const PIP_SIZE: f64 = 0.01;
/// Price-unit ATR is converted to pips with the same scale.
pub const ATR_PERIOD: usize = 14;

/// Clamp bounds for the strategy-side price projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiskBounds {
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub sl_atr_mult: f64,
    pub tp_atr_mult: f64,
}

impl RiskBounds {
    pub fn from_config(config: &Config) -> Self {
        Self {
            sl_min_pips: config.sl_min_pips,
            sl_max_pips: config.sl_max_pips,
            tp_min_pips: config.tp_min_pips,
            tp_max_pips: config.tp_max_pips,
            rr_min: config.rr_min,
            rr_max: config.rr_max,
            sl_atr_mult: config.sl_atr_mult,
            tp_atr_mult: config.tp_atr_mult,
        }
    }
}

/// The ATR-derived distances for one evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    pub atr: f64,
    pub atr_pips: f64,
    pub sl_pips: f64,
    pub tp_pips: f64,
    pub rr: f64,
}

/// Compute SL and TP in pips from ATR. The stop is ATR times the stop multiplier,
/// clamped to the SL band. The target is ATR times the target multiplier,
/// clamped to both the TP band and the RR band around the stop.
/// Returns `None` when ATR is unusable or the bands cannot be satisfied.
pub fn project_pips(atr: f64, bounds: &RiskBounds) -> Option<Projection> {
    if !(atr.is_finite() && atr > 0.0) {
        return None;
    }
    let atr_pips = atr / PIP_SIZE;
    let sl_pips = (atr_pips * bounds.sl_atr_mult).clamp(bounds.sl_min_pips, bounds.sl_max_pips);
    let lower = bounds.tp_min_pips.max(sl_pips * bounds.rr_min);
    let upper = bounds.tp_max_pips.min(sl_pips * bounds.rr_max);
    if lower > upper {
        return None;
    }
    let tp_pips = (atr_pips * bounds.tp_atr_mult).clamp(lower, upper);
    Some(Projection {
        atr,
        atr_pips,
        sl_pips,
        tp_pips,
        rr: tp_pips / sl_pips,
    })
}

/// Strategy-side projection only.
///
/// Node 3 decides the stop and target that the strategy dictates. It does not
/// choose a venue, account, stake, lot size, deviation, or filling mode. Those
/// belong to Node 4.
pub struct RiskProjector {
    bounds: RiskBounds,
    atr: Option<f64>,
}

impl RiskProjector {
    pub fn new(config: &Config) -> Self {
        Self {
            bounds: RiskBounds::from_config(config),
            atr: None,
        }
    }

    /// Store the latest ATR. Unusable values clear it so no stale ATR is reused.
    pub fn update_atr(&mut self, atr: Option<f64>) {
        self.atr = atr.filter(|value| value.is_finite() && *value > 0.0);
    }

    pub fn projection(&self) -> Option<Projection> {
        project_pips(self.atr?, &self.bounds)
    }

    /// Stop and target around `entry` for `side` (`buy` or `sell`).
    pub fn order(&self, entry: f64, side: &str) -> Option<ProjectedOrder> {
        if !(entry.is_finite() && entry > 0.0) {
            return None;
        }
        let projection = self.projection()?;
        let sl_distance = projection.sl_pips * PIP_SIZE;
        let tp_distance = projection.tp_pips * PIP_SIZE;
        let (stop_loss, take_profit) = if side == "sell" {
            (entry + sl_distance, entry - tp_distance)
        } else {
            (entry - sl_distance, entry + tp_distance)
        };
        Some(ProjectedOrder {
            side: side.to_string(),
            entry,
            stop_loss,
            take_profit,
            sl_pips: projection.sl_pips,
            tp_pips: projection.tp_pips,
            risk_reward: projection.rr,
            sizing: "node4_managed".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> RiskBounds {
        RiskBounds::from_config(&Config::default())
    }

    #[test]
    fn projection_is_in_pips_with_one_pip_at_one_cent() {
        // ATR $2.00 is 200 pips. Stop = 200 pips, target clamped to 600 (RR 3).
        let p = project_pips(2.0, &bounds()).expect("projection");
        assert!((p.atr_pips - 200.0).abs() < 1e-9);
        assert!((p.sl_pips - 200.0).abs() < 1e-9);
        assert!((p.tp_pips - 600.0).abs() < 1e-9);
        assert!((p.rr - 3.0).abs() < 1e-9);
    }

    #[test]
    fn stop_is_clamped_into_the_two_to_three_dollar_band() {
        // ATR $1.00 is 100 pips: the stop floors at 200 pips ($2.00).
        assert_eq!(project_pips(1.0, &bounds()).expect("p").sl_pips, 200.0);
        // ATR $5.00 is 500 pips: the stop caps at 300 pips ($3.00).
        assert_eq!(project_pips(5.0, &bounds()).expect("p").sl_pips, 300.0);
    }

    #[test]
    fn rr_stays_between_two_and_three_across_the_atr_range() {
        for step in 1..=100 {
            let atr = step as f64 * 0.1;
            if let Some(p) = project_pips(atr, &bounds()) {
                assert!((2.0..=3.0 + 1e-9).contains(&p.rr), "atr {atr} rr {}", p.rr);
                assert!((200.0..=300.0).contains(&p.sl_pips));
                assert!((600.0..=800.0).contains(&p.tp_pips));
            }
        }
    }

    #[test]
    fn unusable_atr_yields_no_projection() {
        assert!(project_pips(0.0, &bounds()).is_none());
        assert!(project_pips(f64::NAN, &bounds()).is_none());
        let mut projector = RiskProjector::new(&Config::default());
        projector.update_atr(Some(-1.0));
        assert!(projector.projection().is_none());
        assert!(projector.order(2_650.0, "buy").is_none());
    }

    #[test]
    fn buy_and_sell_orders_sit_on_the_correct_sides() {
        let mut projector = RiskProjector::new(&Config::default());
        projector.update_atr(Some(2.5));
        let buy = projector.order(2_650.0, "buy").expect("buy");
        let sell = projector.order(2_650.0, "sell").expect("sell");
        assert!(buy.stop_loss < buy.entry && buy.take_profit > buy.entry);
        assert!(sell.stop_loss > sell.entry && sell.take_profit < sell.entry);
        assert!((2.0..=3.0).contains(&buy.risk_reward));
        assert_eq!(buy.sizing, "node4_managed");
        // Stop distance matches the projected pips.
        assert!(((buy.entry - buy.stop_loss) - buy.sl_pips * PIP_SIZE).abs() < 1e-9);
    }
}
