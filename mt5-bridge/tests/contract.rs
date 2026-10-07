//! Contract tests: the acceptance criteria of the design document, exercised
//! end-to-end against a fake MT5 terminal.
//!
//! ```text
//!   Node 4 command ──> Bridge ──> TerminalTransport ──> fake terminal
//!                       │                                  │
//!                       └────────── OrderOutcome <──────────┘
//! ```
//!
//! Covered: accepted orders, rejected orders, duplicate request ids/idempotency
//! keys, no-link (fail closed), stale quotes, account-mode mismatch, a fill
//! discovered by reconciliation after a timeout, a contradictory "filled with
//! no volume" response, explicit symbol mapping, halt + flatten, and the
//! account snapshot contract (`configured`/`connected`/`authorized`/
//! `account_type`/`error`).
//!
//! Run with `cargo test` (see `README.md`); the CI workflow runs the same
//! command for this crate.

use std::sync::Arc;

use mt5_bridge::bridge::{Bridge, ControlState, HistoryStore, OrderIntent};
use mt5_bridge::config::BridgeConfig;
use mt5_bridge::fake::{DisconnectedTerminal, FakeConfig, FakeLink, FakeTerminal, OrderMode};
use mt5_bridge::snapshot::Mt5Position;
use mt5_bridge::terminal::{AccountType, TerminalClient};

const MAGIC: i64 = 330_033;

fn base_config(history_path: &str) -> BridgeConfig {
    let mut symbol_map = std::collections::BTreeMap::new();
    symbol_map.insert("XAUUSD".to_string(), "XAUUSD.a".to_string());
    BridgeConfig {
        node4_ws_url: "ws://127.0.0.1:9/ws".into(),
        node4_token: Some("test-bridge-token".into()),
        ea_bind_addr: "127.0.0.1".into(),
        ea_port: 0,
        ea_token: Some("test-ea-token".into()),
        expected_login: Some(123_456),
        account_password_set: true,
        requested_symbol: "XAUUSD".into(),
        symbol_map,
        default_volume_lots: 0.01,
        order_timeout_ms: 5_000,
        request_timeout_ms: 2_000,
        account_max_age_ms: 5_000,
        max_quote_age_ms: 3_000,
        history_file: history_path.into(),
        history_page_size: 100,
        push_interval_ms: 1_000,
        max_deviation_points: 20,
        magic: MAGIC,
        trading_enabled_on_start: true,
        halt_on_ea_disconnect: true,
        sim_terminal: true,
        allow_other_symbol: false,
    }
}

struct Harness {
    bridge: Arc<Bridge>,
    fake: Arc<FakeTerminal>,
    link: Arc<FakeLink>,
    history_path: String,
}

fn unique_history_path(tag: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mt5-bridge-contract-{tag}-{}-{}.jsonl",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    path.to_string_lossy().to_string()
}

async fn harness_with(fake: Arc<FakeTerminal>, link: Arc<FakeLink>, tag: &str) -> Harness {
    let history_path = unique_history_path(tag);
    let cfg = Arc::new(base_config(&history_path));
    let history = Arc::new(HistoryStore::open(&history_path).await.unwrap());
    let terminal = TerminalClient::new(fake.clone(), cfg.request_timeout_ms, cfg.order_timeout_ms);
    let bridge = Arc::new(Bridge::new(
        cfg,
        terminal,
        link.clone(),
        Arc::new(ControlState::new(true)),
        history,
    ));
    Harness {
        bridge,
        fake,
        link,
        history_path,
    }
}

async fn harness(tag: &str) -> Harness {
    harness_with(FakeTerminal::demo(), FakeLink::connected_demo(), tag).await
}

impl Harness {
    fn cleanup(&self) {
        let _ = std::fs::remove_file(&self.history_path);
    }
}

fn intent(key: &str, side: &str) -> OrderIntent {
    OrderIntent {
        idempotency_key: key.into(),
        intent_id: format!("N3-{key}"),
        strategy_id: "vp-break-retest".into(),
        requested_symbol: Some("XAUUSD".into()),
        side: side.into(),
        volume: Some(0.01),
        sl: Some(if side == "buy" { 2648.10 } else { 2651.90 }),
        tp: Some(if side == "buy" { 2656.10 } else { 2643.90 }),
        level_name: Some("PW PoC".into()),
        entry_ref: None,
        timeout_ms: None,
    }
}

// 1. Accepted order ----------------------------------------------------------

#[tokio::test]
async fn accepted_order_is_confirmed_with_broker_tickets() {
    let h = harness("accepted").await;
    let outcome = h.bridge.place_order(intent("k1", "buy")).await;

    assert_eq!(outcome.status, "filled", "unexpected outcome: {outcome:?}");
    assert_eq!(outcome.broker_symbol, "XAUUSD");
    assert!(outcome.position_ticket.is_some());
    assert!(outcome.order_ticket.is_some());
    assert!(outcome.deal_ticket.is_some());
    assert_eq!(outcome.filled_volume, 0.01);
    assert!(outcome.is_confirmed_fill());
    // XAUUSD: 2.00 dollars of stop on 0.01 lots = 200 ticks * $1 * 0.01 = $2.
    assert!((outcome.risk_amount.unwrap() - 2.0).abs() < 1e-6);
    assert_eq!(outcome.risk_currency, "USD");

    // The broker really has a position carrying our magic + intent comment.
    let positions = h.fake.positions().await;
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].magic, MAGIC);
    assert!(positions[0].comment.contains("N3-k1"));

    h.cleanup();
}

// 2. Rejected order ----------------------------------------------------------

#[tokio::test]
async fn rejected_order_is_reported_and_creates_no_position() {
    let fake = FakeTerminal::demo();
    fake.set_order_mode(OrderMode::Reject {
        retcode: 10_016,
        description: "invalid stops".into(),
    })
    .await;
    let h = harness_with(fake, FakeLink::connected_demo(), "rejected").await;

    let outcome = h.bridge.place_order(intent("k1", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert_eq!(outcome.retcode, 10_016);
    assert!(!outcome.is_confirmed_fill());
    assert!(h.fake.positions().await.is_empty());

    // A rejection is terminal for that idempotency key: a replay does not
    // resend, it replays.
    let replay = h.bridge.place_order(intent("k1", "buy")).await;
    assert_eq!(replay.status, "rejected");
    assert_eq!(h.fake.order_send_count().await, 1);

    h.cleanup();
}

// 3. Duplicate request ids / idempotency -------------------------------------

#[tokio::test]
async fn duplicate_idempotency_key_places_exactly_one_order() {
    let h = harness("duplicate").await;
    let first = h.bridge.place_order(intent("dup-1", "buy")).await;
    assert!(first.is_confirmed_fill());

    let second = h.bridge.place_order(intent("dup-1", "buy")).await;
    assert!(second.is_confirmed_fill());
    assert_eq!(
        second.position_ticket, first.position_ticket,
        "a duplicate must resolve to the same broker position"
    );
    assert!(second.reconciled, "a replay must be marked as reconciled");

    let third = h.bridge.place_order(intent("dup-1", "buy")).await;
    assert!(third.is_confirmed_fill());

    assert_eq!(
        h.fake.order_send_count().await,
        1,
        "the broker must see the order exactly once"
    );
    assert_eq!(h.fake.positions().await.len(), 1);

    h.cleanup();
}

// 4. Timeout -> reconciliation ----------------------------------------------

#[tokio::test]
async fn timed_out_order_is_reconciled_into_a_confirmed_fill() {
    let fake = FakeTerminal::demo();
    fake.set_order_mode(OrderMode::FillThenTimeout).await;
    let h = harness_with(fake, FakeLink::connected_demo(), "timeout").await;

    let outcome = h.bridge.place_order(intent("late-1", "buy")).await;
    assert_eq!(
        outcome.status, "filled",
        "an order that filled despite a timeout must be reconciled, not lost"
    );
    assert!(outcome.reconciled);
    assert!(outcome.is_confirmed_fill());
    assert!(outcome.position_ticket.is_some());
    assert_eq!(h.fake.positions().await.len(), 1);

    h.cleanup();
}

// 5. Contradictory fill ------------------------------------------------------

#[tokio::test]
async fn filled_without_a_volume_is_not_treated_as_a_confirmation() {
    let fake = FakeTerminal::demo();
    fake.set_order_mode(OrderMode::FillWithoutVolume).await;
    let h = harness_with(fake, FakeLink::connected_demo(), "novolume").await;

    let outcome = h.bridge.place_order(intent("nv-1", "buy")).await;
    // The position is found by reconciliation, so this resolves to filled with
    // the broker's real volume...
    assert_eq!(outcome.status, "filled");
    assert_eq!(outcome.filled_volume, 0.01);
    assert!(outcome.is_confirmed_fill());

    // ...but had the broker shown no position/deal at all, it would be
    // `unknown` — which `is_confirmed_fill()` rejects. Verified directly:
    let mut unresolved = outcome.clone();
    unresolved.status = "unknown".into();
    unresolved.filled_volume = 0.0;
    assert!(!unresolved.is_confirmed_fill());

    h.cleanup();
}

// 6. Stale quote -------------------------------------------------------------

#[tokio::test]
async fn stale_quote_and_closed_market_are_refused_before_sending() {
    let fake = FakeTerminal::demo();
    fake.set_quote_age_ms(30_000).await;
    let h = harness_with(fake, FakeLink::connected_demo(), "stale").await;

    let outcome = h.bridge.place_order(intent("s1", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert!(outcome.error.unwrap().contains("stale"));
    assert_eq!(
        h.fake.order_send_count().await,
        0,
        "a stale quote must never reach the broker"
    );

    // A symbol that is not fully tradable is refused too.
    let fake = FakeTerminal::demo();
    fake.set_trade_mode(3).await; // close only
    let h = harness_with(fake, FakeLink::connected_demo(), "closemode").await;
    let outcome = h.bridge.place_order(intent("s2", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert_eq!(h.fake.order_send_count().await, 0);

    h.cleanup();
}

// 7. Account mode mismatch ---------------------------------------------------

#[tokio::test]
async fn real_account_is_refused_and_halts_the_bridge() {
    let h = harness_with(
        FakeTerminal::real_account(),
        FakeLink::connected_demo(),
        "real",
    )
    .await;

    let outcome = h.bridge.place_order(intent("r1", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert!(outcome.error.unwrap().contains("demo"));
    assert_eq!(h.fake.order_send_count().await, 0, "no order may be sent");

    // The bridge is halted, and it stays halted: an explicit resume is refused
    // while the account is not demo.
    assert!(h.bridge.control().is_halted().await);
    assert!(h.bridge.resume().await.is_err());
    assert!(h.bridge.control().is_halted().await);

    let account = h.bridge.account_snapshot().await;
    assert_eq!(account.account_type, "real");
    assert!(!account.authorized);
    assert!(account.halted);

    h.cleanup();
}

#[tokio::test]
async fn account_mode_change_mid_session_halts_trading() {
    let h = harness("modechange").await;
    assert!(h
        .bridge
        .place_order(intent("m1", "buy"))
        .await
        .is_confirmed_fill());
    assert!(!h.bridge.control().is_halted().await);

    // Terminal flips to a real account: the heartbeat handler must halt.
    h.fake.set_account_type(AccountType::Real).await;
    h.bridge
        .on_link_event(mt5_bridge::ea_link::LinkEvent::Heartbeat(Box::new(
            mt5_bridge::ea_link::HeartbeatInfo {
                login: 123_456,
                mode: "real".into(),
                connected: true,
                trade_allowed: true,
                ea_ts: 0,
                received_at: mt5_bridge::terminal::now_ms(),
            },
        )))
        .await;
    assert!(h.bridge.control().is_halted().await);

    let outcome = h.bridge.place_order(intent("m2", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert_eq!(
        h.fake.order_send_count().await,
        1,
        "the second order must not send"
    );

    h.cleanup();
}

// 8. No link / read-only link ------------------------------------------------

#[tokio::test]
async fn missing_or_read_only_ea_link_fails_closed() {
    // No EA session at all.
    let history_path = unique_history_path("nolink");
    let cfg = Arc::new(base_config(&history_path));
    let history = Arc::new(HistoryStore::open(&history_path).await.unwrap());
    let bridge = Arc::new(Bridge::new(
        cfg.clone(),
        TerminalClient::new(Arc::new(DisconnectedTerminal), 500, 500),
        FakeLink::new(mt5_bridge::bridge::LinkStatusView {
            connected: false,
            write_enabled: false,
            ..Default::default()
        }),
        Arc::new(ControlState::new(true)),
        history,
    ));
    let outcome = bridge.place_order(intent("n1", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert!(outcome.error.unwrap().contains("not connected"));

    // Connected but read-only (MT5_EA_TOKEN unset): the write is refused.
    let fake = FakeTerminal::new(FakeConfig {
        refuse_writes: Some("EA link is read-only".into()),
        ..FakeConfig::default()
    });
    let link = FakeLink::connected_demo();
    link.set_write_enabled(false).await;
    let h = harness_with(fake, link, "readonly").await;
    let outcome = h.bridge.place_order(intent("ro1", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert_eq!(h.fake.order_send_count().await, 0);
    assert!(h.bridge.account_snapshot().await.authorized == false);

    let _ = std::fs::remove_file(&history_path);
    h.cleanup();
}

// 9. Halt / kill switch ------------------------------------------------------

#[tokio::test]
async fn halt_refuses_new_orders_and_flatten_closes_positions() {
    let h = harness("halt").await;
    assert!(h
        .bridge
        .place_order(intent("h1", "buy"))
        .await
        .is_confirmed_fill());
    assert_eq!(h.fake.positions().await.len(), 1);

    // Soft halt: no orders, positions stay.
    h.bridge.halt("manual stop", false).await;
    let outcome = h.bridge.place_order(intent("h2", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert!(outcome.error.unwrap().contains("halted"));
    assert_eq!(h.fake.positions().await.len(), 1);
    assert_eq!(h.fake.order_send_count().await, 1);

    // Flatten halt: positions are closed through the broker.
    h.bridge.halt("flatten now", true).await;
    assert!(h.fake.positions().await.is_empty());
    assert!(!h.fake.deals().await.is_empty());

    // Resume re-verifies (demo + connected + writable) and clears the halt.
    h.bridge.resume().await.expect("resume should succeed");
    assert!(!h.bridge.control().is_halted().await);
    assert!(h
        .bridge
        .place_order(intent("h3", "buy"))
        .await
        .is_confirmed_fill());

    h.cleanup();
}

#[tokio::test]
async fn close_all_and_close_position_only_touch_our_own_magic() {
    let fake = FakeTerminal::demo();
    // A manual position that is not ours must be left alone.
    fake.add_position(Mt5Position {
        ticket: 999,
        symbol: "XAUUSD".into(),
        side: "buy".into(),
        volume: 0.5,
        price_open: 2600.0,
        comment: "manual".into(),
        magic: 0,
        ..Default::default()
    })
    .await;
    let h = harness_with(fake, FakeLink::connected_demo(), "closeall").await;

    let filled = h.bridge.place_order(intent("c1", "buy")).await;
    assert!(filled.is_confirmed_fill());
    let ticket = filled.position_ticket.unwrap();

    let closed = h.bridge.close_all("test").await.unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].ticket, ticket);

    let remaining = h.fake.positions().await;
    assert_eq!(remaining.len(), 1, "the manual position must survive");
    assert_eq!(remaining[0].ticket, 999);

    // Closing someone else's ticket is refused.
    assert!(h.bridge.close_position(999, None).await.is_err());

    h.cleanup();
}

// 10. Symbol mapping ---------------------------------------------------------

#[tokio::test]
async fn symbol_mapping_is_explicit_and_suffixes_are_never_guessed() {
    // The terminal offers XAUUSD.a (mapped) but not XAUUSD.
    let mut symbols = std::collections::HashMap::new();
    symbols.insert(
        "XAUUSD.a".to_string(),
        mt5_bridge::fake::xauusd_spec("XAUUSD.a"),
    );
    let fake = FakeTerminal::new(FakeConfig {
        symbols,
        ..FakeConfig::default()
    });
    let h = harness_with(fake, FakeLink::connected_demo(), "map").await;

    let outcome = h.bridge.place_order(intent("sm1", "buy")).await;
    assert_eq!(outcome.status, "filled");
    assert_eq!(outcome.broker_symbol, "XAUUSD.a");

    // Without the explicit mapping the same request fails closed.
    let mut symbols = std::collections::HashMap::new();
    symbols.insert(
        "XAUUSD.a".to_string(),
        mt5_bridge::fake::xauusd_spec("XAUUSD.a"),
    );
    let fake = FakeTerminal::new(FakeConfig {
        symbols,
        ..FakeConfig::default()
    });
    let history_path = unique_history_path("nomap");
    let mut cfg = base_config(&history_path);
    cfg.symbol_map.clear();
    let cfg = Arc::new(cfg);
    let history = Arc::new(HistoryStore::open(&history_path).await.unwrap());
    let bridge = Arc::new(Bridge::new(
        cfg.clone(),
        TerminalClient::new(fake.clone(), 2_000, 5_000),
        FakeLink::connected_demo(),
        Arc::new(ControlState::new(true)),
        history,
    ));
    let outcome = bridge.place_order(intent("sm2", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert!(outcome.error.unwrap().contains("MT5_SYMBOL_MAP"));
    assert_eq!(fake.order_send_count().await, 0);

    let _ = std::fs::remove_file(&history_path);
    h.cleanup();
}

// 11. Partial fill -----------------------------------------------------------

#[tokio::test]
async fn partial_fill_reports_the_broker_reported_volume() {
    let fake = FakeTerminal::demo();
    fake.set_order_mode(OrderMode::Partial { volume: 0.01 })
        .await;
    let h = harness_with(fake, FakeLink::connected_demo(), "partial").await;

    // The strategy asks for 0.05; the broker only fills 0.01 of it.
    let mut request = intent("p1", "buy");
    request.volume = Some(0.05);
    let outcome = h.bridge.place_order(request).await;

    assert_eq!(outcome.status, "partial");
    assert_eq!(outcome.filled_volume, 0.01);
    assert_eq!(outcome.requested_volume, 0.05);
    assert!(outcome.is_confirmed_fill());

    h.cleanup();
}

// 12. Account snapshot contract ---------------------------------------------

#[tokio::test]
async fn account_snapshot_exposes_the_required_diagnostic_fields() {
    let h = harness("snapshot").await;
    let snapshot = h.bridge.account_snapshot().await;
    assert!(snapshot.configured);
    assert!(snapshot.connected);
    assert!(snapshot.authorized);
    assert_eq!(snapshot.account_type, "demo");
    assert_eq!(snapshot.login, Some(123_456));
    assert_eq!(snapshot.server.as_deref(), Some("Deriv-Demo"));
    assert_eq!(snapshot.requested_symbol, "XAUUSD");
    assert!(snapshot.error.is_none());
    assert!(snapshot.balance.unwrap() > 0.0);

    // A disconnected link flips connected/authorized without inventing data.
    h.link.set_connected(false).await;
    let snapshot = h.bridge.account_snapshot().await;
    assert!(!snapshot.connected);
    assert!(!snapshot.authorized);

    h.cleanup();
}

// 13. Reconciliation of unledgered positions ---------------------------------

#[tokio::test]
async fn positions_from_a_previous_run_are_adopted_not_ignored() {
    let fake = FakeTerminal::demo();
    fake.add_position(Mt5Position {
        ticket: 4_242,
        symbol: "XAUUSD".into(),
        side: "buy".into(),
        volume: 0.01,
        price_open: 2650.0,
        sl: 2648.0,
        comment: "N3-old-intent".into(),
        magic: MAGIC,
        time_ms: mt5_bridge::terminal::now_ms(),
        ..Default::default()
    })
    .await;
    let h = harness_with(fake, FakeLink::connected_demo(), "adopt").await;

    let adopted = h.bridge.reconcile_positions().await;
    assert_eq!(adopted.len(), 1);
    assert_eq!(adopted[0].ticket, 4_242);

    // Re-running reconciliation is idempotent (the position is now ledgered).
    let adopted_again = h.bridge.reconcile_positions().await;
    assert!(adopted_again.is_empty());

    // And the adopted position is visible in the positions snapshot.
    let snapshot = h.bridge.positions_snapshot().await;
    assert_eq!(snapshot.count, 1);
    assert_eq!(snapshot.positions[0].ticket, 4_242);

    h.cleanup();
}

// 14. History -----------------------------------------------------------------

#[tokio::test]
async fn closed_deals_persist_and_history_pages_by_cursor() {
    let h = harness("history").await;
    let filled = h.bridge.place_order(intent("hd1", "buy")).await;
    assert!(filled.is_confirmed_fill());
    let ticket = filled.position_ticket.unwrap();
    h.bridge.close_position(ticket, None).await.unwrap();

    let first = h.bridge.history_snapshot(None).await;
    assert!(first.count >= 1, "closing a position must produce a deal");
    assert!(first.deals.iter().any(|deal| deal.entry == "out"));
    assert!(first.deals.iter().filter(|d| d.entry == "out").count() == 1);

    // Cursor-based paging is stable and reports completion.
    let cursor: usize = first.cursor.clone().unwrap().parse().unwrap();
    let next = h.bridge.history_snapshot(Some(cursor)).await;
    assert_eq!(next.count, 0);
    assert!(next.complete);

    // A Node 4 restart is simulated by re-reading the same file.
    let reopened = HistoryStore::open(&h.history_path).await.unwrap();
    assert_eq!(reopened.deal_count().await, first.count);

    h.cleanup();
}

// 15. Volumes and stops are normalized, not trusted ---------------------------

#[tokio::test]
async fn volumes_and_stops_are_normalized_and_invalid_requests_never_send() {
    let h = harness("normalize").await;

    // Off-grid volume: floored to the broker step, never rounded up.
    let mut request = intent("v1", "buy");
    request.volume = Some(0.014);
    let outcome = h.bridge.place_order(request).await;
    assert!(outcome.is_confirmed_fill());
    assert_eq!(outcome.requested_volume, 0.01);

    // Volume below the broker minimum is refused.
    let mut request = intent("v2", "buy");
    request.volume = Some(0.001);
    let outcome = h.bridge.place_order(request).await;
    assert_eq!(outcome.status, "rejected");

    // A stop inside the broker stops level (100 points = $1.00) is refused.
    let mut request = intent("v3", "buy");
    request.sl = Some(2649.80);
    let outcome = h.bridge.place_order(request).await;
    assert_eq!(outcome.status, "rejected");
    assert!(outcome.error.unwrap().contains("stops level"));

    // A stop on the wrong side is refused.
    let mut request = intent("v4", "buy");
    request.sl = Some(2660.0);
    let outcome = h.bridge.place_order(request).await;
    assert_eq!(outcome.status, "rejected");

    assert_eq!(
        h.fake.order_send_count().await,
        1,
        "only the valid, normalized request may reach the broker"
    );

    h.cleanup();
}

// 16. Symbol metadata is required --------------------------------------------

#[tokio::test]
async fn missing_contract_metadata_blocks_trading() {
    // A terminal whose XAUUSD spec has no contract size: the bridge must refuse
    // rather than guess a risk size.
    let mut spec = mt5_bridge::fake::xauusd_spec("XAUUSD");
    spec.contract_size = 0.0;
    let mut symbols = std::collections::HashMap::new();
    symbols.insert("XAUUSD".to_string(), spec);
    let fake = FakeTerminal::new(FakeConfig {
        symbols,
        ..FakeConfig::default()
    });
    let h = harness_with(fake, FakeLink::connected_demo(), "metadata").await;

    let outcome = h.bridge.place_order(intent("md1", "buy")).await;
    assert_eq!(outcome.status, "rejected");
    assert_eq!(h.fake.order_send_count().await, 0);

    h.cleanup();
}
