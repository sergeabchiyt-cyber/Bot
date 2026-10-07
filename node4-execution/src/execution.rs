//! Venue dispatch: exactly one venue, no fallback, no strategy math.
//!
//! Node 3 owns the strategy and therefore the stop-loss/take-profit prices. Node
//! 4 owns everything account-aware: which venue, how much stake or how many
//! lots, the deviation, the risk cap, and the symbol mapping. This module
//! routes one validated intent to exactly one of those venues and translates
//! the broker result into the execution report the protocol defines.

use std::sync::Arc;

use tracing::{info, warn};

use crate::config::{Config, ExecutionVenue};
use crate::execution_chelsea::ChelseaExecution;
use crate::execution_deriv::DerivExecution;
use crate::execution_mt5::{outcome_status, Mt5BridgeLink, Mt5Execution, VenueRefusal};
use crate::intent::{ExecutionStatus, ReconciliationOutcome, TradeIntent};
use crate::types::ExecutionTrade;

/// What a venue did with an intent.
#[derive(Debug, Clone)]
pub struct VenueOutcome {
    pub status: ExecutionStatus,
    /// Broker identifier (MT5 deal/position, Deriv contract id, ...).
    pub execution_id: Option<String>,
    pub filled_price: Option<f64>,
    pub quantity: Option<f64>,
    pub quantity_unit: Option<String>,
    pub order_ticket: Option<i64>,
    pub deal_ticket: Option<i64>,
    pub position_ticket: Option<i64>,
    pub retcode: Option<i64>,
    /// Non-secret detail for diagnostics/audit.
    pub detail: Option<String>,
}

impl VenueOutcome {
    pub fn dry_run() -> Self {
        Self {
            status: ExecutionStatus::Accepted,
            execution_id: None,
            filled_price: None,
            quantity: None,
            quantity_unit: None,
            order_ticket: None,
            deal_ticket: None,
            position_ticket: None,
            retcode: None,
            detail: Some("venue 'none': no broker order was placed".into()),
        }
    }

    pub fn unknown(detail: impl Into<String>) -> Self {
        Self {
            status: ExecutionStatus::Unknown,
            execution_id: None,
            filled_price: None,
            quantity: None,
            quantity_unit: None,
            order_ticket: None,
            deal_ticket: None,
            position_ticket: None,
            retcode: None,
            detail: Some(detail.into()),
        }
    }
}

/// Failure codes that mean "a write may have reached the broker".
///
/// These are reported as `unknown` and reconciled — never retried as a new
/// order. Everything else the venue refuses **before** the write is a
/// `rejected`.
pub fn is_unclear_write(code: &str) -> bool {
    matches!(
        code,
        "mt5_order_timeout" | "mt5_bridge_not_connected" | "mt5_order_failed"
    )
}

pub struct ExecutionManager {
    pub config: Config,
    /// Deriv options executor — present only when that venue is selected.
    pub deriv: Option<DerivExecution>,
    /// Chelsea MCP executor — present only when that venue is selected.
    pub chelsea: Option<ChelseaExecution>,
    /// Deriv MT5 **demo** executor (broker-confirmed fills only). Present when
    /// the MT5 venue is configured so a halted venue still reports its state
    /// through the same object the operator resources read.
    pub mt5: Option<Mt5Execution>,
}

impl ExecutionManager {
    /// Build the venue dispatcher. `link` is the `Mt5BridgeLink` owned by the
    /// `DiagnosticsHub`, so the venue that places orders and the `/mt5/*`
    /// resources the operator watches are the same session.
    pub fn new(config: &Config, link: Arc<Mt5BridgeLink>) -> Self {
        let venue = config.execution_venue();
        info!("Node 4 execution venue: {}", venue.label());
        if let Some(err) = config.venue_selection_error() {
            // Fail closed: a contradictory venue configuration must never be
            // resolved silently towards whichever credential happens to exist.
            warn!("Execution venue configuration error: {err}");
        }

        let deriv = match venue {
            ExecutionVenue::DerivDemo => Some(DerivExecution::new(config)),
            _ => None,
        };
        let chelsea = match venue {
            ExecutionVenue::ChelseaLive => Some(ChelseaExecution::new(config)),
            _ => None,
        };
        // The link exists whenever MT5 credentials are configured, regardless
        // of which venue is selected: `/mt5/*` and the kill switch must keep
        // working (and must be able to flatten) even if the strategy is not
        // currently trading MT5.
        let mt5 = if config.mt5_configured() {
            Some(Mt5Execution::new(config, link))
        } else {
            None
        };

        Self {
            config: config.clone(),
            deriv,
            chelsea,
            mt5,
        }
    }

    pub fn venue(&self) -> ExecutionVenue {
        self.config.execution_venue()
    }

    pub fn venue_label(&self) -> &'static str {
        self.config.execution_venue().label()
    }

    /// The quantity this venue will send for the next order: lots for MT5,
    /// USD stake for the options venues. It is recorded in the ledger *before*
    /// the write so an unclear outcome can still be audited exactly.
    pub fn planned_volume(&self) -> f64 {
        match self.venue() {
            ExecutionVenue::DerivMt5Demo => self.config.mt5_volume_lots,
            _ => self.config.execution_stake,
        }
    }

    /// Pre-write venue health: everything that must hold before Node 4 is
    /// allowed to write to a broker.
    pub async fn venue_health(&self) -> Result<(), VenueRefusal> {
        if let Some(err) = self.config.venue_selection_error() {
            return Err(VenueRefusal::new("venue_configuration_error", err));
        }
        match self.venue() {
            ExecutionVenue::None => Ok(()),
            ExecutionVenue::DerivMt5Demo => match self.mt5.as_ref() {
                Some(mt5) => mt5.venue_health().await,
                None => Err(VenueRefusal::new(
                    "mt5_not_configured",
                    "MT5 venue selected but no bridge link exists",
                )),
            },
            ExecutionVenue::DerivDemo => {
                if self.config.deriv_configured() {
                    Ok(())
                } else {
                    Err(VenueRefusal::new(
                        "deriv_not_configured",
                        "DERIV_DEMO_API is not set",
                    ))
                }
            }
            ExecutionVenue::ChelseaLive => {
                if self.config.chelsea_configured() {
                    Ok(())
                } else {
                    Err(VenueRefusal::new(
                        "chelsea_not_configured",
                        "MCP_CHELSEA_URL is not set",
                    ))
                }
            }
        }
    }

    /// Place at most one order for this intent.
    ///
    /// `Err` always means **nothing was sent**: it is safe to report `rejected`.
    /// An [`ExecutionStatus::Unknown`] outcome means the write may have landed
    /// and must be reconciled by `intent_id` instead of retried.
    pub async fn execute(
        &self,
        intent: &TradeIntent,
        broker_symbol: &str,
    ) -> Result<VenueOutcome, VenueRefusal> {
        self.venue_health().await?;

        match self.venue() {
            ExecutionVenue::None => Ok(VenueOutcome::dry_run()),
            ExecutionVenue::DerivMt5Demo => {
                let mt5 = self
                    .mt5
                    .as_ref()
                    .expect("MT5 executor missing despite venue selection");
                match mt5.execute(intent, broker_symbol).await {
                    Ok(outcome) => {
                        let status = outcome_status(&outcome);
                        let detail = outcome.error.clone().unwrap_or_else(|| {
                            format!(
                                "bridge status '{}' retcode {} ({})",
                                outcome.status, outcome.retcode, outcome.retcode_desc
                            )
                        });
                        Ok(VenueOutcome {
                            status,
                            execution_id: outcome
                                .position_ticket
                                .map(|ticket| format!("mt5-{ticket}"))
                                .or_else(|| {
                                    outcome
                                        .deal_ticket
                                        .map(|ticket| format!("mt5-deal-{ticket}"))
                                })
                                .or_else(|| Some(format!("mt5-{}", outcome.idempotency_key))),
                            filled_price: if outcome.is_confirmed_fill() {
                                outcome.price
                            } else {
                                None
                            },
                            quantity: if outcome.is_confirmed_fill() {
                                Some(outcome.filled_volume)
                            } else {
                                None
                            },
                            quantity_unit: if outcome.is_confirmed_fill() {
                                Some("lots".into())
                            } else {
                                None
                            },
                            order_ticket: outcome.order_ticket,
                            deal_ticket: outcome.deal_ticket,
                            position_ticket: outcome.position_ticket,
                            retcode: Some(outcome.retcode),
                            detail: Some(detail),
                        })
                    }
                    Err(refusal) if is_unclear_write(&refusal.code) => {
                        warn!(
                            "MT5 order for {} is unclear ({}): reporting unknown and reconciling",
                            intent.intent_id, refusal
                        );
                        Ok(VenueOutcome::unknown(refusal.message))
                    }
                    Err(refusal) => Err(refusal),
                }
            }
            ExecutionVenue::DerivDemo => {
                let deriv = self.deriv.as_ref().expect("Deriv demo executor missing");
                match deriv
                    .place_order(
                        &intent.side,
                        self.config.execution_stake,
                        intent.reference_price,
                        intent.stop_loss,
                        intent.take_profit,
                    )
                    .await
                {
                    Ok(trade) => Ok(VenueOutcome {
                        status: ExecutionStatus::Filled,
                        execution_id: Some(trade.trade_id.clone()),
                        filled_price: if trade.entry > 0.0 {
                            Some(trade.entry)
                        } else {
                            None
                        },
                        quantity: Some(trade.size),
                        quantity_unit: Some("stake_usd".into()),
                        order_ticket: None,
                        deal_ticket: None,
                        position_ticket: None,
                        retcode: None,
                        detail: Some("Deriv contract purchased".into()),
                    }),
                    Err(err) => Err(VenueRefusal::new(
                        "deriv_rejected",
                        format!("Deriv order failed: {err:#}"),
                    )),
                }
            }
            ExecutionVenue::ChelseaLive => {
                let chelsea = self
                    .chelsea
                    .as_ref()
                    .expect("Chelsea live executor missing");
                match chelsea
                    .place_order(
                        broker_symbol,
                        &intent.side,
                        self.config.execution_stake,
                        intent.stop_loss,
                        intent.take_profit,
                    )
                    .await
                {
                    Ok(trade) => Ok(VenueOutcome {
                        status: ExecutionStatus::Filled,
                        execution_id: Some(trade.trade_id.clone()),
                        filled_price: if trade.entry > 0.0 {
                            Some(trade.entry)
                        } else {
                            None
                        },
                        quantity: Some(trade.size),
                        quantity_unit: Some("stake_usd".into()),
                        order_ticket: None,
                        deal_ticket: None,
                        position_ticket: None,
                        retcode: None,
                        detail: Some("Chelsea MCP accepted the order".into()),
                    }),
                    Err(err) => Err(VenueRefusal::new(
                        "chelsea_rejected",
                        format!("Chelsea order failed: {err:#}"),
                    )),
                }
            }
        }
    }

    /// Try to resolve an unclear write using broker state only.
    pub async fn reconcile(&self, intent: &TradeIntent) -> Option<ReconciliationOutcome> {
        match self.venue() {
            ExecutionVenue::DerivMt5Demo => match self.mt5.as_ref() {
                Some(mt5) => mt5.link.reconcile_intent(&intent.intent_id).await,
                None => None,
            },
            _ => None,
        }
    }

    /// Turn a venue outcome into the open-trade record Node 4 tracks.
    pub fn trade_from_outcome(
        &self,
        intent: &TradeIntent,
        outcome: &VenueOutcome,
    ) -> ExecutionTrade {
        ExecutionTrade {
            trade_id: outcome
                .execution_id
                .clone()
                .unwrap_or_else(|| intent.intent_id.clone()),
            symbol: intent.symbol.clone(),
            side: intent.side.clone(),
            size: outcome.quantity.unwrap_or(0.0),
            entry: outcome.filled_price.unwrap_or(intent.reference_price),
            sl: intent.stop_loss,
            tp: intent.take_profit,
            status: "open".into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            intent_id: Some(intent.intent_id.clone()),
            level_name: if intent.level_name.is_empty() {
                None
            } else {
                Some(intent.level_name.clone())
            },
            venue: Some(self.venue_label().to_string()),
            rr: Some(intent.risk_reward),
            current_price: outcome.filled_price,
            unrealized_pnl: Some(0.0),
            closed_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn config() -> Config {
        Config {
            mt5_bridge_token: Some("bridge-token".into()),
            execution_venue_override: Some("deriv_mt5_demo".into()),
            venue: ExecutionVenue::DerivMt5Demo,
            ..Default::default()
        }
    }

    fn intent() -> TradeIntent {
        TradeIntent {
            schema_version: 1,
            intent_id: "n3-xauusd-1-buy-pw-poc".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: 2_650.0,
            stop_loss: 2_648.0,
            take_profit: 2_654.0,
            risk_reward: 2.0,
            level_name: "PW PoC".into(),
            source_candle_time: 1_700_000_000_000,
            created_at: 1_700_000_000_100,
            expires_at: 1_700_000_120_100,
        }
    }

    #[test]
    fn only_unclear_write_codes_become_unknown() {
        assert!(is_unclear_write("mt5_order_timeout"));
        assert!(is_unclear_write("mt5_bridge_not_connected"));
        assert!(is_unclear_write("mt5_order_failed"));
        // Everything the venue refuses before the write stays a rejection.
        assert!(!is_unclear_write("mt5_risk_limit"));
        assert!(!is_unclear_write("mt5_halted"));
        assert!(!is_unclear_write("mt5_not_demo"));
        assert!(!is_unclear_write("deriv_rejected"));
    }

    #[tokio::test]
    async fn the_none_venue_is_explicit_dry_run() {
        let mut cfg = config();
        cfg.mt5_bridge_token = None;
        cfg.venue = ExecutionVenue::None;
        cfg.execution_venue_override = Some("none".into());
        let link = Arc::new(Mt5BridgeLink::new(&cfg));
        let manager = ExecutionManager::new(&cfg, link);

        assert!(manager.venue_health().await.is_ok());
        let outcome = manager
            .execute(&intent(), "XAUUSD")
            .await
            .expect("dry run never refuses");
        assert_eq!(outcome.status, ExecutionStatus::Accepted);
        assert!(outcome.execution_id.is_none());
        assert!(manager.reconcile(&intent()).await.is_none());
    }

    #[tokio::test]
    async fn mt5_health_is_refused_without_a_bridge_session() {
        let cfg = config();
        let link = Arc::new(Mt5BridgeLink::new(&cfg));
        let manager = ExecutionManager::new(&cfg, link);
        let refusal = manager.venue_health().await.unwrap_err();
        assert_eq!(refusal.code, "mt5_bridge_not_connected");

        // A contradictory configuration is refused too.
        let mut contradictory = cfg.clone();
        contradictory.deriv_demo_api = Some("token".into());
        let link = Arc::new(Mt5BridgeLink::new(&contradictory));
        let manager = ExecutionManager::new(&contradictory, link);
        let refusal = manager.venue_health().await.unwrap_err();
        assert_eq!(refusal.code, "venue_configuration_error");
    }

    #[test]
    fn trade_records_carry_the_intent_and_node3_prices() {
        let cfg = config();
        let link = Arc::new(Mt5BridgeLink::new(&cfg));
        let manager = ExecutionManager::new(&cfg, link);
        let outcome = VenueOutcome {
            status: ExecutionStatus::Filled,
            execution_id: Some("mt5-42".into()),
            filled_price: Some(2_650.30),
            quantity: Some(0.01),
            quantity_unit: Some("lots".into()),
            order_ticket: Some(1),
            deal_ticket: Some(2),
            position_ticket: Some(42),
            retcode: Some(10_009),
            detail: None,
        };
        let trade = manager.trade_from_outcome(&intent(), &outcome);
        assert_eq!(trade.intent_id.as_deref(), Some("n3-xauusd-1-buy-pw-poc"));
        assert_eq!(trade.venue.as_deref(), Some("deriv_mt5_demo"));
        // The stop and target are Node 3's, untouched.
        assert_eq!(trade.sl, 2_648.0);
        assert_eq!(trade.tp, 2_654.0);
        assert_eq!(trade.size, 0.01);
    }
}
