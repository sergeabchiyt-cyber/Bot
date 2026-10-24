// render.test.js — panel rendering with a headless DOM shim.
// Focus: lifecycle status rendering (accepted/unknown never shown as filled),
// MT5 venue state rules, strategy/demo/position panel states.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installDomShim, markupContains } from "./helpers/dom-shim.js";

const shim = installDomShim({ innerWidth: 1500 });

const { h } = await import("../public/src/render/dom.js");
const {
  renderOpenTrades, renderMt5Venue, renderDiagnostics,
} = await import("../public/src/render/execution.js");
const {
  renderStrategyPanel, renderDemoStrip, renderPositionCard, renderTradeHistory,
} = await import("../public/src/render/strategy.js");
const { renderMarketHeader } = await import("../public/src/render/market.js");
const { createStore } = await import("../public/src/state/store.js");

function findAll(node, pred, out = []) {
  if (!node) return out;
  if (node.nodeType === 1) {
    if (pred(node)) out.push(node);
    for (const c of node.children) findAll(c, pred, out);
  }
  return out;
}

function rowsOf(section) {
  // Data rows only (skip the thead row, which contains <th> children).
  return findAll(section, (el) => el.tagName === "TR"
    && el.children.some((c) => c.tagName === "TD"));
}

// ------------------------------------------------------------ lifecycle ------

test("open trades: accepted is rendered as Accepted — never Filled or confirmed", () => {
  const section = renderOpenTrades([{
    id: "a", lifecycle: "accepted", symbol: "XAUUSD", side: "sell", lots: 0.3,
    filledVolume: 0, entry: null, mark: 2653.5, openedAt: 1,
  }]);
  const row = rowsOf(section)[0];
  const markup = row.toMarkup();
  assert.match(markup, /lifecycle--warn/);
  assert.match(markup, /Accepted/);
  assert.doesNotMatch(markup, /Filled/);
  assert.doesNotMatch(markup, /chip--confirmed/);
  // The header row explains the pending state.
  assert.match(section.toMarkup(), /pending broker/);
});

test("open trades: unknown is rendered as Unknown — never Filled or confirmed", () => {
  const section = renderOpenTrades([{
    id: "b", lifecycle: "unknown", symbol: "XAUUSD", side: "buy", lots: 0.2,
    filledVolume: 0, entry: null, mark: 2653.5, openedAt: 1,
  }]);
  const row = rowsOf(section)[0];
  const markup = row.toMarkup();
  assert.match(markup, /lifecycle--bad/);
  assert.match(markup, /Unknown/);
  assert.doesNotMatch(markup, /Filled/);
  assert.doesNotMatch(markup, /chip--confirmed/);
  // Header surfaces the unknown count explicitly.
  assert.match(section.toMarkup(), /unknown/);
});

test("open trades: broker-confirmed fills get the confirmed chip", () => {
  const section = renderOpenTrades([
    {
      id: "f", lifecycle: "filled", symbol: "XAUUSD", side: "buy", lots: 0.5,
      filledVolume: 0.5, entry: 2651.2, mark: 2653.5, openedAt: 1,
      mt5: { matched: true, ticket: 77001 },
    },
    {
      id: "p", lifecycle: "partial", symbol: "XAUUSD", side: "buy", lots: 0.4,
      filledVolume: 0.15, entry: 2650, mark: 2652, openedAt: 1,
    },
  ]);
  const rows = rowsOf(section);
  assert.match(rows[0].toMarkup(), /Filled/);
  assert.match(rows[0].toMarkup(), /chip--confirmed/);
  assert.match(rows[1].toMarkup(), /Partial/);
  assert.match(rows[1].toMarkup(), /chip--confirmed/);
});

test("open trades: partial with zero filled volume is NOT confirmed", () => {
  const section = renderOpenTrades([{
    id: "z", lifecycle: "partial", symbol: "XAUUSD", side: "buy", lots: 0.4,
    filledVolume: 0, entry: 2650, mark: 2652, openedAt: 1,
  }]);
  const markup = rowsOf(section)[0].toMarkup();
  assert.match(markup, /Partial/);
  assert.doesNotMatch(markup, /chip--confirmed/);
});

test("open trades: unmatched MT5 trades show the unmatched state, not a fill", () => {
  const section = renderOpenTrades([{
    id: "m", lifecycle: "accepted", symbol: "XAUUSD", side: "buy", lots: 0.2,
    filledVolume: 0, entry: null, mark: 2653.5, openedAt: 1,
    mt5: { matched: false, status: "unmatched" },
  }]);
  const markup = rowsOf(section)[0].toMarkup();
  assert.match(markup, /MT5 unmatched/);
  assert.doesNotMatch(markup, /MT5 #/);
});

// ---------------------------------------------------------------- venue -------

test("venue: configured=false shows venue OFF", () => {
  const venue = renderMt5Venue({ configured: false }, null);
  assert.match(venue.toMarkup(), /venue OFF/);
});

test("venue: connected=false shows the setup hint", () => {
  // Render functions consume the store's normalized resources
  // (parseMt5Account output), not raw snake_case payloads.
  const venue = renderMt5Venue({
    configured: true, connected: false, setupHint: "run mt5 bridge", accountType: "demo",
  }, null);
  assert.match(venue.toMarkup(), /setup: run mt5 bridge/);
});

test("venue: authorized=false warns cannot trade", () => {
  const venue = renderMt5Venue({
    configured: true, connected: true, authorized: false, accountType: "demo",
  }, null);
  assert.match(venue.toMarkup(), /cannot trade/);
});

test("venue: non-demo account_type is flagged red", () => {
  const venue = renderMt5Venue({
    configured: true, connected: true, authorized: true, accountType: "live",
  }, null);
  const markup = venue.toMarkup();
  assert.match(markup, /account_type=live/);
  assert.match(markup, /chip--bad/);
});

test("venue: demo account is marked", () => {
  const venue = renderMt5Venue({
    configured: true, connected: true, authorized: true, accountType: "demo",
  }, null);
  assert.match(venue.toMarkup(), /demo account/);
});

test("venue: halted shows the kill switch", () => {
  const venue = renderMt5Venue({
    configured: true, connected: true, authorized: true, accountType: "demo", halted: true,
  }, null);
  assert.match(venue.toMarkup(), /KILL SWITCH ENGAGED/);
});

test("venue: stale heartbeat is flagged", () => {
  const venue = renderMt5Venue({
    configured: true, connected: true, authorized: true, accountType: "demo",
    lastHeartbeat: Date.now() - 60000, // 60s old
  }, null);
  assert.match(venue.toMarkup(), /stale/);
});

test("venue: fresh heartbeat is not flagged", () => {
  const venue = renderMt5Venue({
    configured: true, connected: true, authorized: true, accountType: "demo",
    lastHeartbeat: Date.now() - 1000,
  }, null);
  assert.doesNotMatch(venue.toMarkup(), /stale/);
});

test("venue: missing account shows unknown, not off", () => {
  const venue = renderMt5Venue(null, null);
  assert.match(venue.toMarkup(), /venue unknown/);
});

// ------------------------------------------------------------- strategy -------

test("strategy panel: renders decision, confidence gauge with 60% tick", () => {
  const store = createStore();
  store.seedNode1Rest({
    strategy: {
      symbol: "XAUUSD", enabled: true,
      last: {
        ts: 1, symbol: "XAUUSD", action: "buy", score: 63, confidence: 71.4,
        price: 2661.99, atr: 2.94, regime: "range", volatility: "normal",
        stop_loss: 2657.74, take_profit: 2670.12, reward_risk: 1.8,
        lots: 1.51, risk_usd: 50,
        reasons: [
          { name: "a", value: "v1", weight: 0.35, contribution: 0.1, detail: "d1" },
          { name: "b", value: "v2", weight: 0.3, contribution: 0.5, detail: "d2" },
        ],
        notes: ["demo note"], next_event_ts: 1,
      },
    },
  });
  const panel = renderStrategyPanel(store);
  const markup = panel.toMarkup();
  assert.match(markup, /action-badge--good/);
  assert.match(markup, /buy/);
  assert.match(markup, /71\.4%/);
  assert.match(markup, /left:60%/); // confidence threshold tick
  // reasons sorted by |contribution|: "b" first
  const reasonNodes = findAll(panel, (el) => el.className && el.className.includes("reason__name"));
  assert.equal(reasonNodes.length, 2);
  assert.equal(reasonNodes[0].textContent, "b");
  assert.match(markup, /demo note/);
});

test("strategy panel: unavailable endpoint degrades to a disabled state", () => {
  const store = createStore();
  store.setEndpointError("node1:strategy", "not available on this engine");
  const panel = renderStrategyPanel(store);
  assert.match(panel.toMarkup(), /unavailable/);
});

test("strategy panel: no data yet shows waiting state", () => {
  const store = createStore();
  const panel = renderStrategyPanel(store);
  assert.match(panel.toMarkup(), /Waiting for the first strategy decision/);
});

// --------------------------------------------------------------- demo ---------

test("demo strip: renders DEMO badge, equity and margin level", () => {
  const store = createStore();
  store.seedNode1Rest({
    demo: {
      enabled: true, mode: "demo", symbol: "XAUUSD", leverage: 1000,
      quote: { bid: 1, ask: 2, mid: 1.5, spread: 0.1, ts: 1 },
      account: {
        currency: "USD", balance: 10000, equity: 10201.66, free_margin: 10191,
        margin_level: 99904.51, unrealized_pnl: 201.66,
      },
      params: {}, ts: 1,
    },
  });
  const panel = renderDemoStrip(store);
  const markup = panel.toMarkup();
  assert.match(markup, /DEMO 1000/);
  assert.match(markup, /10,201\.66/);
  assert.match(markup, /99904\.5%/);
  assert.match(markup, /\+201\.66/);
});

test("demo strip: unavailable snapshot degrades", () => {
  const store = createStore();
  const panel = renderDemoStrip(store);
  assert.match(panel.toMarkup(), /unavailable/);
});

test("position card: open position renders SL/TP and progress", () => {
  const store = createStore();
  store.seedNode1Rest({
    demo: {
      enabled: true, mode: "demo",
      position: {
        id: "pos-1", side: "buy", lots: 0.75, entry: 2655.12, stop_loss: 2644,
        take_profit: 2675, opened_at: 1, risk_usd: 50, unrealized_pnl: 100,
      },
      account: {}, params: {}, ts: 1,
    },
  });
  const panel = renderPositionCard(store);
  const markup = panel.toMarkup();
  assert.match(markup, /2,655\.12/);
  assert.match(markup, /2,644\.00/);
  assert.match(markup, /2,675\.00/);
  assert.match(markup, /progress-fill/);
});

test("position card: no position shows the empty state and the read-only note", () => {
  const store = createStore();
  store.seedNode1Rest({ demo: { enabled: true, position: null, account: {} } });
  const panel = renderPositionCard(store);
  const markup = panel.toMarkup();
  assert.match(markup, /No open demo position/);
  assert.match(markup, /never opens positions/);
});

// ------------------------------------------------------------ header ---------

test("market header: freshness pill reflects connection state", () => {
  const store = createStore();
  store.setConn("node1", { state: "disconnected", lastMsgTs: 0 });
  const header = renderMarketHeader(store);
  assert.match(header.toMarkup(), /Disconnected/);
  assert.match(header.toMarkup(), /DEMO/);
});

test("market header: stale candles badge", () => {
  const store = createStore();
  store.setConn("node1", { state: "open", lastMsgTs: Date.now() });
  store.seedNode1Rest({
    status: { candles: { interval: "15m", edge_lag_minutes: 90 } },
    candles: [{ time: Math.floor(Date.now() / 1000) - 100, open: 1, high: 2, low: 0.5, close: 1.5 }],
  });
  const header = renderMarketHeader(store);
  assert.match(header.toMarkup(), /Stale/);
});

test("diagnostics panel: UNKNOWN fills counter is surfaced and flagged", () => {
  const panel = renderDiagnostics({
    timestamp: 1,
    node3: { degraded: false, latency_ms: 5, attempts: 1, errors: [] },
    work: {
      open_trades: 2, positions: 1, fills_attempted: 12, fills_confirmed: 10,
      fills_rejected: 1, fills_unknown: 1, cancels_sent: 0, cancels_accepted: 0,
    },
    ledger: { version: 1, entries: [] },
  });
  const markup = panel.toMarkup();
  assert.match(markup, /fills UNKNOWN/);
  assert.match(markup, /1/);
});
