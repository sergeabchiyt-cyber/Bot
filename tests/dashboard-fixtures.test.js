"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

function element() {
  return {
    innerHTML: "",
    textContent: "",
    title: "",
    dataset: {},
    attributes: {},
    querySelector(selector) {
      return selector === ".service-status-value" ? this.statusValue : null;
    },
    setAttribute(name, value) { this.attributes[name] = String(value); },
    statusValue: { textContent: "" },
  };
}

const ids = [
  "strategy-body", "strategy-count", "execution-body", "execution-count", "mt5-body", "mt5-count",
  "market-service-badge", "strategy-service-badge", "execution-service-badge",
];
const elements = new Map(ids.map((id) => [id, element()]));
global.window = { App: {} };
global.document = {
  visibilityState: "visible",
  getElementById(id) { return elements.get(id) || null; },
  querySelectorAll() { return []; },
  addEventListener() {},
  removeEventListener() {},
};

require("../js/config.js");
require("../js/normalize.js");
require("../js/utils.js");
require("../js/stream.js");
require("../js/strategy.js");
require("../js/mt5.js");
require("../js/execution.js");

const App = global.window.App;
const now = Date.now();
const intent = {
  schema_version: 1,
  intent_id: "n3-fixture-buy-pw-poc",
  strategy: "vp_break_retest_v1",
  symbol: "XAUUSD",
  side: "buy",
  order_type: "market",
  reference_price: 2650.25,
  stop_loss: 2647.45,
  take_profit: 2656.25,
  risk_reward: 2.14,
  level_name: "PW PoC",
  source_candle_time: now - 1000,
  created_at: now - 500,
  expires_at: now + 60000,
};
const projectionBuy = {
  side: "buy", entry: 2650.25, stop_loss: 2647.45, take_profit: 2656.25,
  sl_pips: 280, tp_pips: 600, risk_reward: 2.14, sizing: "node4_managed",
};
const projectionSell = {
  side: "sell", entry: 2649.75, stop_loss: 2652.5, take_profit: 2643.75,
  sl_pips: 275, tp_pips: 600, risk_reward: 2.18, sizing: "node4_managed",
};
const report = (status, timestamp, extra = {}) => ({
  schema_version: 1,
  intent_id: intent.intent_id,
  status,
  venue: "deriv_mt5_demo",
  symbol: "XAUUSD",
  side: "buy",
  timestamp,
  execution_id: status === "filled" ? "mt5-deal-987654321" : undefined,
  filled_price: status === "filled" ? 2650.28 : undefined,
  quantity: status === "filled" ? 0.01 : undefined,
  quantity_unit: status === "filled" ? "lots" : undefined,
  ...extra,
});

const strategyDiagnostics = {
  timestamp: now,
  scanning: {
    active_setups_count: 1,
    total_levels_tracked: 5,
    proximity_dollars: 0.5,
    invalidation_dollars: 2,
    volume_threshold: 10500,
    current_volume: 11000,
    volume_confirmed: true,
    last_price: 2650.25,
    atr: 3.1,
    atr_pips: 310,
    setups: [{
      id: "PW-PoC",
      name: "PW PoC",
      window: "PW",
      level_price: 2649.9,
      state: "broken_above",
      state_label: "Broken above · retest pending",
      pending_side: "buy",
      current_price: 2650.25,
      distance_dollars: 0.35,
      distance_pips: 35,
      retest_zone_low: 2649.4,
      retest_zone_high: 2650.4,
      invalidation_price: 2647.9,
      volume_required: 10500,
      current_volume: 11000,
      volume_confirmed: true,
      projected_buy: projectionBuy,
      projected_sell: projectionSell,
      active_projection: projectionBuy,
      broken_at: now - 3000,
      broken_price: 2650.1,
      last_note: "Retest remains valid",
    }],
    timestamp: now,
  },
  signals: {
    pending: [intent],
    recent: [intent],
    execution_reports: [
      report("accepted", now - 4000),
      report("filled", now - 3000),
      report("rejected", now - 2000, { error_code: "broker_rejected", error_message: "fixture reject" }),
      report("unknown", now - 1000, { error_code: "outcome_unclear", error_message: "Reconcile <script>not safe</script>" }),
    ],
    pending_count: 1,
    node4_connected: true,
    node4_token_configured: true,
    timestamp: now,
  },
  work: {
    service: "xauusd-node3-strategy",
    version: "1.0.0",
    role: "strategy_only",
    started_at: now - 60000,
    uptime_secs: 60,
    node1_ws_url: "wss://market.example/ws",
    node1_connected: true,
    node1_state: "connected",
    node1_reconnect_count: 2,
    last_node1_msg_ts: now - 1000,
    last_candle_ts: now - 15000,
    last_levels_ts: now - 30000,
    candles_received: 40,
    levels_received: 12,
    ws_messages_received: 52,
    candle_buffer_len: 30,
    candle_buffer_capacity: 30,
    last_price: 2650.25,
    last_candle: { time: now - 15000, open: 2650, high: 2651, low: 2649, close: 2650.25, volume: 11000, source: "fixture" },
    atr: 3.1,
    atr_pips: 310,
    current_sl_pips: 280,
    current_tp_pips: 600,
    current_rr: 2.14,
    volume_threshold: 10500,
    last_candle_volume: 11000,
    volume_ratio: 1.05,
    sl_min_pips: 200,
    sl_max_pips: 300,
    tp_min_pips: 600,
    tp_max_pips: 800,
    rr_min: 2,
    rr_max: 3,
    breaks_detected: 2,
    breaks_invalidated: 0,
    retests_rejected_low_volume: 1,
    signals_confirmed: 1,
    intents_emitted: 1,
    intents_dropped: 0,
    execution_reports_received: 4,
    last_signal_ts: now - 500,
    last_execution_report_ts: now - 1000,
    last_error: null,
    public_ws_clients_connected: 1,
    node4_connected: true,
    node4_token_configured: true,
  },
  recent_events: [
    { timestamp: now - 1000, level: "signal", category: "strategy", message: "Emitted buy intent from PW PoC" },
    { timestamp: now - 2000, level: "info", category: "node1", message: "Node 1 stream: connected" },
    { timestamp: now - 3000, level: "info", category: "node4", message: "Node 4 link connected" },
  ],
};

const trade = {
  trade_id: "trade-mt5-987654321",
  symbol: "XAUUSD",
  side: "buy",
  size: 0.01,
  entry: 2650.28,
  sl: 2647.45,
  tp: 2656.25,
  status: "open",
  timestamp: now - 2000,
  intent_id: intent.intent_id,
  level_name: "PW PoC",
  venue: "deriv_mt5_demo",
  rr: 2.14,
  current_price: 2650.4,
  unrealized_pnl: 0,
  closed_at: null,
};
const contract = {
  contract_id: "deriv-contract-123",
  symbol: "frxXAUUSD",
  display_symbol: "XAUUSD",
  contract_type: "CALL",
  side: "buy",
  buy_price: 0.5,
  bid_price: 0.61,
  payout: 0.95,
  entry_spot: 2650.1,
  current_spot: 2650.5,
  barrier: null,
  profit: 0.11,
  profit_pct: 22,
  currency: "USD",
  date_start: now - 10000,
  date_expiry: now + 50000,
  status: "open",
  longcode: "fixture Deriv contract",
};
const position = {
  ticket: 123456789,
  symbol: "XAUUSD",
  side: "buy",
  volume: 0.01,
  price_open: 2650.28,
  sl: 2647.45,
  tp: 2656.25,
  profit: 0,
  swap: 0,
  comment: "n3-fixture-buy-pw-poc",
  magic: 330033,
  time_ms: now - 2000,
  current_price: 2650.4,
  unrealized_pnl: 0,
};
const account = {
  configured: true,
  connected: true,
  authorized: true,
  account_type: "demo",
  login: 123456,
  server: "Deriv-Demo",
  company: "Deriv",
  currency: "USD",
  balance: 10000,
  equity: 10000,
  margin: 0,
  margin_free: 10000,
  leverage: 100,
  trade_allowed: true,
  halted: false,
  halt_reason: null,
  requested_symbol: "XAUUSD",
  broker_symbol: "XAUUSD",
  symbol_digits: 2,
  symbol_volume_min: 0.01,
  symbol_volume_max: 100,
  symbol_volume_step: 0.01,
  symbol_contract_size: 100,
  terminal_build: 4755,
  ea_version: "1.0.0",
  bridge_version: "0.1.0",
  latency_ms: 42,
  last_heartbeat: now - 1000,
  last_updated: now - 1000,
  error: null,
  setup_hint: null,
};
const positions = {
  positions: [position], count: 1, total_volume: 0.01, total_unrealized_pnl: 0,
  halted: false, halt_reason: null, account_login: 123456, account_type: "demo",
  source: "bridge", timestamp: now - 500,
};
const history = {
  deals: [
    { ticket: 987654320, order_ticket: 123456790, position_ticket: 123456789, symbol: "XAUUSD", side: "buy", volume: 0.01, price: 2650.28, profit: 0, swap: 0, commission: 0, comment: intent.intent_id, magic: 330033, time_ms: now - 3000, entry: "in", reason: "expert" },
    { ticket: 987654321, order_ticket: 123456791, position_ticket: 123456789, symbol: "XAUUSD", side: "sell", volume: 0.01, price: 2656.02, profit: 5.9, swap: 0, commission: -0.07, comment: intent.intent_id, magic: 330033, time_ms: now - 1000, entry: "out", reason: "tp" },
  ],
  count: 2, cursor: null, complete: true, total_realized_pnl: 5.83,
  first_ms: now - 3000, last_ms: now - 1000, source: "bridge", timestamp: now,
};
const bridgeStatus = {
  configured: true, connected: true, authorized: true, protocol: 1, bridge_version: "0.1.0",
  node4_url: "wss://execution.example/ws", ea_connected: true, ea_write_enabled: true,
  ea_mode: "demo", ea_login: 123456, ea_server: "Deriv-Demo", ea_last_heartbeat: now - 1000,
  halted: false, halt_reason: null, trading_enabled: true,
  orders_sent: 1, orders_filled: 1, orders_rejected: 0, orders_unknown: 0,
  positions_open: 1, history_deals: 2, last_error: null, uptime_secs: 3600, timestamp: now,
};

const node4Diagnostics = {
  timestamp: now,
  node3: {
    url: "wss://strategy.example/private-link", token_configured: true, protocol_version: 1,
    connected: true, authenticated: true, state: "connected", session_age_secs: 30,
    last_message_ts: now - 1000, last_hello_ack_ts: now - 30000, last_report_ts: now - 1000,
    reconnect_count: 1, frames_received: 6, reports_sent: 4, reports_acked: 4, last_error: null,
  },
  work: {
    service: "xauusd-node4-execution", version: "1.0.0", role: "execution_only",
    venue: "deriv_mt5_demo", started_at: now - 60000, uptime_secs: 60,
    execution_protocol_version: 1, ledger_path: "data/ledger.jsonl", ledger_available: true,
    ledger_records: 8, ledger_intents: 1, intents_received: 1, intents_accepted: 1,
    intents_rejected: 0, intents_expired: 0, intents_duplicate: 0, orders_placed: 1,
    orders_filled: 1, orders_partial: 0, orders_rejected: 0, orders_unknown: 0,
    orders_reconciled: 0, last_intent_ts: now - 5000, last_report_ts: now - 1000,
    last_error: "<img src=x onerror=alert(1)>", ws_clients_connected: 1,
  },
  open_trades: {
    node4_open_trades: [trade], deriv_open_trades: [contract], mt5_open_positions: [position],
    recent_trades: [trade], total_open_count: 3, mt5_open_count: 1, timestamp: now,
  },
  deriv_account: {
    configured: true, connected: true, authorized: true, account_id: "VRTC123456",
    account_type: "demo", balance: 1000, currency: "USD", open_trades_count: 1,
    total_open_stake: 0.5, total_unrealized_pnl: 0.11, open_trades: [contract],
    last_updated: now, error: null, app_id_configured: true, token_kind: "legacy", setup_hint: null,
  },
  mt5: { account, positions, history, status: bridgeStatus },
  recent_events: [
    { timestamp: now - 2000, level: "info", category: "execution", message: "Accepted intent n3-fixture-buy-pw-poc" },
    { timestamp: now - 1000, level: "signal", category: "execution", message: "Report n3-fixture-buy-pw-poc → filled" },
  ],
};

 test("representative Node 3 and Node 4 fixtures render without merging strategy and broker state", () => {
  App.strategy.applyFrame({ type: "diagnostics", data: strategyDiagnostics });
  App.execution.applyFrame({ type: "diagnostics", data: node4Diagnostics });
  App.execution.applyFrame({ type: "trade", data: trade });
  App.execution.applyFrame({ type: "bridge_event", event: "order_filled", data: { intent_id: intent.intent_id, position_ticket: 123456789 } });

  const strategyHtml = elements.get("strategy-body").innerHTML;
  const executionHtml = elements.get("execution-body").innerHTML;
  const mt5Html = elements.get("mt5-body").innerHTML;

  assert.match(strategyHtml, /PW PoC/);
  assert.match(strategyHtml, /Retest remains valid/);
  assert.match(strategyHtml, /not an execution or fill/);
  assert.match(strategyHtml, /pending Node 4 acknowledgement/);
  assert.match(executionHtml, /execution report timeline/i);
  for (const status of ["accepted", "filled", "rejected", "unknown"]) {
    assert.match(executionHtml, new RegExp(`report-${status}`));
  }
  assert.match(executionHtml, /must not be treated as safe to retry/);
  assert.match(executionHtml, /stake is currency, not MT5 lots/);
  assert.match(executionHtml, /unit not supplied/);
  assert.match(executionHtml, /trade-mt5-987654321/);
  assert.match(executionHtml, /currency unknown/);
  assert.match(executionHtml, /&lt;img src=x onerror=alert\(1\)&gt;/);
  assert.doesNotMatch(executionHtml, /<img src=x onerror/);

  assert.match(mt5Html, /DEMO MODE CONFIRMED/);
  assert.match(mt5Html, /123456789/);
  assert.match(mt5Html, /0\.01 lots/);
  assert.match(mt5Html, /987654321/);
  assert.match(mt5Html, /5\.83/);
  assert.match(mt5Html, /order_filled/);
  assert.doesNotMatch(`${strategyHtml}${executionHtml}${mt5Html}`, /undefined|NaN/);
});

test("offline and stale service badges update independently", () => {
  App.utils.updateServiceStatus("market", "offline", { lastDataAt: null });
  App.utils.updateServiceStatus("strategy", "live", { lastDataAt: now });
  App.utils.updateServiceStatus("execution", "stale", { lastDataAt: now - 60000 });

  assert.equal(elements.get("market-service-badge").dataset.state, "offline");
  assert.equal(elements.get("market-service-badge").statusValue.textContent, "OFFLINE");
  assert.equal(elements.get("strategy-service-badge").dataset.state, "live");
  assert.equal(elements.get("strategy-service-badge").statusValue.textContent, "LIVE");
  assert.equal(elements.get("execution-service-badge").dataset.state, "stale");
  assert.equal(elements.get("execution-service-badge").statusValue.textContent, "STALE");

  App.utils.updateServiceStatus("market", "not-configured", { lastDataAt: null });
  assert.equal(elements.get("market-service-badge").statusValue.textContent, "NOT CONFIGURED");
  assert.match(elements.get("market-service-badge").title, /endpoint is not configured/);
});
