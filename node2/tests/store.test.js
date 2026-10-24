// store.test.js — ts-keyed upsert semantics, caps, REST seeds, conn state.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createStore, candleBars, latestClose } from "../public/src/state/store.js";

function candle(time, close) {
  return { time, open: close, high: close + 1, low: close - 1, close, volume: 10 };
}

test("store: candles are upserted by time (replay does not duplicate)", () => {
  const store = createStore();
  store.applyNode1Frame({ type: "candle", frames: [candle(100, 1), candle(200, 2)] });
  store.applyNode1Frame({ type: "candle", frames: [candle(200, 2.5), candle(300, 3)] });

  const bars = candleBars(store);
  assert.equal(bars.length, 3);
  assert.equal(bars[1].close, 2.5); // updated close for the same ts
});

test("store: candle cap keeps memory bounded and drops oldest", () => {
  const store = createStore();
  for (let t = 1; t <= 2100; t++) {
    store.applyNode1Frame({ type: "candle", frames: [candle(t * 900, t)] });
  }
  const bars = candleBars(store);
  assert.equal(bars.length, 2000);
  assert.equal(bars[0].time, 101 * 900); // oldest dropped
  assert.equal(bars[bars.length - 1].time, 2100 * 900);
});

test("store: latestClose returns newest bar", () => {
  const store = createStore();
  assert.equal(latestClose(store), null);
  store.applyNode1Frame({ type: "candle", frames: [candle(900, 2661.99)] });
  assert.equal(latestClose(store), 2661.99);
});

test("store: strategy history keyed by ts, last updated", () => {
  const store = createStore();
  store.applyNode1Frame({ type: "strategy", frames: [{ ts: 100, symbol: "XAUUSD", action: "buy" }] });
  store.applyNode1Frame({ type: "strategy", frames: [{ ts: 100, symbol: "XAUUSD", action: "sell" }] });
  store.applyNode1Frame({ type: "strategy", frames: [{ ts: 200, symbol: "XAUUSD", action: "flat" }] });

  assert.equal(store.state.strategy.last.ts, 200);
  assert.equal(store.state.strategy.history.size, 2);
  assert.equal(store.state.strategy.history.get(100).action, "sell");
});

test("store: strategy history cap", () => {
  const store = createStore();
  for (let t = 1; t <= 120; t++) {
    store.applyNode1Frame({ type: "strategy", frames: [{ ts: t * 60, symbol: "XAUUSD", action: "buy" }] });
  }
  assert.equal(store.state.strategy.history.size, 100);
});

test("store: conn state transitions via setConn", () => {
  const store = createStore();
  assert.equal(store.state.conn.node1.state, "connecting");
  store.setConn("node1", { state: "open", lastMsgTs: 123 });
  assert.equal(store.state.conn.node1.state, "open");
  assert.equal(store.state.conn.node1.lastMsgTs, 123);
});

test("store: endpoint errors are tracked and cleared", () => {
  const store = createStore();
  const events = [];
  store.subscribe(() => events.push("change"));
  store.setEndpointError("node1:strategy", "not available on this engine");
  store.setEndpointError("node1:strategy", "not available on this engine"); // no-op
  assert.equal(events.length, 1);
  store.clearEndpointError("node1:strategy");
  assert.equal(store.state.errors["node1:strategy"], undefined);
});

test("store: node1 REST seed populates all panels", () => {
  const store = createStore();
  store.seedNode1Rest({
    status: { version: "0.1.0", candles: { interval: "15m" } },
    levels: [{ window: "PT", poc: 1, vah: 2, val: 0.5, timestamp: 1 }],
    candles: [candle(900, 10)],
    tick_volume: [{ time: 900, ticks: 10 }],
    calendar: { count: 1, events: [{ event: "CPI", impact: "High" }] },
    strategy: { symbol: "XAUUSD", last: { ts: 1, action: "buy" } },
    demo: { enabled: true, symbol: "XAUUSD" },
  });
  assert.equal(store.state.seeds.node1, true);
  assert.equal(store.state.market.candles.get(900).close, 10);
  assert.equal(store.state.market.levels.length, 1);
  assert.equal(store.state.strategy.last.action, "buy");
  assert.equal(store.state.demo.enabled, true);
});

test("store: node4 REST seed populates all resources", () => {
  const store = createStore();
  store.seedNode4Rest({
    diagnostics: { timestamp: 1, node3: {}, work: {} },
    open_trades: [{ id: "a", lifecycle: "filled", filled_volume: 0.5 }],
    mt5_account: { configured: true, connected: true, authorized: true },
    mt5_positions: { positions: [], count: 0 },
    mt5_history: { deals: [], count: 0 },
    mt5_status: { configured: true, orders_unknown: 1 },
  });
  assert.equal(store.state.seeds.node4, true);
  assert.equal(store.state.node4.openTrades[0].id, "a");
  assert.equal(store.state.node4.mt5.account.configured, true);
  assert.equal(store.state.node4.mt5.status.ordersUnknown, 1);
});

test("store: node4 frames dispatch per topic", () => {
  const store = createStore();
  store.applyNode4Frame({ topic: "diagnostics", diagnostics: { timestamp: 5, node3: {}, work: {} } });
  store.applyNode4Frame({ topic: "open_trades", open_trades: [{ id: "x", lifecycle: "accepted" }] });
  store.applyNode4Frame({ topic: "mt5_account", mt5_account: { configured: false } });
  store.applyNode4Frame({ topic: "bridge_status", mt5_status: { configured: true, orders_unknown: 2 } });
  store.applyNode4Frame({
    topic: "bridge_event",
    bridge_event: { type: "order_unknown", ts: 5, outcome: { status: "unknown" } },
  });

  assert.equal(store.state.node4.diagnostics.timestamp, 5);
  assert.equal(store.state.node4.openTrades[0].lifecycle, "accepted");
  assert.equal(store.state.node4.mt5.account.configured, false);
  assert.equal(store.state.node4.mt5.status.ordersUnknown, 2);
  assert.equal(store.state.node4.events[0].type, "order_unknown");
  assert.equal(store.state.node4.events[0].outcome.status, "unknown");
});

test("store: malformed frames are ignored, never crash", () => {
  const store = createStore();
  for (const frame of [null, undefined, "garbage", { noTopic: true }, { topic: "nope", data: 1 }]) {
    store.applyNode4Frame(frame);
  }
  for (const frame of [null, undefined, "garbage", { noType: true }]) {
    store.applyNode1Frame(frame);
  }
  assert.equal(store.state.node4.diagnostics, null);
});
