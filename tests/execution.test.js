"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

// ---------------------------------------------------------------------------
// DOM shim: every innerHTML write is counted, which is how "a second identical
// diagnostics frame produces no flicker" is proven.
// ---------------------------------------------------------------------------

const writes = { innerHTML: 0, textContent: 0 };

function element(id) {
  let markup = "";
  let text = "";
  return {
    id,
    hidden: true,
    dataset: {},
    attributes: {},
    classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
    get innerHTML() { return markup; },
    set innerHTML(value) { markup = String(value); writes.innerHTML += 1; },
    get textContent() { return text; },
    set textContent(value) { text = String(value); writes.textContent += 1; },
    setAttribute(name, value) { this.attributes[name] = String(value); },
    addEventListener() {},
    removeEventListener() {},
    focus() {},
    querySelector() { return null; },
  };
}

const ids = [
  "exec-feed-body", "exec-feed-mode", "exec-strategy-body", "exec-strategy-mode",
  "exec-venue-body", "exec-venue-mode", "exec-node3-body", "exec-node3-mode",
  "dry-run-banner", "dry-run-text",
];
const elements = new Map(ids.map((id) => [id, element(id)]));

global.window = {
  App: {},
  XAUUSD_CONFIG: { NODE4_HOST: "https://execution.example" },
};
global.document = {
  visibilityState: "visible",
  getElementById(id) { return elements.get(id) || null; },
  querySelectorAll() { return []; },
  addEventListener() {},
  removeEventListener() {},
};

require("../js/config.js");
require("../js/normalize.js");
require("../js/format.js");
require("../js/utils.js");
require("../js/api.js");
require("../js/execution.js");

const App = global.window.App;
const DASH = "\u2014";
const MINUS = "\u2212";

const body = (id) => elements.get(id).innerHTML;
const feedBody = () => body("exec-feed-body");
const strategyBody = () => body("exec-strategy-body");
const venueBody = () => body("exec-venue-body");
const node3Body = () => body("exec-node3-body");

const TS = 1792788752000;

function healthy(overrides = {}) {
  return {
    timestamp: TS,
    market_feed: {
      enabled: true,
      state: "live",
      trading_ready: true,
      not_ready_reason: null,
      url: "wss://feed.example/ws",
      auth: "none (contract v1)",
      contract_version: "1.0",
      contract_required: "1.0",
      symbol: "XAUUSD",
      interval: "15m",
      connected: true,
      last_frame_ms: TS - 500,
      last_status_ms: TS - 1000,
      last_candle_time: TS - 452000,
      last_closed_candle_time: TS - 1352000,
      last_levels_ts: TS - 152000,
      last_seq: 1841,
      session_closed_candles: 19,
      sync_complete: true,
      contract_ok: true,
      freshness_state: "live",
      freshness_age_ms: 900,
      market_open: true,
      reconnect_count: 1,
      frames_received: 2190,
      candles_received: 2160,
      closed_candles: 19,
      levels_received: 26,
      status_frames: 4,
      duplicates: 0,
      out_of_order: 0,
      gaps: 0,
      contract_violations: 0,
      resyncs: 0,
      last_error: null,
      last_event_ms: TS - 500,
    },
    strategy: {
      enabled: true,
      mode: "local",
      strategy_version: "vp_break_retest_v1",
      symbol: "XAUUSD",
      interval: "15m",
      volume_threshold: 10500,
      sl_min_pips: 200,
      sl_max_pips: 300,
      tp_min_pips: 600,
      tp_max_pips: 800,
      rr_min: 2,
      rr_max: 3,
      ttl_secs: 120,
      levels_tracked: 7,
      candles_observed: 19,
      last_candle_time: TS - 1352000,
      atr: 1.8,
      last_decision_at_ms: TS - 152000,
      last_action: "flat",
      last_trigger_level: null,
      last_intent_id: null,
      intents_emitted: 0,
      intents_suppressed: 0,
      last_suppression_reason: null,
      last_error: null,
      updated_at: TS - 152000,
      breaks_detected: 3,
      breaks_invalidated: 1,
      retests_rejected_low_volume: 5,
      signals_confirmed: 0,
    },
    work: {
      service: "node4-execution",
      version: "0.1.0",
      role: "execution_only",
      venue: "none",
      started_at: TS - 3600000,
      uptime_secs: 3600,
      intents_received: 0,
      intents_accepted: 0,
      intents_rejected: 0,
      intents_expired: 0,
      intents_duplicate: 0,
      orders_placed: 0,
      orders_filled: 0,
      orders_rejected: 0,
      orders_unknown: 0,
      last_intent_ts: null,
      last_error: null,
    },
    node3: {
      url: "",
      token_configured: false,
      protocol_version: 1,
      connected: false,
      authenticated: false,
      state: "starting",
      session_age_secs: null,
      last_message_ts: null,
      last_hello_ack_ts: null,
      last_report_ts: null,
      reconnect_count: 0,
      frames_received: 0,
      reports_sent: 0,
      reports_acked: 0,
      last_error: null,
    },
    ...overrides,
  };
}

const stale = () => healthy({
  timestamp: TS + 1000,
  market_feed: {
    ...healthy().market_feed,
    state: "live",                  // the trap: state stays live
    trading_ready: false,
    not_ready_reason: "Node 1 freshness is stale",
    freshness_state: "stale",
    freshness_age_ms: 94000,
    connected: true,
    contract_ok: true,
    sync_complete: true,
  },
  strategy: {
    ...healthy().strategy,
    intents_suppressed: 1,
    last_suppression_reason: "Node 1 freshness is stale",
  },
});

const monitor = () => healthy({
  timestamp: TS + 2000,
  strategy: {
    ...healthy().strategy,
    enabled: false,
    mode: "monitor",
    intents_emitted: 0,
  },
});

// ---------------------------------------------------------------------------

test("local + ready renders both cards from trading_ready", () => {
  assert.equal(App.execution.apply(healthy(), "rest"), true);

  assert.match(feedBody(), /exec-status" data-tone="live"/);
  assert.match(feedBody(), /Market data ready/);
  assert.doesNotMatch(feedBody(), /not ready/i);
  assert.equal(elements.get("exec-feed-mode").textContent, "LOCAL");
  assert.equal(elements.get("exec-strategy-mode").textContent, "LOCAL");
  assert.doesNotMatch(feedBody(), /monitor only/);
  assert.doesNotMatch(strategyBody(), /monitor only/);
  assert.match(feedBody(), /duplicates <b>0<\/b>/);
  assert.match(feedBody(), /contract violations <b>0<\/b>/);
  assert.match(feedBody(), /1\.0 \(requires 1\.0\)/);
  // The feed URL is operator information, never a browser-connectable link.
  assert.match(feedBody(), /wss:\/\/feed\.example\/ws/);
  assert.doesNotMatch(feedBody(), /<a\b/i);
  // The auth value is a label, never a credential.
  assert.match(feedBody(), /auth none \(contract v1\)/);

  assert.match(strategyBody(), /vp_break_retest_v1/);
  assert.match(strategyBody(), /XAUUSD/);
  assert.match(strategyBody(), /is-flat">FLAT/);
  assert.match(strategyBody(), /1\.80/);
  assert.match(strategyBody(), /breaks detected <b>3<\/b>/);
  assert.match(strategyBody(), /signals confirmed <b>0<\/b>/);
  assert.match(strategyBody(), /Intents emitted/);

  // venue "none" is a dry run: banner visible, no position implied.
  assert.equal(elements.get("dry-run-banner").hidden, false);
  assert.match(venueBody(), /Dry run/);
  assert.match(venueBody(), /no position exists/i);

  // Local mode never blames Node 3.
  assert.match(node3Body(), /Not used \u2014 local strategy/);
  assert.match(node3Body(), /exec-status" data-tone="muted"/);
  assert.doesNotMatch(node3Body(), /data-tone="bad"/);

  // No control of any kind reaches the DOM.
  for (const id of ["exec-feed-body", "exec-strategy-body", "exec-venue-body", "exec-node3-body"]) {
    const markup = body(id);
    assert.doesNotMatch(markup, /<(button|input|select|textarea|form)\b/i);
  }
});

test("the stale fixture is NOT ready even though state is live", () => {
  App.execution.apply(stale(), "ws");

  const markup = feedBody();
  assert.match(markup, /Market data not ready/);
  assert.doesNotMatch(markup, /Market data ready/);
  assert.match(markup, /data-tone="warn"/);
  assert.match(markup, /Node 1 freshness is stale/);
  // state is still surfaced as data, not as health colour.
  assert.match(markup, /Transport<\/dt><dd class="tnum ">live/);

  assert.match(strategyBody(), /Intents suppressed<\/span><b class="tnum is-warn">1/);
  assert.match(strategyBody(), /Last signal suppressed: Node 1 freshness is stale/);
});

test("colour follows trading_ready: red when disconnected or contract-broken", () => {
  const disconnected = healthy({
    timestamp: TS + 3000,
    market_feed: {
      ...healthy().market_feed,
      trading_ready: false,
      connected: false,
      state: "live",
      not_ready_reason: "feed socket is down",
    },
  });
  App.execution.apply(disconnected, "ws");
  assert.match(feedBody(), /data-tone="bad"/);
  assert.match(feedBody(), /feed socket is down/);

  const mismatch = healthy({
    timestamp: TS + 4000,
    market_feed: {
      ...healthy().market_feed,
      trading_ready: false,
      connected: true,
      state: "contract_mismatch",
      not_ready_reason: "contract_version 0 is not 1.0",
      contract_version: "0",
      contract_ok: false,
    },
  });
  App.execution.apply(mismatch, "ws");
  assert.match(feedBody(), /data-tone="bad"/);
  assert.match(feedBody(), /contract_version 0 is not 1\.0/);
});

test("feed integrity turns red on non-zero contract violations", () => {
  const violated = healthy({
    timestamp: TS + 5000,
    market_feed: {
      ...healthy().market_feed,
      trading_ready: false,
      state: "contract_violation",
      not_ready_reason: "forbidden field in a levels frame",
      duplicates: 2,
      gaps: 1,
      out_of_order: 3,
      contract_violations: 4,
      resyncs: 1,
    },
  });
  App.execution.apply(violated, "ws");
  const markup = feedBody();
  assert.match(markup, /integrity-chip is-bad" title="[^"]*">contract violations <b>4<\/b>/);
  assert.match(markup, /duplicates <b>2<\/b>/);
  assert.match(markup, /gaps <b>1<\/b>/);
  assert.match(markup, /out of order <b>3<\/b>/);
  assert.match(markup, /resyncs <b>1<\/b>/);
});

test("a disabled feed is grey, not an alarm", () => {
  App.execution.apply(healthy({
    timestamp: TS + 6000,
    market_feed: {
      ...healthy().market_feed,
      enabled: false,
      state: "disabled",
      trading_ready: false,
      connected: false,
      not_ready_reason: "NODE1_FEED_ENABLED is not set",
    },
  }), "ws");
  assert.match(feedBody(), /Feed disabled/);
  assert.match(feedBody(), /data-tone="muted"/);
  assert.match(feedBody(), /NODE1_FEED_ENABLED is not set/);
});

test("monitor mode labels both cards monitor only", () => {
  App.execution.apply(monitor(), "ws");
  assert.equal(elements.get("exec-feed-mode").textContent, "MONITOR ONLY");
  assert.equal(elements.get("exec-strategy-mode").textContent, "MONITOR ONLY");
  assert.equal(elements.get("exec-strategy-mode").dataset.tone, "warn");
  assert.match(feedBody(), /exec-flag">monitor only/);
  assert.match(strategyBody(), /exec-flag">monitor only/);
  // The gate can still read ready; it just cannot trade.
  assert.match(feedBody(), /Market data ready/);
});

test("missing market_feed / strategy renders unavailable and never throws", () => {
  assert.equal(App.execution.apply({ timestamp: TS + 7000, work: { venue: "none" } }, "ws"), true);
  assert.match(feedBody(), /Market feed state unavailable/);
  assert.match(feedBody(), /does not include market_feed/);
  assert.match(strategyBody(), /Local strategy state unavailable/);
  assert.match(strategyBody(), /does not include strategy/);
  assert.match(venueBody(), /Dry run/);
  assert.match(node3Body(), /Node 3 link unavailable/);
  assert.equal(elements.get("dry-run-banner").hidden, false);
});

test("an unknown or malformed payload is ignored instead of clearing the cards", () => {
  App.execution.apply(healthy(), "ws");
  const before = feedBody();
  assert.equal(App.execution.apply(null, "ws"), false);
  assert.equal(App.execution.apply([], "ws"), false);
  assert.equal(App.execution.apply({ nonsense: true }, "ws"), false);
  assert.equal(feedBody(), before);
});

test("diagnostics frames replace: counters reset and identical frames do not repaint", () => {
  App.execution.apply(healthy(), "ws");
  assert.equal(elements.get("exec-feed-mode").textContent, "LOCAL");
  assert.equal(elements.get("exec-strategy-mode").textContent, "LOCAL");
  assert.doesNotMatch(feedBody(), /monitor only/);
  assert.doesNotMatch(strategyBody(), /monitor only/);
  assert.match(feedBody(), /duplicates <b>0<\/b>/);

  const dirty = healthy({
    timestamp: TS + 8000,
    market_feed: { ...healthy().market_feed, duplicates: 9, gaps: 4, contract_violations: 2, trading_ready: false },
  });
  App.execution.apply(dirty, "ws");
  assert.match(feedBody(), /duplicates <b>9<\/b>/);
  assert.match(feedBody(), /integrity-chip is-bad/);

  // Same timestamp but cleared counters: a complete replacement, not a merge.
  const cleared = healthy({
    timestamp: TS + 9000,
    market_feed: { ...healthy().market_feed, duplicates: 0, gaps: 0, contract_violations: 0 },
  });
  App.execution.apply(cleared, "ws");
  assert.equal(elements.get("exec-feed-mode").textContent, "LOCAL");
  assert.equal(elements.get("exec-strategy-mode").textContent, "LOCAL");
  assert.doesNotMatch(feedBody(), /monitor only/);
  assert.doesNotMatch(strategyBody(), /monitor only/);
  assert.match(feedBody(), /duplicates <b>0<\/b>/);
  assert.doesNotMatch(feedBody(), /integrity-chip is-bad/);

  // A second identical frame changes nothing at all (no flicker, no new rows).
  const snapshotOfBodies = [feedBody(), strategyBody(), venueBody(), node3Body()];
  const beforeWrites = writes.innerHTML;
  App.execution.apply(cleared, "ws");
  assert.equal(writes.innerHTML, beforeWrites);
  assert.deepEqual([feedBody(), strategyBody(), venueBody(), node3Body()], snapshotOfBodies);
  assert.equal((feedBody().match(/integrity-chip/g) || []).length, 5);
});

test("REST bodies and WS envelopes with the same snapshot render identically", () => {
  const fixture = healthy({ timestamp: TS + 10000 });
  App.execution.apply(fixture, "rest");
  const fromRest = [feedBody(), strategyBody(), venueBody(), node3Body()];
  const beforeWrites = writes.innerHTML;

  App.execution.apply({ type: "diagnostics", data: fixture }, "ws");
  assert.deepEqual([feedBody(), strategyBody(), venueBody(), node3Body()], fromRest);
  assert.equal(writes.innerHTML, beforeWrites, "identical payload must not repaint");
  assert.match(feedBody(), /Market data ready/);
});

test("an older REST seed cannot overwrite a newer diagnostics frame", () => {
  const newer = healthy({ timestamp: TS + 20000 });
  App.execution.apply(newer, "ws");
  const before = feedBody();
  assert.equal(App.execution.apply(healthy({ timestamp: TS + 1000 }), "rest"), false);
  assert.equal(feedBody(), before);
});

test("duplicates rejected is neutral, never an error", () => {
  App.execution.apply(healthy({
    timestamp: TS + 30000,
    work: { ...healthy().work, venue: "mt5_demo", orders_placed: 0, intents_duplicate: 3 },
  }), "ws");
  const markup = venueBody();
  assert.match(markup, /Duplicates rejected<\/dt><dd class="tnum ">3/);
  assert.match(markup, /expected count after a reconnect, not an error/);
  assert.doesNotMatch(markup, /data-tone="bad"/);
  assert.equal(elements.get("dry-run-banner").hidden, true);
});

test("the non-local snapshot shows a real Node 3 link", () => {
  App.execution.apply(healthy({
    timestamp: TS + 40000,
    strategy: { ...healthy().strategy, mode: "node3", enabled: false },
    node3: {
      ...healthy().node3,
      url: "wss://node3.example/execution",
      connected: true,
      authenticated: true,
      state: "connected",
      last_message_ts: TS + 39000,
      reports_sent: 12,
      reports_acked: 12,
    },
  }), "ws");
  assert.match(node3Body(), /data-tone="live"/);
  assert.match(node3Body(), /Node 3 intent link|Connected/);
  assert.match(node3Body(), /reports sent|Reports sent/);
  assert.match(node3Body(), /wss:\/\/node3\.example\/execution/);
  assert.doesNotMatch(node3Body(), /Not used/);
});

// ---------------------------------------------------------------------------
// Transports
// ---------------------------------------------------------------------------

test("the initial GET /diagnostics seeds the cards over HTTP", async () => {
  const http = require("node:http");
  let seen = null;
  const server = http.createServer((request, response) => {
    seen = { method: request.method, url: request.url, authorization: request.headers.authorization };
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify(healthy({ timestamp: TS + 80000 })));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  const saved = App.config.endpoints.executionDiagnostics;
  App.config.endpoints.executionDiagnostics = `http://127.0.0.1:${port}/diagnostics`;
  try {
    assert.equal(await App.execution.load(), true);
    assert.equal(seen.method, "GET");
    assert.equal(seen.url, "/diagnostics");
    assert.equal(seen.authorization, undefined);
    assert.match(feedBody(), /Market data ready/);
  } finally {
    App.config.endpoints.executionDiagnostics = saved;
    server.close();
  }
});

// ---------------------------------------------------------------------------
// Live transport: the Node 4 socket subscribes to diagnostics only.
// ---------------------------------------------------------------------------

const sockets = [];

class FakeWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  constructor(url) {
    this.url = url;
    this.readyState = FakeWebSocket.CONNECTING;
    this.sent = [];
    sockets.push(this);
  }
  send(message) { this.sent.push(JSON.parse(message)); }
  open() {
    this.readyState = FakeWebSocket.OPEN;
    if (this.onopen) this.onopen();
  }
  receive(frame) {
    if (this.onmessage) this.onmessage({ data: typeof frame === "string" ? frame : JSON.stringify(frame) });
  }
  close() {
    this.readyState = FakeWebSocket.CLOSED;
    if (this.onclose) this.onclose();
  }
}

test("the diagnostics socket subscribes to diagnostics and renders its frames", () => {
  const originalWebSocket = global.WebSocket;
  const originalFetch = global.fetch;
  global.WebSocket = FakeWebSocket;
  global.fetch = async () => ({ ok: false, status: 503, json: async () => ({}) });
  try {
    App.execution.stop();
    sockets.length = 0;
    App.execution.start();
    assert.equal(sockets.length, 1);
    assert.equal(sockets[0].url, "wss://execution.example/ws");

    sockets[0].open();
    assert.deepEqual(sockets[0].sent[0], { type: "subscribe", topics: ["diagnostics"] });

    sockets[0].receive({ type: "heartbeat" });
    sockets[0].receive({ type: "diagnostics", data: healthy({ timestamp: TS + 50000 }) });
    assert.match(feedBody(), /Market data ready/);
    assert.equal(App.execution.getState().phase, "live");

    // A complete snapshot that clears readiness must reach the UI as-is.
    sockets[0].receive({ type: "diagnostics", data: stale() });
    assert.match(feedBody(), /Market data not ready/);
    assert.match(feedBody(), /Node 1 freshness is stale/);
  } finally {
    App.execution.stop();
    global.WebSocket = originalWebSocket;
    global.fetch = originalFetch;
  }
});

// ---------------------------------------------------------------------------
// Static read-only guarantees
// ---------------------------------------------------------------------------

test("the execution module is read-only: no controls, no writes, no credentials", () => {
  const source = fs.readFileSync(path.join(__dirname, "..", "js", "execution.js"), "utf8");
  assert.doesNotMatch(source, /<(button|input|select|textarea|form)\b/i);
  assert.doesNotMatch(source, /method\s*:\s*["'](POST|PUT|PATCH|DELETE)/i);
  assert.doesNotMatch(source, /\/mt5\/control/);
  assert.doesNotMatch(source, /Authorization/i);
  assert.doesNotMatch(source, /bearer\s+[A-Za-z0-9._-]{8,}/i);
});

test("the page ships the new sections and the dry-run banner", () => {
  const markup = fs.readFileSync(path.join(__dirname, "..", "index.html"), "utf8");
  for (const id of ["panel-exec-feed", "panel-exec-strategy", "panel-exec-venue", "panel-exec-node3", "dry-run-banner"]) {
    assert.match(markup, new RegExp(`id="${id}"`));
  }
  assert.match(markup, /js\/execution\.js/);
  // HTML text must carry real glyphs, not JavaScript escape sequences.
  assert.doesNotMatch(markup, /\\u[0-9a-f]{4}/i);
  assert.doesNotMatch(markup, /\\n/);
});

test("a missing market_feed with a real snapshot never renders the dash as a reason", () => {
  App.execution.apply({ timestamp: TS + 60000, market_feed: { enabled: true, trading_ready: false, connected: true } }, "ws");
  assert.match(feedBody(), /Market data not ready/);
  assert.match(feedBody(), /Node 4 reported no not_ready_reason/);
  assert.doesNotMatch(feedBody(), new RegExp(`exec-reason[^>]*>${DASH}`));
  assert.match(feedBody(), new RegExp(`requires ${DASH}`));
});

test("format helpers never leak raw nulls into the strategy card", () => {
  App.execution.apply(healthy({
    timestamp: TS + 70000,
    strategy: { ...healthy().strategy, atr: null, last_decision_at_ms: null, intents_emitted: null },
    market_feed: { ...healthy().market_feed, last_frame_ms: null },
  }), "ws");
  assert.doesNotMatch(strategyBody(), /null|NaN|undefined/);
  assert.doesNotMatch(feedBody(), /null|NaN|undefined/);
  assert.ok(strategyBody().includes(DASH));
  assert.ok(strategyBody().includes(MINUS) === false, "no negative sign invented for missing values");
});
