//! Venue dispatch: one validated Node 3 intent in, at most one broker write,
//! a broker-authoritative outcome out.
//!
//! Node 4 owns sizing (Deriv stake, MT5 lots) and venue policy. It does not
//! own the strategy: the intent's stop loss and take profit are hard
//! constraints and are used verbatim — a rejection is allowed, a silent
//! re-derivation is not.

use std::sync::Arc;

use tracing::info;

use crate::config::{Config, ExecutionVenue};
use crate::execution_chelsea::ChelseaExecution;
use crate::execution_deriv::{DerivError, DerivExecution};
use crate::execution_mt5::{Mt5BridgeLink, Mt5Execution};
use crate::types::TradeIntent;

/// Broker-authoritative result of one intent.
#[derive(Debug, Clone)]
pub enum VenueOutcome {
    Filled {
        execution_id: String,
        price: Option<f64>,
        quantity: f64,
        quantity_unit: String,
    },
    Partial {
        execution_id: String,
        price: Option<f64>,
        quantity: f64,
        quantity_unit: String,
    },
    /// Refused before (or authoritatively by) the broker: no unknown write.
    Rejected {
        code: String,
        message: String,
    },
    /// A write may have reached the broker; reconcile, never resend.
    Unknown {
        message: String,
    },
}

pub struct ExecutionManager {
    pub config: Config,
    pub deriv: Option<DerivExecution>,
    pub chelsea: Option<ChelseaExecution>,
    /// Deriv MT5 **demo** executor (broker-confirmed fills only). Present
    /// whenever MT5 credentials are configured, so a halted venue still
    /// reports its state through the same object the resources watch.
    pub mt5: Option<Mt5Execution>,
}

impl ExecutionManager {
    /// Build an execution manager. `shared_link` lets the caller pass the
    /// `Mt5BridgeLink` owned by `DiagnosticsHub`, so the venue that places
    /// orders and the link `/mt5/*` reports on are the same object.
    pub fn new(config: Config, shared_link: Option<Arc<Mt5BridgeLink>>) -> Self {
        let venue = config.execution_venue();
        info!("Node 4 Execution Venue: {}", venue.label());
        if let Some(err) = config.venue_selection_error() {
            // Fail closed: a contradictory venue configuration must never be
            // resolved silently towards whichever credential happens to exist.
            tracing::warn!("Execution venue configuration error: {err}");
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
        // working (and must be able to flatten) even when the selected venue
        // is not MT5.
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
        }
    }

    pub fn mt5(&self) -> Option<&Mt5Execution> {
        self.mt5.as_ref()
    }

    /// Policy gate before the `accepted` report: venue configuration, link
    /// freshness, and the MT5 demo guards. No broker state changes here.
    pub async fn precheck(&self, _intent: &TradeIntent) -> Result<(), (String, String)> {
        if let Some(err) = self.config.venue_selection_error() {
            return Err((
                "venue_configuration".to_string(),
                format!("execution venue configuration error: {err}"),
            ));
        }
        match self.config.execution_venue() {
            ExecutionVenue::DerivMt5Demo => {
                let mt5 = self.mt5.as_ref().expect(
                    "MT5 executor missing despite venue selection",
                );
                mt5.precheck().await
            }
            ExecutionVenue::None | ExecutionVenue::DerivDemo | ExecutionVenue::ChelseaLive => {
                Ok(())
            }
        }
    }

    /// Execute one intent: at most one broker write, Node 3's SL/TP verbatim.
    pub async fn execute(&self, intent: &TradeIntent) -> VenueOutcome {
        let venue = self.config.execution_venue();
        info!(
            "executing intent {} ({} {} {} ref={:.2} SL={:.2} TP={:.2} venue={})",
            intent.intent_id,
            intent.side,
            intent.symbol,
            intent.order_type,
            intent.reference_price,
            intent.stop_loss,
            intent.take_profit,
            venue.label()
        );

        match venue {
            ExecutionVenue::None => {
                // Unreachable: the processor handles venue none as a dry run
                // (durable record + accepted, no broker order).
                VenueOutcome::Rejected {
                    code: "no_venue".into(),
                    message: "venue none — dry run only".into(),
                }
            }
            ExecutionVenue::DerivDemo => {
                let deriv = self
                    .deriv
                    .as_ref()
                    .expect("Deriv demo executor missing despite venue selection");
                // Node 4 owns the stake; the intent carries no size. The
                // intent's SL/TP are recorded (a Deriv Rise/Fall contract has
                // no stop leg — its expiry is the venue's risk shape).
                let stake = self.config.order_size;
                match deriv
                    .place_order(
                        &intent.side,
                        stake,
                        intent.reference_price,
                        intent.stop_loss,
                        intent.take_profit,
                    )
                    .await
                {
                    Ok(trade) => VenueOutcome::Filled {
                        execution_id: trade.trade_id,
                        price: Some(trade.entry),
                        quantity: trade.size,
                        quantity_unit: "stake".into(),
                    },
                    Err(DerivError::Rejected(message)) => VenueOutcome::Rejected {
                        code: "deriv_rejected".into(),
                        message,
                    },
                    Err(DerivError::Unknown(message)) => VenueOutcome::Unknown { message },
                }
            }
            ExecutionVenue::ChelseaLive => {
                let chelsea = self
                    .chelsea
                    .as_ref()
                    .expect("Chelsea executor missing despite venue selection");
                match chelsea
                    .place_order(&intent.side, self.config.order_size, intent.stop_loss, intent.take_profit)
                    .await
                {
                    Ok(trade) => VenueOutcome::Filled {
                        execution_id: trade.trade_id,
                        price: Some(intent.reference_price),
                        quantity: self.config.order_size,
                        quantity_unit: "units".into(),
                    },
                    Err(crate::execution_chelsea::ChelseaError::Rejected(message)) => {
                        VenueOutcome::Rejected {
                            code: "chelsea_rejected".into(),
                            message,
                        }
                    }
                    Err(crate::execution_chelsea::ChelseaError::Unknown(message)) => {
                        VenueOutcome::Unknown { message }
                    }
                }
            }
            ExecutionVenue::DerivMt5Demo => {
                let mt5 = self
                    .mt5
                    .as_ref()
                    .expect("MT5 demo executor missing despite venue selection");
                mt5.execute_intent(intent).await
            }
        }
    }

    /// Reconcile an unknown outcome with idempotent reads only. Returns
    /// `None` while the outcome is still unknown. For the MT5 venue this
    /// re-sends the same idempotency key, which the bridge resolves with a
    /// broker lookup (comment/order/deal/position) and never re-sends as a
    /// new order.
    pub async fn reconcile(&self, intent: &TradeIntent) -> Option<VenueOutcome> {
        if self.config.execution_venue() != ExecutionVenue::DerivMt5Demo {
            return None;
        }
        let mt5 = self
            .mt5
            .as_ref()
            .expect("MT5 demo executor missing despite venue selection");
        mt5.reconcile(intent).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::now_ms;

    fn intent(side: &str) -> TradeIntent {
        TradeIntent {
            schema_version: 1,
            intent_id: format!("n3-test-{side}-1"),
            strategy: "test".into(),
            symbol: "XAUUSD".into(),
            side: side.into(),
            order_type: "market".into(),
            reference_price: 2650.0,
            stop_loss: if side == "buy" { 2647.5 } else { 2652.5 },
            take_profit: if side == "buy" { 2656.0 } else { 2644.0 },
            risk_reward: 2.4,
            level_name: "PW PoC".into(),
            source_candle_time: 0,
            created_at: 1,
            expires_at: now_ms() + 120_000,
        }
    }

    #[tokio::test]
    async fn precheck_fails_closed_on_a_broken_venue_configuration() {
        let mut config = Config::default();
        config.deriv_demo_api = Some("token".into());
        config.mt5_bridge_token = Some("bridge-token".into());
        // Two venues configured, no override: venue_error must be set.
        let err = config.venue_decision().unwrap_err();
        config.venue = ExecutionVenue::None;
        config.venue_error = Some(err);

        let manager = ExecutionManager::new(config, None);
        let err = manager.precheck(&intent("buy")).await.unwrap_err();
        assert_eq!(err.0, "venue_configuration");
    }

    #[tokio::test]
    async fn precheck_passes_for_a_single_configured_venue() {
        let config = Config {
            deriv_demo_api: Some("token".into()),
            venue: ExecutionVenue::DerivDemo,
            ..Default::default()
        };
        let manager = ExecutionManager::new(config, None);
        assert!(manager.precheck(&intent("buy")).await.is_ok());
    }
}
