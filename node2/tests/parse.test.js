// parse.test.js — resource parsing: every published payload shape must
// parse to normalized data, and malformed input must degrade, not crash.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  parseCandles, parseLevels, parseTickBars, parseCalendar, parseStatus,
  parseStrategy, parseDemo, parseDiagnostics, parseOpenTrades, parseMt5Account,
  parseMt5Positions, parseMt5History, parseMt5Status, parseBridgeEvent,
  parseExecutionSnapshot, parseDecision,
} from "../public/src/lib/parse.js";
import {
  makeCandles, makeLevels, makeTickBars, makeCalendar, makeStatus,
  makeStrategy, makeDemo, makeDiagnostics, makeOpenTrades, makeMt5Account,
  makeMt5Positions, makeMt5History, makeMt5Status, makeDerivAccount,
  makeBridgeEvent,
} from "../mock/feed.js";

const T = 1_700_000_000;

// ---- Node 1: candles ---------------------------------------------------------

test("parseCandles: live shape (sifting source)", () => {
  const input = [
    { time: T, open: 2662.49, high: 2662.85, low: 2661.98, close: 2661.99, volume: 68, source: "sifting" },
    { time: T + 900, open: 2661.99, high: 2663.4, low: 2661.5, close: 2663.2, volume: 120, source: "sifting" },
  ];
  const { bars, error } = parseCandles(input);
  assert.equal(error, null);
  assert.equal(bars.length, 2);
  assert.deepEqual(bars[0], {
    time: T, open: 2662.49, high: 2662.85, low: 2661.98, close: 2661.99, volume: 68, source: "sifting",
  });
});

test("parseCandles: drops entries without time or close", () => {
  const { bars } = parseCandles([
    { time: T, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 },
    { open: 1, close: 2 }, // no time
    { time: T + 1, open: 1 }, // no close
    "junk",
  ]);
  assert.equal(bars.length, 1);
  assert.equal(bars[0].time, T);
});

test("parseCandles: tolerates garbage without throwing", () => {
  for (const input of [undefined, null, "not json", 42, { not: "an array" }]) {
    const { bars, error } = parseCandles(input);
    assert.ok(Array.isArray(bars));
    assert.ok(error === null || typeof error === "string");
  }
});

// ---- Node 1: levels -----------------------------------------------------------

test("parseLevels: maps snake_case to display fields", () => {
  const input = [{
    window: "PT", poc: 2655.1, vah: 2670.0, val: 2640.0,
    start: T, end: T + 7200, timestamp: T, direction: "up",
    swing_high: 2675, swing_low: 2630, sunday_open: 2648.9,
  }];
  const { levels } = parseLevels(input);
  assert.equal(levels.length, 1);
  assert.equal(levels[0].window, "PT");
  assert.equal(levels[0].poc, 2655.1);
  assert.equal(levels[0].vah, 2670);
  assert.equal(levels[0].val, 2640);
  assert.equal(levels[0].swingHigh, 2675);
  assert.equal(levels[0].swingLow, 2630);
  assert.equal(levels[0].sundayOpen, 2648.9);
  assert.equal(levels[0].direction, "up");
});

// ---- Node 1: tick volume ------------------------------------------------------

test("parseTickBars: keeps closed flag and rates", () => {
  const { bars } = parseTickBars([{
    time: T, ticks: 1539, up_ticks: 800, down_ticks: 699, flat_ticks: 40,
    close: 2661.99, last_tick: "B", ticks_per_sec: 51.3, closed: true, source: "mt5",
  }]);
  assert.equal(bars.length, 1);
  assert.equal(bars[0].upTicks, 800);
  assert.equal(bars[0].downTicks, 699);
  assert.equal(bars[0].flatTicks, 40);
  assert.equal(bars[0].ticksPerSec, 51.3);
  assert.equal(bars[0].closed, true);
});

// ---- Node 1: calendar ---------------------------------------------------------

test("parseCalendar: count + event fields", () => {
  const cal = parseCalendar({
    count: 1,
    events: [{
      actual: "22B", currency: "USD", event: "CPI YoY", forecast: "3.1%",
      gold_relevant: true, impact: "High", previous: "3.2%", time: T, timestamp: T,
    }],
  });
  assert.equal(cal.count, 1);
  assert.equal(cal.events[0].event, "CPI YoY");
  assert.equal(cal.events[0].goldRelevant, true);
  assert.equal(cal.events[0].impact, "High");
});

test("parseCalendar: handles empty object", () => {
  const cal = parseCalendar({});
  assert.deepEqual(cal.events, []);
  assert.equal(cal.count, 0);
});

// ---- Node 1: status -----------------------------------------------------------

test("parseStatus: feeds + candle meta", () => {
  const st = parseStatus({
    version: "0.1.0", ts: T,
    feeds: { sifting: { state: "connected", msgs: 10432, last_msg: T - 4 } },
    candles: {
      interval: "15m", bars: 2000, first_bar: T - 2000 * 900, last_bar: T - 1800,
      edge_lag_minutes: 27, seed_bars: 0,
    },
    volume_profile: { input: "15m", row_mode: "pt", rows: 40, va_pct: 33.2 },
  });
  assert.equal(st.ok, true);
  assert.equal(st.version, "0.1.0");
  assert.equal(st.feeds.sifting.state, "connected");
  assert.equal(st.feeds.sifting.msgs, 10432);
  assert.equal(st.candles.interval, "15m");
  assert.equal(st.candles.intervalMin, 15);
  assert.equal(st.candles.edgeLagMinutes, 27);
  assert.equal(st.volumeProfile.input, "15m");
});

test("parseStatus: missing status object degrades", () => {
  assert.equal(parseStatus(undefined).ok, false);
  assert.equal(parseStatus("nope").ok, false);
});

// ---- Node 1: strategy / decisions ---------------------------------------------

test("parseDecision: full live-shaped decision", () => {
  const d = parseDecision({
    ts: T, symbol: "XAUUSD", action: "buy", score: 63, confidence: 71.4, price: 2661.99,
    atr: 2.94, regime: "range", volatility: "normal",
    stop_loss: 2657.74, take_profit: 2670.12, reward_risk: 1.8,
    lots: 1.51, risk_usd: 50,
    reasons: [
      { name: "range_position", value: "18.49 pts from range low", weight: 0.35, contribution: 0.22, detail: "Distance to range low" },
      { name: "volume_profile", value: "near POC 2655", weight: 0.3, contribution: 0.2, detail: "close to POC" },
    ],
    notes: ["note one"],
    next_event_ts: T + 1800,
  });
  assert.equal(d.action, "buy");
  assert.equal(d.confidence, 71.4);
  assert.equal(d.stopLoss, 2657.74);
  assert.equal(d.rewardRisk, 1.8);
  assert.equal(d.riskUsd, 50);
  assert.equal(d.reasons.length, 2);
  assert.equal(d.reasons[0].contribution, 0.22);
  assert.deepEqual(d.notes, ["note one"]);
});

test("parseDecision: invalid action falls back to unknown, never fabricates", () => {
  const d = parseDecision({ ts: T, action: "hold", confidence: 50 });
  assert.equal(d.action, "unknown");
});

test("parseStrategy: snapshot with history keyed by ts", () => {
  const s = parseStrategy({
    symbol: "XAUUSD", enabled: true, params: { atr_period: 14 },
    last: { ts: T, symbol: "XAUUSD", action: "buy" },
    history: [{ ts: T - 900, symbol: "XAUUSD", action: "sell" }, { ts: T, symbol: "XAUUSD", action: "buy" }],
  });
  assert.equal(s.ok, true);
  assert.equal(s.enabled, true);
  assert.equal(s.last.action, "buy");
  assert.equal(s.history.length, 2);
});

// ---- Node 1: demo snapshot -----------------------------------------------------

test("parseDemo: full live-shaped snapshot", () => {
  const d = parseDemo({
    enabled: true, mode: "demo", symbol: "XAUUSD", leverage: 1000,
    quote: { bid: 2661.94, ask: 2662.04, mid: 2661.99, spread: 0.1, ts: T },
    account: {
      currency: "USD", balance: 10000, equity: 10201.66, margin: 10.2,
      free_margin: 10191.46, margin_level: 99904.51, unrealized_pnl: 201.66,
      peak_equity: 10201.66, max_drawdown_pct: 3.2,
    },
    params: { start_balance: 10000, leverage: 1000 },
    position: {
      id: "position-1", side: "buy", lots: 0.75, entry: 2655.12,
      stop_loss: 2644.0, take_profit: 2675.0, opened_at: T - 900,
      risk_usd: 50, margin: 10.2, unrealized_pnl: 201.66, reason: "test",
    },
    stats: {
      closed_trades: 2, wins: 1, losses: 1, win_rate: 50,
      gross_profit: 300, gross_loss: -150, net_pnl: 150,
      profit_factor: 2.0, avg_r: 0.5, max_drawdown_pct: 2.0,
    },
    recent_trades: [{
      id: 41, side: "buy", lots: 0.8, entry: 2655.0, exit: 2662.0,
      opened_at: T - 1800, closed_at: T - 900, pnl: 560, r_multiple: 1.8, exit_reason: "tp",
    }],
    note: "Simulated demo", ts: T,
  });
  assert.equal(d.ok, true);
  assert.equal(d.quote.mid, 2661.99);
  assert.equal(d.account.unrealizedPnl, 201.66);
  assert.equal(d.account.marginLevel, 99904.51);
  assert.equal(d.position.takeProfit, 2675);
  assert.equal(d.recentTrades[0].rMultiple, 1.8);
  assert.equal(d.recentTrades[0].exitReason, "tp");
});

test("parseDemo: null position stays null", () => {
  const d = parseDemo({ enabled: true, position: null, account: {} });
  assert.equal(d.ok, true);
  assert.equal(d.position, null);
  assert.equal(d.account.balance, null);
});

// ---- Node 4: diagnostics -------------------------------------------------------

test("parseDiagnostics: node3 link + work counters + ledger", () => {
  const d = parseDiagnostics({
    timestamp: T,
    node3: { degraded: false, last_heartbeat: T - 1, latency_ms: 12, attempts: 990, errors: [] },
    work: {
      open_trades: 1, positions: 1, fills_attempted: 1, fills_confirmed: 1,
      fills_rejected: 0, fills_unknown: 0, cancels_sent: 0, cancels_accepted: 0,
    },
    ledger: { version: 1, entries: [{ seq: 1 }] },
  });
  assert.equal(d.ok, true);
  assert.equal(d.node3.degraded, false);
  assert.equal(d.node3.latencyMs, 12);
  assert.equal(d.work.fillsConfirmed, 1);
  assert.equal(d.work.fillsUnknown, 0);
  assert.equal(d.ledger.version, 1);
});

// ---- Node 4: open trades with lifecycle normalization ---------------------------

test("parseOpenTrades: normalizes lifecycle statuses", () => {
  const { trades } = parseOpenTrades([
    { id: "a", lifecycle: "filled", symbol: "XAUUSD", side: "buy", lots: 0.5, filled_volume: 0.5 },
    { id: "b", lifecycle: "accepted", symbol: "XAUUSD", side: "sell", lots: 0.3, filled_volume: 0 },
    { id: "c", lifecycle: "unknown", symbol: "XAUUSD", side: "buy", lots: 0.2, filled_volume: 0 },
    { id: "d", lifecycle: "partial", symbol: "XAUUSD", side: "buy", lots: 0.4, filled_volume: 0.15 },
    { id: "e", lifecycle: "weird", symbol: "XAUUSD", side: "buy", lots: 0.1 }, // unknown status
  ]);
  assert.equal(trades.length, 5);
  assert.equal(trades[0].lifecycle, "filled");
  assert.equal(trades[1].lifecycle, "accepted");
  assert.equal(trades[2].lifecycle, "unknown");
  assert.equal(trades[3].lifecycle, "partial");
  assert.equal(trades[4].lifecycle, "unknown"); // unknown status falls back to unknown
  assert.equal(trades[1].filledVolume, 0);
});

test("parseOpenTrades: mt5 match block mapped", () => {
  const { trades } = parseOpenTrades([{
    id: "a", lifecycle: "filled",
    mt5: { matched: true, ticket: 77001, symbol: "GOLD", status: "filled", filled_volume: 0.5 },
  }]);
  assert.equal(trades[0].mt5.matched, true);
  assert.equal(trades[0].mt5.ticket, 77001);
  assert.equal(trades[0].mt5.symbol, "GOLD");
});

// ---- Node 4: MT5 resources -----------------------------------------------------

test("parseMt5Account: all UI-rule fields", () => {
  const a = parseMt5Account({
    configured: true, connected: true, authorized: true, account_type: "demo",
    login: 11001001, server: "MetaQuotes-Demo", currency: "USD",
    balance: 10000, equity: 10024.5, margin: 12.4, margin_free: 10012.1,
    leverage: "1:500", trade_allowed: true, halted: false,
    requested_symbol: "XAUUSD", broker_symbol: "GOLD", symbol_digits: 2,
    latency_ms: 8, last_heartbeat: T - 2, last_updated: T,
    terminal_build: "5050", ea_version: "x", bridge_version: "0.1.0",
    error: null, setup_hint: null,
  });
  assert.equal(a.ok, true);
  assert.equal(a.configured, true);
  assert.equal(a.connected, true);
  assert.equal(a.authorized, true);
  assert.equal(a.accountType, "demo");
  assert.equal(a.requestedSymbol, "XAUUSD");
  assert.equal(a.brokerSymbol, "GOLD");
  assert.equal(a.halted, false);
});

test("parseMt5Positions: positions + halted flag", () => {
  const p = parseMt5Positions({
    positions: [{
      ticket: 77001, symbol: "GOLD", side: "buy", volume: 0.5, price_open: 2651.2,
      sl: 2644, tp: 2664, profit: 115, swap: 0, comment: "NODE1", magic: 2024001,
      time_ms: T - 1800, current_price: 2653.5, unrealized_pnl: 115,
    }],
    count: 1, total_volume: 0.5, total_unrealized_pnl: 115, halted: false,
    account_login: 11001001, account_type: "demo", source: "mt5_history", timestamp: T,
  });
  assert.equal(p.ok, true);
  assert.equal(p.positions.length, 1);
  assert.equal(p.positions[0].ticket, 77001);
  assert.equal(p.positions[0].unrealizedPnl, 115);
  assert.equal(p.halted, false);
});

test("parseMt5History: deals + cursor completeness", () => {
  const h = parseMt5History({
    deals: [{
      ticket: 76990, order_ticket: 76989, position_ticket: 0, symbol: "GOLD",
      side: "buy", volume: 0.4, price: 2648.1, profit: 160.2, swap: 0,
      commission: -1.2, comment: "NODE1", magic: 2024001, time_ms: T - 7200,
      entry: "in", reason: "OPEN",
    }],
    count: 1, cursor: null, complete: true, total_realized_pnl: 69.7,
    first_ms: T - 14400, last_ms: T - 7200, source: "mt5_history", timestamp: T,
  });
  assert.equal(h.ok, true);
  assert.equal(h.deals[0].entry, "in");
  assert.equal(h.complete, true);
  assert.equal(h.totalRealizedPnl, 69.7);
});

test("parseMt5Status: bridge counters incl. orders_unknown", () => {
  const s = parseMt5Status({
    configured: true, connected: true, authorized: true, protocol: "json_v2",
    bridge_version: "0.1.0", ea_connected: true, ea_write_enabled: false,
    halted: false, trading_enabled: false,
    orders_sent: 11, orders_filled: 10, orders_rejected: 1, orders_unknown: 0,
    positions_open: 1, history_deals: 36, last_error: null, uptime_secs: 86400,
    timestamp: T,
  });
  assert.equal(s.ok, true);
  assert.equal(s.ordersSent, 11);
  assert.equal(s.ordersFilled, 10);
  assert.equal(s.ordersRejected, 1);
  assert.equal(s.ordersUnknown, 0);
  assert.equal(s.tradingEnabled, false);
});

// ---- Node 4: bridge events -----------------------------------------------------

test("parseBridgeEvent: order outcomes carry lifecycle", () => {
  const e = parseBridgeEvent({
    type: "order_unknown", ts: T,
    outcome: {
      status: "unknown", idempotency_key: "idem-1", intent_id: "int-0013",
      requested_symbol: "XAUUSD", broker_symbol: "GOLD", side: "buy",
      requested_volume: 0.2, filled_volume: 0, price: null, sl: null, tp: null,
      order_ticket: null, deal_ticket: null, position_ticket: null,
      retcode: null, retcode_desc: null, risk_amount: 100, risk_currency: "USD",
      notional: null, reconciled: true, latency_ms: null, ts: T - 120,
      error: "no fill or reject received from broker",
    },
  });
  assert.equal(e.type, "order_unknown");
  assert.equal(e.outcome.status, "unknown");
  assert.equal(e.outcome.reconciled, true);
});

test("parseBridgeEvent: halted/resumed/ea_link_disconnected", () => {
  const h = parseBridgeEvent({ type: "halted", ts: T, halted: true, reason: "risk", auto: true, flattened: "1/1" });
  assert.equal(h.halted, true);
  assert.equal(h.reason, "risk");
  const r = parseBridgeEvent({ type: "resumed", ts: T, halted: false });
  assert.equal(r.halted, false);
  const d = parseBridgeEvent({ type: "ea_link_disconnected", ts: T, reason: "timeout" });
  assert.equal(d.reason, "timeout");
});

// ---- Node 4: full snapshot -------------------------------------------------------

test("parseExecutionSnapshot: all resources together", () => {
  const snap = parseExecutionSnapshot({
    timestamp: T,
    node3: { degraded: false, last_heartbeat: T - 1, latency_ms: 12, attempts: 990, errors: [] },
    work: {
      open_trades: 2, positions: 1, fills_attempted: 12, fills_confirmed: 10,
      fills_rejected: 1, fills_unknown: 1, cancels_sent: 1, cancels_accepted: 1,
    },
    open_trades: [{ id: "a", lifecycle: "filled", symbol: "XAUUSD", side: "buy", lots: 0.5 }],
    deriv_account: {
      connected: true, trading_disabled: true, can_trade: false, balance: 5000,
      margin: 0, pnl: 0, currency: "USD", leverage: 1000, timestamp: T,
    },
    mt5: {
      account: { configured: true, connected: true, authorized: true, account_type: "demo", login: 1 },
      positions: { positions: [], count: 0 },
      history: { deals: [], count: 0 },
      status: { configured: true, connected: true, orders_unknown: 1 },
    },
    recent_events: [{ type: "order_filled", ts: T, outcome: { status: "filled" } }],
  });
  assert.equal(snap.ok, true);
  assert.equal(snap.diagnostics.ok, true);
  assert.equal(snap.openTrades.trades.length, 1);
  assert.equal(snap.mt5.account.ok, true);
  assert.equal(snap.mt5.status.ordersUnknown, 1);
  assert.equal(snap.derivAccount.tradingDisabled, true);
  assert.equal(snap.recentEvents.length, 1);
  assert.equal(snap.recentEvents[0].type, "order_filled");
});

// ---- mock generator contract parity ---------------------------------------

test("mock feed: generated Node 1 payloads parse through the same parsers", () => {
  const candles = makeCandles(T, { count: 15 });
  const levels = makeLevels(T, 2655);
  const tickBars = makeTickBars(T);
  const strategy = makeStrategy(T, 2655);
  const demo = makeDemo(T, 2655);

  assert.equal(parseCandles(candles).bars.length, 15);
  assert.equal(parseLevels(levels).levels.length, 5);
  assert.ok(parseTickBars(tickBars).bars.length >= 8);
  assert.equal(parseStrategy(strategy).ok, true);
  assert.equal(parseDemo(demo).ok, true);
  assert.equal(parseStatus(makeStatus(T, candles)).ok, true);
  assert.equal(parseCalendar(makeCalendar(T)).count, 5);
});

test("mock feed: generated Node 4 payloads parse through the same parsers", () => {
  const diag = makeDiagnostics(T);
  const trades = makeOpenTrades(T);
  const account = makeMt5Account(T);
  const positions = makeMt5Positions(T);
  const history = makeMt5History(T);
  const status = makeMt5Status(T);
  const deriv = makeDerivAccount(T);
  const event = makeBridgeEvent(T, "order_filled");

  assert.equal(parseDiagnostics(diag).ok, true);
  assert.equal(parseOpenTrades(trades).trades.length, 3);
  assert.equal(parseMt5Account(account).ok, true);
  assert.equal(parseMt5Positions(positions).ok, true);
  assert.equal(parseMt5History(history).ok, true);
  assert.equal(parseMt5Status(status).ok, true);
  assert.equal(parseBridgeEvent(event).outcome.status, "filled");
  assert.equal(parseExecutionSnapshot({
    timestamp: T, node3: diag.node3, work: diag.work,
    open_trades: trades, deriv_account: deriv,
    mt5: { account, positions, history, status },
    recent_events: [event],
  }).ok, true);
});
