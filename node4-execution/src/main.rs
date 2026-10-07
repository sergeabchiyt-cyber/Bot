//! Node 4 — the execution service.
//!
//! Node 4 is an outbound **client** of Node 3's strategy engine: it receives
//! `trade_intent` frames on `NODE3_WS_URL` (`wss://<node3-host>/execution`),
//! validates them, durably records the `intent_id`, then places **at most one**
//! broker order and reports the broker-authoritative result back.
//!
//! It owns all execution concerns — venue selection, stake/lot size, account
//! risk, the Deriv options adapter, the Chelsea MCP adapter, the MT5 demo
//! execution adapter and its outbound `mt5-bridge`, broker/account state,
//! reconciliation, the kill switch, and the audit ledger.
//!
//! It owns **no** strategy logic and **no** market analytics: no indicators, no
//! volume profile, no candles, no trigger state machine. Those live in Node 1
//! (market data) and Node 3 (strategy).

mod config;
mod diagnostics;
mod execution;
mod execution_chelsea;
mod execution_deriv;
mod execution_mt5;
mod health;
mod intent;
mod ledger;
mod node3;
mod types;

use std::sync::Arc;

use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::{Config, EXECUTION_PROTOCOL_VERSION};
use diagnostics::DiagnosticsHub;
use execution::ExecutionManager;
use ledger::ExecutionLedger;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();

    // Fail fast. A process that cannot authenticate to Node 3, or whose venue
    // configuration contradicts itself, must not look healthy: Node 4 never
    // guesses a peer, a venue or a fallback.
    let startup_errors = config.startup_errors();
    if !startup_errors.is_empty() {
        for err in &startup_errors {
            error!("startup configuration error: {err}");
        }
        error!("refusing to start — Node 4 never guesses a venue and never falls back to another one");
        std::process::exit(2);
    }

    info!("Starting Node 4 (Execution Service) — role execution_only");
    info!(
        "Node 3 intent input: {} (protocol version {EXECUTION_PROTOCOL_VERSION})",
        config.node3_ws_url
    );
    info!("HTTP & WS port: {}", config.port);
    info!("Execution venue: {}", config.execution_venue().label());

    let hub = DiagnosticsHub::new(&config);

    // The durable idempotency ledger is opened (and replayed) before any intent
    // can be accepted: `intent_id` must survive a restart.
    let ledger = Arc::new(ExecutionLedger::open(&config.execution_ledger_file));
    if ledger.available() {
        info!(
            "Execution ledger ready: {} ({})",
            config.execution_ledger_file,
            ledger.records()
        );
    } else {
        warn!(
            "Execution ledger is UNAVAILABLE at {}: {} — a real venue will refuse to trade until \
             this is fixed",
            config.execution_ledger_file,
            ledger
                .open_error()
                .unwrap_or_else(|| "unknown error".into())
        );
    }
    hub.attach_ledger(ledger.clone()).await;

    // MT5 demo bridge status at startup. The bridge dials out to this service
    // (`WS /mt5/bridge`) and authenticates with `bridge_hello`; Node 4 never
    // connects to the terminal host and never holds the MT5 password.
    if config.mt5_configured() {
        info!(
            "MT5 venue: requested symbol {} | lots {} | order timeout {} ms | history page {} | control {}",
            config.mt5_symbol,
            config.mt5_volume_lots,
            config.mt5_order_timeout_ms,
            config.mt5_history_page_size,
            if config.mt5_control_enabled() {
                "enabled (/mt5/control)"
            } else {
                "disabled (MT5_CONTROL_TOKEN not set)"
            }
        );
    } else {
        info!("MT5 venue: MT5_BRIDGE_TOKEN not set — the MT5 bridge view stays inert");
    }
    if let Some(err) = config.venue_selection_error() {
        warn!("Execution venue configuration error: {err}");
    }

    // Make the Deriv credential shape explicit at startup: PAT (pat_...) tokens
    // are rejected by Deriv unless DERIV_APP_ID is set (Deriv-App-ID header).
    info!(
        "Deriv credential: token kind {} | {}: {}",
        execution_deriv::token_kind_label(config.deriv_demo_api.as_deref()),
        execution_deriv::DERIV_APP_ID_ENV_VAR,
        if config.deriv_app_id_configured() {
            "set"
        } else {
            "not set"
        }
    );
    if let Some(hint) = execution_deriv::deriv_setup_hint(
        config.deriv_demo_api.as_deref(),
        config.deriv_app_id.as_deref(),
    ) {
        warn!("{hint}");
    }

    // Share the hub's bridge link so the venue that places orders and the
    // `/mt5/*` resources the operator watches are the same session.
    let manager = Arc::new(ExecutionManager::new(&config, hub.mt5()));

    // HTTP (/health, /diagnostics, /open-trades, /account, /mt5/*,
    // POST /mt5/control) and WebSocket (/ws frontends, /mt5/bridge for the
    // bridge) on 0.0.0.0:$PORT.
    tokio::spawn(health::serve(config.port, hub.clone(), config.clone()));

    // Deriv Demo account monitor (balance & open contracts) when DERIV_DEMO_API
    // is configured. Read-only: it never places an order.
    tokio::spawn(execution_deriv::spawn_deriv_monitor(
        config.clone(),
        hub.clone(),
    ));

    info!(
        "Node 4 ready: awaiting trade_intent frames from Node 3 over the execution link \
         (venue: {}, ledger: {})",
        config.execution_venue().label(),
        if ledger.available() {
            "durable"
        } else {
            "UNAVAILABLE"
        }
    );

    // Runs forever with bounded reconnect backoff.
    node3::run(config.clone(), hub.clone(), manager.clone(), ledger.clone()).await;
    Ok(())
}
