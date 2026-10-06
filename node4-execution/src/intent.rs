//! Trade intent processing: the Node 3 → broker pipeline.
//!
//! Order of operations (protocol version 1):
//!
//! 1. Parse and validate the schema, ids, symbol, side, order type, finite
//!    prices, stop/target direction, and expiry. Invalid intents are
//!    reported `rejected` — never executed.
//! 2. Reject when `now >= expires_at`.
//! 3. **Durably** claim the `intent_id` in the execution ledger before any
//!    broker write. A repeated id replays the recorded result and never
//!    places a second order.
//! 4. Apply Node 4 venue/account/risk policy. A policy refusal is reported
//!    `rejected` (no unknown write).
//! 5. Send the `accepted` report — Node 4 has durably assumed responsibility
//!    for the id; a broker fill does not exist yet.
//! 6. Place at most one broker order. The Node 3 stop loss and take profit
//!    are hard constraints: they are used verbatim and never recomputed.
//! 7. Report the broker-authoritative outcome (`filled`, `partial`,
//!    `rejected`, `unknown`, …). `unknown` outcomes are reconciled by
//!    idempotent lookups — never re-sent as a new order — and the reconciled
//!    result is published back to Node 3.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::diagnostics::DiagnosticsHub;
use crate::execution::{ExecutionManager, VenueOutcome};
use crate::ledger::{Claim, ExecutionLedger, LedgerEvent};
use crate::node3_link::Node3Link;
use crate::types::{ExecutionReport, TradeIntent, report_status, now_ms};

/// A validation failure. `code` is machine-readable, `message` is safe to
/// show to operators (never contains secrets).
#[derive(Debug, Clone)]
pub struct IntentRejection {
    pub code: &'static str,
    pub message: String,
}

/// Validate an intent against the protocol and Node 4's symbol policy.
///
/// Node 4 owns sizing and venue policy but **not** the strategy: the stop
/// loss and take profit are checked for sanity (finite, correct side of the
/// reference price) and then used verbatim.
pub fn validate_intent(intent: &TradeIntent, config: &Config) -> Result<(), IntentRejection> {
    if intent.schema_version != crate::types::TRADE_INTENT_SCHEMA_VERSION {
        return Err(IntentRejection {
            code: "unsupported_schema",
            message: format!(
                "schema_version {} is not supported (expected {}); unknown versions fail closed",
                intent.schema_version,
                crate::types::TRADE_INTENT_SCHEMA_VERSION
            ),
        });
    }
    if intent.intent_id.trim().is_empty() {
        return Err(IntentRejection {
            code: "invalid_intent_id",
            message: "intent_id is empty; it is the idempotency key".into(),
        });
    }
    if !config.symbol_supported(&intent.symbol) {
        return Err(IntentRejection {
            code: "unsupported_symbol",
            message: format!(
                "symbol {} is not in the supported set [{}]; symbols are mapped explicitly",
                intent.symbol,
                config.supported_symbols.join(", ")
            ),
        });
    }
    if !matches!(intent.side.as_str(), "buy" | "sell") {
        return Err(IntentRejection {
            code: "invalid_side",
            message: format!("side '{}' must be exactly 'buy' or 'sell'", intent.side),
        });
    }
    if intent.order_type != "market" {
        return Err(IntentRejection {
            code: "unsupported_order_type",
            message: format!(
                "order_type '{}' is not supported; Node 4 executes market intents only",
                intent.order_type
            ),
        });
    }
    if !intent.reference_price.is_finite() || intent.reference_price <= 0.0 {
        return Err(IntentRejection {
            code: "invalid_price",
            message: format!(
                "reference_price {} is not a finite positive price",
                intent.reference_price
            ),
        });
    }
    if !intent.stop_loss.is_finite() || intent.stop_loss <= 0.0 {
        return Err(IntentRejection {
            code: "invalid_stop_loss",
            message: format!("stop_loss {} is not a finite positive price", intent.stop_loss),
        });
    }
    if !intent.take_profit.is_finite() || intent.take_profit <= 0.0 {
        return Err(IntentRejection {
            code: "invalid_take_profit",
            message: format!(
                "take_profit {} is not a finite positive price",
                intent.take_profit
            ),
        });
    }
    if !intent.risk_reward.is_finite() {
        return Err(IntentRejection {
            code: "invalid_risk_reward",
            message: format!("risk_reward {} is not finite", intent.risk_reward),
        });
    }
    match intent.side.as_str() {
        "buy" => {
            if !(intent.stop_loss < intent.reference_price) {
                return Err(IntentRejection {
                    code: "wrong_stop_side",
                    message: format!(
                        "buy stop_loss {} must be below reference_price {}",
                        intent.stop_loss, intent.reference_price
                    ),
                });
            }
            if !(intent.take_profit > intent.reference_price) {
                return Err(IntentRejection {
                    code: "wrong_target_side",
                    message: format!(
                        "buy take_profit {} must be above reference_price {}",
                        intent.take_profit, intent.reference_price
                    ),
                });
            }
        }
        "sell" => {
            if !(intent.stop_loss > intent.reference_price) {
                return Err(IntentRejection {
                    code: "wrong_stop_side",
                    message: format!(
                        "sell stop_loss {} must be above reference_price {}",
                        intent.stop_loss, intent.reference_price
                    ),
                });
            }
            if !(intent.take_profit < intent.reference_price) {
                return Err(IntentRejection {
                    code: "wrong_target_side",
                    message: format!(
                        "sell take_profit {} must be below reference_price {}",
                        intent.take_profit, intent.reference_price
                    ),
                });
            }
        }
        _ => unreachable!("side validated above"),
    }
    if now_ms() >= intent.expires_at {
        return Err(IntentRejection {
            code: "expired",
            message: format!(
                "intent expired at {} (now {})",
                intent.expires_at,
                now_ms()
            ),
        });
    }
    Ok(())
}

/// Build the replay report for an already-recorded intent id.
///
/// `received`/`dry_run` record states mean "durably ours, broker result
/// pending or venue none", which the protocol reports as `accepted`.
pub fn report_from_record(record: &crate::ledger::LedgerRecord) -> ExecutionReport {
    let status = match record.status.as_str() {
        "received" | "dry_run" => report_status::ACCEPTED.to_string(),
        other => other.to_string(),
    };
    let venue = if record.venue.is_empty() {
        "none".to_string()
    } else {
        record.venue.clone()
    };
    ExecutionReport {
        schema_version: crate::types::EXECUTION_REPORT_SCHEMA_VERSION,
        intent_id: record.intent_id.clone(),
        status,
        venue,
        symbol: if record.symbol.is_empty() {
            record.intent.symbol.clone()
        } else {
            record.symbol.clone()
        },
        side: if record.side.is_empty() {
            record.intent.side.clone()
        } else {
            record.side.clone()
        },
        timestamp: record.updated_at,
        execution_id: record.execution_id.clone(),
        filled_price: record.filled_price,
        quantity: record.quantity,
        quantity_unit: record.quantity_unit.clone(),
        error_code: record.error_code.clone(),
        error_message: record.error_message.clone(),
    }
}

struct Processor {
    config: Arc<Config>,
    hub: DiagnosticsHub,
    ledger: Arc<ExecutionLedger>,
    exec: Arc<ExecutionManager>,
    link: Node3Link,
}

/// Spawn the intent processor task, fed by the intent channel the Node 3
/// link receives intents on.
pub fn spawn(
    config: Arc<Config>,
    hub: DiagnosticsHub,
    ledger: Arc<ExecutionLedger>,
    exec: Arc<ExecutionManager>,
    link: Node3Link,
    intent_rx: mpsc::UnboundedReceiver<TradeIntent>,
) {
    tokio::spawn(process_loop(
        Processor {
            config,
            hub,
            ledger,
            exec,
            link,
        },
        intent_rx,
    ));
}

async fn process_loop(mut processor: Processor, mut intent_rx: mpsc::UnboundedReceiver<TradeIntent>) {
    while let Some(intent) = intent_rx.recv().await {
        processor.handle(intent).await;
    }
}

impl Processor {
    async fn handle(&self, intent: TradeIntent) {
        let intent_id = intent.intent_id.clone();
        self.hub.on_intent_received(&intent).await;
        info!(
            "trade_intent {} ({} {} {} ref={:.2} sl={:.2} tp={:.2} rr={:.2})",
            intent_id,
            intent.strategy,
            intent.side,
            intent.symbol,
            intent.reference_price,
            intent.stop_loss,
            intent.take_profit,
            intent.risk_reward
        );

        // 1–2. Validate (includes expiry).
        if let Err(rejection) = validate_intent(&intent, &self.config) {
            if rejection.code == "expired" {
                self.hub.on_intent_expired(&intent_id).await;
            }
            self.hub.on_intent_rejected(&intent_id, rejection.code).await;
            self.ledger_reject(&intent, rejection.code, &rejection.message)
                .await;
            self.report_rejected(&intent, rejection.code, &rejection.message)
                .await;
            warn!(
                "intent {} rejected: {} — {}",
                intent_id, rejection.code, rejection.message
            );
            return;
        }

        // 3. Durable claim before any broker write.
        match self.ledger.claim(&intent).await {
            Err(err) => {
                // Fail closed: a real venue must never trade without a
                // durable idempotency record.
                error!(
                    "intent {} could not be persisted: {err} — refusing (fail closed)",
                    intent_id
                );
                self.hub.on_intent_rejected(&intent_id, "ledger_unavailable").await;
                self.report_rejected(
                    &intent,
                    "ledger_unavailable",
                    &format!(
                        "the durable execution ledger is unavailable ({err}); the intent was \
                         not executed"
                    ),
                )
                .await;
                return;
            }
            Ok(Claim::Duplicate(record)) => {
                // 4'. Replayed id: return the recorded state, never execute
                // again.
                let report = report_from_record(&record);
                self.hub
                    .on_intent_duplicate(&intent_id, &report.status)
                    .await;
                self.ledger
                    .append(&LedgerEvent::ReportSent {
                        intent_id: &intent_id,
                        status: &report.status,
                    })
                    .await
                    .ok();
                info!(
                    "intent {} already recorded — replaying status {} (no second order)",
                    intent_id, report.status
                );
                self.send_report(report).await;
                return;
            }
            Ok(Claim::New) => {}
        }

        // 4. Node 4 policy: venue health, link freshness, demo guards.
        let venue = self.config.execution_venue();
        if let Err((code, message)) = self.exec.precheck(&intent).await {
            self.ledger_reject(&intent, code, &message).await;
            self.hub.on_intent_rejected(&intent_id, code).await;
            self.report_rejected(&intent, code, &message).await;
            warn!("intent {} refused by venue policy: {code} — {message}", intent_id);
            return;
        }

        // 5. Accepted: Node 4 has durably assumed responsibility for the id.
        let accepted = ExecutionReport::new(
            report_status::ACCEPTED,
            &intent,
            venue.label(),
        );
        self.ledger
            .set_status(
                &intent_id,
                "accepted",
                &LedgerEvent::IntentAccepted {
                    intent_id: &intent_id,
                    venue: venue.label(),
                },
                |record| record.venue = venue.label().to_string(),
            )
            .await
            .unwrap_or_else(|err| {
                error!("could not persist accepted state for {intent_id}: {err}")
            });
        self.hub.on_intent_accepted(&intent_id).await;
        self.send_report(accepted).await;

        // 6–7. Execute at most once; report the broker-authoritative result.
        match venue {
            crate::config::ExecutionVenue::None => {
                // Explicit dry-run/signal mode: durably recorded and
                // acknowledged, no broker order.
                self.ledger
                    .set_status(
                        &intent_id,
                        "dry_run",
                        &LedgerEvent::DryRun {
                            intent_id: &intent_id,
                            note: "venue none — dry run, no broker order",
                        },
                        |_| {},
                    )
                    .await
                    .ok();
                self.hub
                    .push_event(
                        "info",
                        "execution",
                        format!(
                            "intent {} recorded and acknowledged (dry run, venue none)",
                            intent_id
                        ),
                    )
                    .await;
                return;
            }
            _ => {}
        }

        // Fail-closed gate: no new broker write while the Node 3 session is
        // stale or disconnected (protocol rule). The intent is already
        // durably claimed and was reported `accepted`, so we record the
        // refusal and report it; Node 3 replays the id on a healthy session
        // and the idempotency ledger guarantees no duplicate order.
        if !self.link.accepts_broker_writes().await {
            let message =
                "Node 3 session is stale or disconnected — no new broker writes are made";
            let _ = self
                .ledger
                .set_status(
                    &intent_id,
                    "rejected",
                    &LedgerEvent::IntentRejected {
                        intent_id: &intent_id,
                        code: "node3_link_stale",
                        message,
                    },
                    |record| {
                        record.venue = venue.label().to_string();
                        record.error_code = Some("node3_link_stale".to_string());
                        record.error_message = Some(message.to_string());
                    },
                )
                .await;
            self.hub
                .on_intent_rejected(&intent_id, "node3_link_stale")
                .await;
            self.report_rejected(&intent, "node3_link_stale", message).await;
            warn!("intent {} not sent to broker: {message}", intent_id);
            return;
        }

        self.ledger
            .append(&LedgerEvent::BrokerCommand {
                intent_id: &intent_id,
                venue: venue.label(),
                command: "order_send",
                volume: match venue {
                    crate::config::ExecutionVenue::DerivMt5Demo => {
                        Some(self.config.mt5_volume_lots)
                    }
                    _ => Some(self.config.order_size),
                },
                sl: Some(intent.stop_loss),
                tp: Some(intent.take_profit),
            })
            .await
            .ok();

        let outcome = self.exec.execute(&intent).await;
        self.finish(&intent, venue.label(), outcome).await;
    }

    /// Map a venue outcome to ledger + report + trade tape, and start
    /// reconciliation for unknowns.
    async fn finish(&self, intent: &TradeIntent, venue: &str, outcome: VenueOutcome) {
        let intent_id = &intent.intent_id;
        match outcome {
            VenueOutcome::Filled {
                execution_id,
                price,
                quantity,
                quantity_unit,
            } => {
                let mut report =
                    ExecutionReport::new(report_status::FILLED, intent, venue);
                report.execution_id = Some(execution_id.clone());
                report.filled_price = price;
                report.quantity = Some(quantity);
                report.quantity_unit = Some(quantity_unit.clone());
                self.ledger
                    .set_status(
                        intent_id,
                        "filled",
                        &LedgerEvent::BrokerOutcome {
                            intent_id,
                            venue,
                            symbol: &intent.symbol,
                            side: &intent.side,
                            status: "filled",
                            execution_id: Some(&execution_id),
                            filled_price: price,
                            quantity: Some(quantity),
                            quantity_unit: Some(&quantity_unit),
                            retcode: None,
                            reconciled: false,
                        },
                        |record| {
                            record.execution_id = Some(execution_id.clone());
                            record.filled_price = price;
                            record.quantity = Some(quantity);
                            record.quantity_unit = Some(quantity_unit.clone());
                        },
                    )
                    .await
                    .ok();
                self.record_trade_opened(intent, &execution_id, price, quantity)
                    .await;
                self.hub
                    .on_trade_executed(intent_id, report_status::FILLED)
                    .await;
                self.send_report(report).await;
                info!("intent {} filled (execution {execution_id})", intent_id);
            }
            VenueOutcome::Partial {
                execution_id,
                price,
                quantity,
                quantity_unit,
            } => {
                let mut report = ExecutionReport::new(report_status::PARTIAL, intent, venue);
                report.execution_id = Some(execution_id.clone());
                report.filled_price = price;
                report.quantity = Some(quantity);
                report.quantity_unit = Some(quantity_unit.clone());
                self.ledger
                    .set_status(
                        intent_id,
                        "partial",
                        &LedgerEvent::BrokerOutcome {
                            intent_id,
                            venue,
                            symbol: &intent.symbol,
                            side: &intent.side,
                            status: "partial",
                            execution_id: Some(&execution_id),
                            filled_price: price,
                            quantity: Some(quantity),
                            quantity_unit: Some(&quantity_unit),
                            retcode: None,
                            reconciled: false,
                        },
                        |record| {
                            record.execution_id = Some(execution_id.clone());
                            record.filled_price = price;
                            record.quantity = Some(quantity);
                            record.quantity_unit = Some(quantity_unit.clone());
                        },
                    )
                    .await
                    .ok();
                self.record_trade_opened(intent, &execution_id, price, quantity)
                    .await;
                self.hub
                    .on_trade_executed(intent_id, report_status::PARTIAL)
                    .await;
                self.send_report(report).await;
            }
            VenueOutcome::Rejected { code, message } => {
                self.ledger
                    .set_status(
                        intent_id,
                        "rejected",
                        &LedgerEvent::IntentRejected {
                            intent_id,
                            code: &code,
                            message: &message,
                        },
                        |record| {
                            record.error_code = Some(code.clone());
                            record.error_message = Some(message.clone());
                        },
                    )
                    .await
                    .ok();
                self.hub.on_trade_failed(intent_id, &message).await;
                self.send_report(
                    ExecutionReport::new(report_status::REJECTED, intent, venue)
                        .with_error(code, message),
                )
                .await;
                warn!("intent {} rejected by venue: {code} — {message}", intent_id);
            }
            VenueOutcome::Unknown { message } => {
                self.ledger
                    .set_status(
                        intent_id,
                        "unknown",
                        &LedgerEvent::BrokerOutcome {
                            intent_id,
                            venue,
                            symbol: &intent.symbol,
                            side: &intent.side,
                            status: "unknown",
                            execution_id: None,
                            filled_price: None,
                            quantity: None,
                            quantity_unit: None,
                            retcode: None,
                            reconciled: false,
                        },
                        |record| record.error_message = Some(message.clone()),
                    )
                    .await
                    .ok();
                self.hub.on_trade_failed(intent_id, &message).await;
                self.send_report(
                    ExecutionReport::new(report_status::UNKNOWN, intent, venue).with_error(
                        "unknown_outcome",
                        format!(
                            "a broker write may have reached the broker; it will be reconciled \
                             by idempotent lookup and never re-sent as a new order: {message}"
                        ),
                    ),
                )
                .await;
                warn!("intent {} outcome unknown — reconciling: {message}", intent_id);
                self.spawn_reconciliation(intent).await;
            }
        }
    }

    /// Reconcile an unknown MT5 outcome: re-send the *same* idempotency key.
    /// The bridge resolves it with an idempotent broker lookup (find by
    /// comment/order/deal/position) and never places a second order. The
    /// reconciled result is published back to Node 3 as a follow-up report.
    async fn spawn_reconciliation(&self, intent: &TradeIntent) {
        let Some(mt5) = self.exec.mt5() else {
            // Deriv/Chelsea unknowns are operator-reconciled via the account
            // and open-trade views; there is no idempotent broker lookup.
            return;
        };
        let _ = mt5;
        let intent = intent.clone();
        let processor = Processor {
            config: self.config.clone(),
            hub: self.hub.clone(),
            ledger: self.ledger.clone(),
            exec: self.exec.clone(),
            link: self.link.clone(),
        };
        tokio::spawn(async move {
            let attempts = 8;
            let mut delay = Duration::from_secs(5);
            for _ in 0..attempts {
                sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(30));
                let Some(outcome) = processor.exec.reconcile(&intent).await else {
                    continue; // still unknown
                };
                let status = match &outcome {
                    VenueOutcome::Filled { .. } => "filled",
                    VenueOutcome::Partial { .. } => "partial",
                    VenueOutcome::Rejected { .. } => "rejected",
                    VenueOutcome::Unknown { .. } => "unknown",
                };
                processor
                    .ledger
                    .set_status(
                        &intent.intent_id,
                        status,
                        &LedgerEvent::Reconciled {
                            intent_id: &intent.intent_id,
                            venue: "deriv_mt5_demo",
                            status,
                            execution_id: match &outcome {
                                VenueOutcome::Filled { execution_id, .. }
                                | VenueOutcome::Partial { execution_id, .. } => {
                                    Some(execution_id)
                                }
                                _ => None,
                            },
                        },
                        |record| match &outcome {
                            VenueOutcome::Filled {
                                execution_id,
                                price,
                                quantity,
                                ..
                            }
                            | VenueOutcome::Partial {
                                execution_id,
                                price,
                                quantity,
                                ..
                            } => {
                                record.execution_id = Some(execution_id.clone());
                                record.filled_price = *price;
                                record.quantity = Some(*quantity);
                            }
                            _ => {}
                        },
                    )
                    .await
                    .ok();
                let report = match outcome {
                    VenueOutcome::Filled {
                        execution_id,
                        price,
                        quantity,
                        quantity_unit,
                    } => {
                        let mut r =
                            ExecutionReport::new(report_status::FILLED, &intent, "deriv_mt5_demo");
                        r.execution_id = Some(execution_id);
                        r.filled_price = price;
                        r.quantity = Some(quantity);
                        r.quantity_unit = Some(quantity_unit);
                        r
                    }
                    VenueOutcome::Partial {
                        execution_id,
                        price,
                        quantity,
                        quantity_unit,
                    } => {
                        let mut r =
                            ExecutionReport::new(report_status::PARTIAL, &intent, "deriv_mt5_demo");
                        r.execution_id = Some(execution_id);
                        r.filled_price = price;
                        r.quantity = Some(quantity);
                        r.quantity_unit = Some(quantity_unit);
                        r
                    }
                    VenueOutcome::Rejected { code, message } => ExecutionReport::new(
                        report_status::REJECTED,
                        &intent,
                        "deriv_mt5_demo",
                    )
                    .with_error(code, message),
                    VenueOutcome::Unknown { message } => {
                        ExecutionReport::new(report_status::UNKNOWN, &intent, "deriv_mt5_demo")
                            .with_error("still_unknown", message)
                    }
                };
                info!(
                    "intent {} reconciled as {}",
                    intent.intent_id, report.status
                );
                processor.hub.push_event(
                    "info",
                    "reconciliation",
                    format!(
                        "intent {} reconciled: {}",
                        intent.intent_id, report.status
                    ),
                ).await;
                processor.send_report(report).await;
                return;
            }
            warn!(
                "intent {} still unknown after reconciliation attempts — operator action \
                 required (check /mt5/positions and the bridge history)",
                intent.intent_id
            );
        });
    }

    /// Record a pre-execution rejection in the ledger (best effort: the
    /// intent was never a broker write, so a ledger blip does not block the
    /// report).
    async fn ledger_reject(&self, intent: &TradeIntent, code: &str, message: &str) {
        if intent.intent_id.trim().is_empty() {
            return;
        }
        // Claim first so a later resend of the same id replays this rejection
        // instead of being re-validated (and possibly re-executed if Node 3
        // fixes only its own state).
        let claim = match self.ledger.claim(intent).await {
            Ok(claim) => claim,
            Err(err) => {
                error!(
                    "could not persist rejection for intent {}: {err}",
                    intent.intent_id
                );
                return;
            }
        };
        if matches!(claim, Claim::Duplicate(_)) {
            return;
        }
        let _ = self
            .ledger
            .set_status(
                &intent.intent_id,
                "rejected",
                &LedgerEvent::IntentRejected {
                    intent_id: &intent.intent_id,
                    code,
                    message,
                },
                |record| {
                    record.venue = self.config.execution_venue().label().to_string();
                    record.error_code = Some(code.to_string());
                    record.error_message = Some(message.to_string());
                },
            )
            .await;
    }

    async fn report_rejected(&self, intent: &TradeIntent, code: &str, message: &str) {
        if intent.intent_id.trim().is_empty() {
            // No id to report on: Node 3 cannot attribute it. Diagnostics
            // only (the event above already logged it).
            return;
        }
        let _ = self
            .ledger
            .append(&LedgerEvent::ReportSent {
                intent_id: &intent.intent_id,
                status: report_status::REJECTED,
            })
            .await;
        self.send_report(
            ExecutionReport::new(report_status::REJECTED, intent, self.config.execution_venue().label())
                .with_error(code, message),
        )
        .await;
    }

    async fn record_trade_opened(
        &self,
        intent: &TradeIntent,
        execution_id: &str,
        price: Option<f64>,
        quantity: f64,
    ) {
        let trade = crate::types::TradeEvent {
            trade_id: execution_id.to_string(),
            symbol: intent.symbol.clone(),
            side: intent.side.clone(),
            size: quantity,
            entry: price.unwrap_or(intent.reference_price),
            sl: intent.stop_loss,
            tp: intent.take_profit,
            status: "open".into(),
            timestamp: now_ms(),
            level_name: if intent.level_name.is_empty() {
                None
            } else {
                Some(intent.level_name.clone())
            },
            venue: Some(self.config.execution_venue().label().to_string()),
            rr: Some(intent.risk_reward),
            current_price: price,
            unrealized_pnl: Some(0.0),
            closed_at: None,
            intent_id: Some(intent.intent_id.clone()),
        };
        self.hub.record_trade_opened(trade).await;
    }

    async fn send_report(&self, report: ExecutionReport) {
        let _ = self
            .ledger
            .append(&LedgerEvent::ReportSent {
                intent_id: &report.intent_id,
                status: &report.status,
            })
            .await;
        let _ = self.link.report_tx.send(report).await;
    }
}
