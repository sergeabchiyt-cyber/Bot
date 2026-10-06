//! `mt5-bridge` — Deriv MT5 **demo** execution bridge (binary entry point).
//!
//! Deployment shape (see `docs/mt5/EXECUTION_ARCHITECTURE.md`):
//!
//! ```text
//!   Node 3 (Render, strategy + risk)
//!        ^  WSS — the bridge dials out, token authenticated
//!        |
//!   mt5-bridge  ── listens on 127.0.0.1:5055 ──> Mt5BridgeEA.mq5 in the terminal
//!        |                                              |
//!        +── appended JSONL history store               v
//!                                            Deriv MT5 **demo** server
//! ```
//!
//! The bridge runs next to the MT5 terminal (a terminal cannot run on a 512 MB
//! Free-tier container with no GUI) and is the only process that talks to it.
//! It refuses to start on a configuration that could reach a live account,
//! refuses every order while the demo guard fails, and never reports a fill the
//! broker did not confirm.

use std::sync::Arc;

use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use mt5_bridge::bridge::{Bridge, ControlState, HistoryStore, LinkStatusProvider};
use mt5_bridge::config::BridgeConfig;
use mt5_bridge::ea_link::{EaLink, LinkEvent};
use mt5_bridge::terminal::TerminalClient;
use mt5_bridge::{fake, node3, snapshot};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = Arc::new(BridgeConfig::from_env());
    info!(
        "mt5-bridge {} starting (protocol {})",
        snapshot::BRIDGE_VERSION,
        snapshot::BRIDGE_PROTOCOL_VERSION
    );

    // Fail closed: a misconfigured bridge must not run at all.
    let errors = cfg.validate();
    if !errors.is_empty() {
        for err in &errors {
            error!("configuration error: {err}");
        }
        anyhow::bail!(
            "refusing to start with {} configuration error(s) — fix them and restart",
            errors.len()
        );
    }

    if cfg.sim_terminal {
        warn!(
            "MT5_SIM_TERMINAL=1 — running against the in-process FAKE terminal. No broker \
             orders will be placed. Use this for protocol work only."
        );
    }

    let control = Arc::new(ControlState::new(cfg.trading_enabled_on_start));
    if !cfg.trading_enabled_on_start {
        warn!("MT5_TRADING_ENABLED=0 — the bridge starts halted");
    }

    let history = Arc::new(
        HistoryStore::open(&cfg.history_file)
            .await
            .map_err(|err| {
                anyhow::anyhow!(
                    "cannot open the history store {}: {err}",
                    cfg.history_file.display()
                )
            })?,
    );
    info!(
        "history store: {} ({} deals persisted)",
        history.path().display(),
        history.deal_count().await
    );

    // 1. Terminal transport (real EA link, or the fake in SIM mode).
    let (terminal, link, link_events): (
        TerminalClient,
        Arc<dyn LinkStatusProvider>,
        Option<tokio::sync::broadcast::Receiver<LinkEvent>>,
    ) = if cfg.sim_terminal {
        let fake_terminal = fake::FakeTerminal::demo();
        let fake_link = fake::FakeLink::connected_demo();
        (
            TerminalClient::new(fake_terminal, cfg.request_timeout_ms, cfg.order_timeout_ms),
            fake_link,
            None,
        )
    } else {
        let (ea_link, addr) = EaLink::bind(cfg.clone()).await?;
        info!("EA link bound on {addr}");
        let provider: Arc<dyn LinkStatusProvider> = ea_link.clone();
        let receiver = ea_link.subscribe();
        (
            TerminalClient::new(ea_link, cfg.request_timeout_ms, cfg.order_timeout_ms),
            provider,
            Some(receiver),
        )
    };

    // 2. Service layer.
    let bridge = Arc::new(Bridge::new(
        cfg.clone(),
        terminal,
        link,
        control.clone(),
        history.clone(),
    ));

    // 3. React to the terminal link: demo-mode changes and disconnects halt
    //    trading immediately instead of being discovered on the next order.
    if let Some(mut events) = link_events {
        let bridge = bridge.clone();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => bridge.on_link_event(event).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("dropped {skipped} EA link events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // 4. Startup guard + reconciliation. Neither is fatal: the bridge keeps
    //    retrying, and no order can be placed until the guard passes.
    match bridge.verify_account(true).await {
        Ok(account) => info!(
            "terminal account: login {} on {} ({}) balance {} {} — demo guard passed",
            account.login, account.server, account.company, account.balance, account.currency
        ),
        Err(err) => warn!(
            "startup account guard failed: {err} — the bridge will keep retrying; no order can \
             be placed until it passes"
        ),
    }
    let adopted = bridge.reconcile_positions().await;
    if !adopted.is_empty() {
        warn!(
            "reconciled {} broker position(s) that were not in the local ledger",
            adopted.len()
        );
    }
    let mirrored = bridge.fill_history_from_broker().await;
    info!("mirrored {mirrored} closed deal(s) into the history store");

    // 5. Node 3 link (outbound only) plus a graceful halt on shutdown.
    let node3_bridge = bridge.clone();
    let node3_cfg = cfg.clone();
    tokio::spawn(async move {
        node3::run(node3_cfg, node3_bridge).await;
    });

    if let Ok(()) = tokio::signal::ctrl_c().await {
        bridge.shutdown().await;
        info!("shutdown requested — new orders refused; exiting");
    }

    Ok(())
}
