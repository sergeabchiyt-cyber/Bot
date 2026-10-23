//! Node 4 — the execution service.
//!
//! By default Node 4 is an outbound **client** of Node 3's strategy engine: it
//! receives `trade_intent` frames on `NODE3_WS_URL`
//! (`wss://<node3-host>/execution`), validates them, durably records the
//! `intent_id`, then places **at most one** broker order and reports the
//! broker-authoritative result back.
//!
//! It owns all execution concerns — venue selection, stake/lot size, account
//! risk, the Deriv options adapter, the Chelsea MCP adapter, the MT5 demo
//! execution adapter and its outbound `mt5-bridge`, broker/account state,
//! reconciliation, the kill switch, and the audit ledger.
//!
//! Two intent sources are possible, never both at once:
//!
//! * `NODE4_INTENT_SOURCE=node3` (default): the original execution-only mode.
//!   Node 4 dials Node 3's authenticated `/execution` socket and contains no
//!   strategy or market analytics.
//! * `NODE4_INTENT_SOURCE=node1_strategy` (opt-in): Node 4 consumes the
//!   versioned Node 1 market feed and runs the ported `vp_break_retest_v1`
//!   strategy. It still owns only execution policy: the strategy emits an
//!   immutable intent that goes through the same validation, durable ledger,
//!   venue policy and reconciliation as a Node 3 intent.

mod config;
mod diagnostics;
mod execution;
mod execution_chelsea;
mod execution_deriv;
mod execution_mt5;
mod health;
mod intent;
mod ledger;
mod node1;
mod node3;
mod strategy;
mod strategy_runner;
mod types;

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::{Config, IntentSource};
use diagnostics::DiagnosticsHub;
use execution::ExecutionManager;
use ledger::ExecutionLedger;
use node1::MarketFeed;
use strategy_runner::StrategyHandle;

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
        error!(
            "refusing to start — Node 4 never guesses a venue and never falls back to another one"
        );
        std::process::exit(2);
    }

    info!("Starting Node 4 (Execution Service)");
    match config.intent_source {
        IntentSource::Node3 => info!(
            "Intent source: Node 3 ({})",
            config.node3_ws_url
        ),
        IntentSource::Node1Strategy => info!(
            "Intent source: local strategy on Node 1 feed ({})",
            config.node1_ws_url
        ),
    }
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

    // The strategy diagnostics section exists in both modes: disabled/monitor
    // when Node 3 is the source, live when the local strategy is.
    let strategy_handle = StrategyHandle::new(&config);
    hub.attach_strategy(strategy_handle.clone());

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
        "Node 4 execution pipeline ready (venue: {}, ledger: {})",
        config.execution_venue().label(),
        if ledger.available() {
            "durable"
        } else {
            "UNAVAILABLE"
        }
    );

    match config.intent_source {
        IntentSource::Node3 => {
            // Optional read-only monitor: the feed runs and is visible on
            // `/diagnostics`, but no intent is ever produced from it.
            if config.node1_feed_enabled {
                let feed = MarketFeed::new(&config);
                hub.attach_market_feed(feed.clone());
                let (tx, mut rx) = mpsc::channel(256);
                tokio::spawn(node1::run(config.clone(), feed, tx));
                tokio::spawn(async move {
                    while rx.recv().await.is_some() {}
                    info!("Node 1 monitor feed stopped");
                });
            }
            info!("Node 4 ready: awaiting trade_intent frames from Node 3");
            // Runs forever with bounded reconnect backoff.
            node3::run(config.clone(), hub.clone(), manager.clone(), ledger.clone()).await;
        }
        IntentSource::Node1Strategy => {
            let feed = MarketFeed::new(&config);
            hub.attach_market_feed(feed.clone());
            let (tx, rx) = mpsc::channel(256);
            let shared = node3::LinkShared::for_local(
                config.clone(),
                hub.clone(),
                manager.clone(),
                ledger.clone(),
                feed.clone(),
            );
            info!(
                "Node 4 ready: local strategy on {} (dry-run unless a venue is configured)",
                config.node1_ws_url
            );
            tokio::spawn(node1::run(config.clone(), feed.clone(), tx));
            strategy_runner::run(config.clone(), feed, shared, strategy_handle, rx).await;
        }
    }
    Ok(())
}
