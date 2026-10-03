/* ============================================================
 * Frontend tests — Node 3 execution diagnostics dashboard
 *
 * Runs with plain Node (no dependencies or browser):
 *   node tests/execution.test.js
 * ============================================================ */
"use strict";

const path = require("path");

function makeEl() {
  return {
    innerHTML: "",
    textContent: "",
    title: "",
    dataset: {},
  };
}

const elements = {
  "execution-body": makeEl(),
  "execution-count": makeEl(),
  "deriv-pill": makeEl(),
  "deriv-balance-value": makeEl(),
};

global.window = global;
global.document = {
  getElementById(id) {
    return elements[id] || null;
  },
};

require(path.join(__dirname, "..", "js", "config.js"));
require(path.join(__dirname, "..", "js", "utils.js"));
require(path.join(__dirname, "..", "js", "execution.js"));

const App = window.App;
let passed = 0;
let failed = 0;

function assert(condition, message) {
  if (!condition) throw new Error(message || "assertion failed");
}

function test(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`ok   - ${name}`);
  } catch (error) {
    failed += 1;
    console.error(`FAIL - ${name}\n       ${error && error.message}`);
  }
}

const now = Date.now();
const snapshot = {
  timestamp: now,
  scanning: {
    active_setups_count: 1,
    total_levels_tracked: 2,
    volume_threshold: 10500,
    current_volume: 11240,
    last_price: 2650.35,
    atr_pips: 285,
    setups: [
      {
        id: "PW_VAH",
        name: "Scanned level",
        level_price: 2652,
        state: "scanning_break",
        distance_pips: 18,
        retest_zone_low: 2651.5,
        retest_zone_high: 2652.5,
        current_volume: 9000,
        volume_required: 10500,
        projected_buy: { side: "buy", entry: 2652, sl: 2649.5, tp: 2658.5, rr: 2.6 },
      },
      {
        id: "PW_POC",
        name: "Armed level",
        level_price: 2650,
        state: "broken_above",
        pending_side: "buy",
        distance_pips: 35,
        retest_zone_low: 2649.5,
        retest_zone_high: 2650.5,
        volume_required: 10500,
        current_volume: 11240,
        volume_confirmed: true,
        active_order: { side: "buy", entry: 2650.35, sl: 2647.85, tp: 2656.85, rr: 2.6 },
      },
    ],
  },
  open_trades: {
    total_open_count: 2,
    deriv_open_trades: [{
      contract_id: "246813579",
      display_symbol: "XAUUSD",
      contract_type: "CALL",
      side: "buy",
      buy_price: 10,
      bid_price: 14.2,
      current_spot: 2651.1,
      profit: 4.2,
      profit_pct: 42,
      status: "open",
    }],
    node3_open_trades: [{
      trade_id: "strategy-1",
      symbol: "XAUUSD",
      side: "buy",
      size: 0.01,
      entry: 2650.25,
      sl: 2647.75,
      tp: 2656.75,
      level_name: "PW PoC",
      rr: 2.6,
      unrealized_pnl: 0.85,
      current_price: 2651.1,
    }],
  },
  deriv_account: {
    configured: true,
    connected: true,
    authorized: true,
    account_id: "VRTC1234567",
    balance: 10004.2,
    currency: "USD",
    open_trades_count: 1,
    total_open_stake: 10,
    total_unrealized_pnl: 4.2,
    open_trades: [],
  },
  work: {
    node1_connected: true,
    node1_state: "connected",
    atr_pips: 285,
    current_sl_pips: 243,
    current_tp_pips: 686,
    current_rr: 2.82,
    volume_ratio: 1.07,
    breaks_detected: 3,
    breaks_invalidated: 1,
    retests_rejected_low_volume: 2,
    signals_confirmed: 1,
    trades_executed: 1,
    trades_failed: 0,
  },
  recent_events: [{
    timestamp: now,
    level: "signal",
    category: "execution",
    message: "Executed BUY XAUUSD",
  }],
};

test("config exposes Node 3 WS and REST endpoints", () => {
  assert(App.config.endpoints.executionWs.endsWith("/ws"), "execution WebSocket endpoint missing");
  assert(App.config.endpoints.executionDiagnostics.endsWith("/diagnostics"), "diagnostics endpoint missing");
  assert(App.config.endpoints.executionScanning.endsWith("/scanning"), "scanning endpoint missing");
  assert(App.config.endpoints.executionOpenTrades.endsWith("/open-trades"), "open trades endpoint missing");
  assert(App.config.endpoints.executionDeriv.endsWith("/deriv"), "Deriv endpoint missing");
});

test("diagnostics render account, open trades, scanner, work metrics and activity", () => {
  App.execution.applyDiagnostics(snapshot);
  const html = elements["execution-body"].innerHTML;
  assert(html.includes("AUTHORIZED"), "Deriv authorization status not rendered");
  assert(html.includes("VRTC1234567"), "account id not rendered");
  assert(html.includes("$10,004.20"), "account balance not rendered");
  assert(html.includes("DERIV DEMO CONTRACTS"), "Deriv contracts section missing");
  assert(html.includes("NODE 3 STRATEGY TRADES"), "strategy trades section missing");
  assert(html.includes("Candle Volume"), "volume diagnostic missing");
  assert(html.includes("Retests Rejected"), "retest counter missing");
  assert(html.includes("Executed BUY XAUUSD"), "activity event missing");
  assert(elements["execution-count"].textContent === "3", "tab count should include two trades and one armed setup");
  assert(elements["deriv-balance-value"].textContent === "$10,004.20", "topbar balance should be populated");
});

test("armed scanner setups sort ahead of nearby unarmed levels", () => {
  const html = elements["execution-body"].innerHTML;
  const armedPosition = html.indexOf("Armed level");
  const unarmedPosition = html.indexOf("Scanned level");
  assert(armedPosition !== -1 && unarmedPosition !== -1, "expected both scanner levels");
  assert(armedPosition < unarmedPosition, "armed setup should be rendered first");
  assert(html.includes("Retest"), "retest zone not rendered");
  assert(html.includes("WAITING") && html.includes("CONFIRMED"), "volume confirmation status missing");
  assert(html.includes("2.60R"), "projected reward/risk missing");
});

test("diagnostic event frames update the live activity log safely", () => {
  App.execution.applyFrame({
    type: "diagnostic_event",
    data: { timestamp: now + 1000, category: "execution", message: "Opened follow-up trade" },
  });
  assert(elements["execution-body"].innerHTML.includes("Opened follow-up trade"), "live event was not appended");
});

test("untrusted account and event text is HTML escaped", () => {
  App.execution.applyDiagnostics({
    deriv_account: { configured: true, account_id: '<img src=x onerror="x">' },
    open_trades: {
      deriv_open_trades: [{ side: '<script>x</script>', contract_type: '<img src=x>', contract_id: "xss" }],
      node3_open_trades: [],
    },
    recent_events: [{ timestamp: now, category: "test", message: "<script>alert(1)</script>" }],
  });
  const html = elements["execution-body"].innerHTML;
  assert(!html.includes('<img src=x'), "account id was not escaped");
  assert(!html.includes("<script>alert(1)</script>"), "activity message was not escaped");
  assert(html.includes("&lt;script&gt;alert(1)&lt;/script&gt;"), "escaped activity text missing");
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed) process.exitCode = 1;
