//! Node 4 — XAUUSD execution service (execution_only).
//!
//! Consumes trade intents from Node 3 over the authenticated execution
//! endpoint, validates them, claims them in the durable idempotency ledger,
//! executes at most one broker order per intent on the single configured
//! venue, and reports broker-authoritative outcomes back to Node 3.
//!
//! This service owns execution, MT5, account state, reconciliation, controls,
//! and audit. It does **not** own market analytics or strategy triggers: it
//! never scans, never builds levels, and never decides *when* to trade.

mod config;
mod diagnostics;
mod execution;
mod execution_chelsea;
mod execution_deriv;
mod execution_mt5;
mod health;
mod intent;
mod ledger;
mod node3_link;
mod types;

use std::sync::Arc;

use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::{Config, SERVICE_NAME, SERVICE_ROLE};
use diagnostics::DiagnosticsHub;
use execution::ExecutionManager;
use execution_deriv::spawn_deriv_monitor;
use execution_mt5::Mt5BridgeLink;
use ledger::ExecutionLedger;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("tokio_tungstenite=off".into()))
        .init();

    info!("Starting {SERVICE_NAME} (role: {SERVICE_ROLE})");

    let config = Config::from_env();

    // Fail closed on a contradictory configuration (e.g. two venue credential
    // sets, or a venue with no credentials): the process must not start in a
    // state where it might trade the wrong venue.
    let startup_errors = config.startup_errors();
    let mut fatal = false;
    for startup_error in &startup_errors {
        error!("startup configuration error: {startup_error}");
        if startup_error.contains("venue") {
            fatal = true;
        }
    }
    if fatal {
        error!("refusing to start: fix the execution venue configuration first");
        std::process::exit(1);
    }

    info!(
        "Execution venue: {} (dry-run: {})",
        config.execution_venue().label(),
        config.execution_venue() == config::ExecutionVenue::None
    );
    info!("HTTP & WS port: {}", config.port);
    info!(
        "Node 3 link: {}",
        if config.node3_configured() {
            "configured (wss, token auth)"
        } else {
            "not configured — monitor-only mode, no intent consumption"
        }
    );

    // Durable idempotency ledger. A real venue must never run without it.
    let ledger_path = config.execution_ledger_file.clone();
    let ledger = match ExecutionLedger::open(&ledger_path) {
        Ok(ledger) => {
            info!(
                "execution ledger: {} ({} intents on record)",
                ledger_path.display(),
                ledger.len()
            );
            Arc::new(ledger)
        }
        Err(err) => {
            if config.execution_venue() != config::ExecutionVenue::None {
                error!(
                    "could not open the durable execution ledger at {}: {err} — \
                     refusing to start with a real venue (fail closed)",
                    ledger_path.display()
                );
                std::process::exit(1);
            }
            // Venue none: no broker writes can happen, so a degraded ledger in
            // a temp location is safe (the dry-run audit trail just lands in
            // an unexpected place).
            let fallback = std::env::temp_dir().join("node4-execution-ledger-fallback.jsonl");
            warn!(
                "could not open the execution ledger at {}: {err} — venue is \
                 none, continuing with a fallback ledger at {}",
                ledger_path.display(),
                fallback.display()
            );
            match ExecutionLedger::open(&fallback) {
                Ok(ledger) => Arc::new(ledger),
                Err(err) => {
                    error!("could not open the fallback ledger either: {err}");
                    std::process::exit(1);
                }
            }
        }
    };

    // The MT5 bridge link is shared between the execution venue and the
    // diagnostics hub so `/mt5/*` reports on the same session the executor
    // places orders through.
    let mt5_link = if config.mt5_configured() {
        Some(Arc::new(Mt5BridgeLink::new(&config)))
    } else {
        None
    };

    let hub = DiagnosticsHub::new(&config, mt5_link.clone());

    let exec = Arc::new(ExecutionManager::new(config.clone(), mt5_link));

    let (link, intent_rx) = node3_link::spawn(&config, hub.clone());

    intent::spawn(
        Arc::new(config.clone()),
        hub.clone(),
        ledger,
        exec.clone(),
        link,
        intent_rx,
    );

    // Deriv options monitor: keeps the account/portfolio state fresh for
    // `/account` and the diagnostics snapshot. Only when the Deriv venue is
    // configured.
    if config.deriv_configured() {
        spawn_deriv_monitor(config.clone(), hub.clone());
    }

    health::serve(config.port, hub, config).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_identity_is_node4_execution_only() {
        assert_eq!(SERVICE_NAME, "xauusd-node4-execution");
        assert_eq!(SERVICE_ROLE, "execution_only");
    }
}
