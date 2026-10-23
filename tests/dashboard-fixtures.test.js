"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

function element() {
  const classes = new Set();
  return {
    innerHTML: "",
    textContent: "",
    className: "",
    title: "",
    hidden: false,
    dataset: {},
    attributes: {},
    classList: {
      toggle(name, force) {
        if (force === undefined) {
          if (classes.has(name)) classes.delete(name);
          else classes.add(name);
        } else if (force) classes.add(name);
        else classes.delete(name);
      },
      add(name) { classes.add(name); },
      remove(name) { classes.delete(name); },
      contains(name) { return classes.has(name); },
    },
    setAttribute(name, value) { this.attributes[name] = String(value); },
    querySelector() { return null; },
    focus() {},
    addEventListener() {},
  };
}

const ids = [
  "strategy-body", "strategy-state", "panel-strategy", "strat-age", "strat-conn",
  "demo-strip", "demo-mode", "demo-spec", "demo-balance", "demo-equity",
  "demo-free-margin", "demo-margin-level", "demo-open-pnl", "demo-engine", "demo-updated",
  "position-body", "position-state", "performance-body", "trades-body", "trades-count",
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
require("../js/format.js");
require("../js/strategy-math.js");
require("../js/utils.js");
require("../js/api.js");
require("../js/strategy.js");
require("../js/demo.js");

const App = global.window.App;
const now = Date.now();
const MINUS = "\u2212";

const reason = (name, value, weight, contribution, detail) => ({
  name, value, weight, contribution, detail,
});

function decision(overrides = {}) {
  return {
    ts: now,
    symbol: "XAUUSD",
    action: "buy",
    score: 0.34,
    confidence: 0.62,
    price: 3415.1,
    atr: 4.2,
    regime: "trend_up",
    volatility: "expanding",
    stop_loss: 3408.2,
    take_profit: 3428.9,
    reward_risk: 2,
    lots: 0.18,
    risk_usd: 48.6,
    reasons: [
      reason("value_location", -0.35, 0.2, -0.07, "price above the weekly VAH"),
      reason("momentum", 0.9, 0.15, 0.135, "5-bar ROC +0.42%"),
      reason("trend", 0.72, 0.25, 0.18, "EMA9 3415.10 vs EMA21 3409.32"),
    ],
    notes: [],
    next_event_ts: null,
    ...overrides,
  };
}

const params = {
  enabled: true, min_confidence: 0.55, risk_pct: 0.5, atr_period: 14, atr_stop_mult: 1.5,
  reward_risk: 2, min_lots: 0.01, max_lots: 1, lot_step: 0.01, fast_ema: 9, slow_ema: 21,
  momentum_bars: 5, tick_pressure_bars: 5, event_blackout_secs: 900, leverage: 1000, max_margin_pct: 50,
};

function demoSnapshot(overrides = {}) {
  return {
    enabled: true,
    mode: "demo",
    symbol: "XAUUSD",
    leverage: 1000,
    quote: { bid: 3422.4, ask: 3422.5, mid: 3422.45, spread: 0.1, ts: now },
    account: {
      currency: "USD", balance: 10000, equity: 10012.4, margin: 61.7, free_margin: 9950.7,
      margin_level: 16227.6, unrealized_pnl: 12.4, peak_equity: 10300, max_drawdown_pct: 4.2,
    },
    params: { start_balance: 10000, leverage: 1000, spread: 0.1, min_lots: 0.01, max_lots: 1, lot_step: 0.01, stop_out_pct: 50 },
    position: {
      id: 7, side: "buy", lots: 0.18, entry: 3415.1, stop_loss: 3408.2, take_profit: 3428.9,
      opened_at: now - 600000, risk_usd: 48.6, margin: 61.7, unrealized_pnl: 12.4,
      reason: "trend + value_location + momentum",
    },
    stats: {
      closed_trades: 12, wins: 7, losses: 5, win_rate: 58.3, gross_profit: 220, gross_loss: -120,
      net_pnl: 100, profit_factor: 1.83, avg_r: 0.42, max_drawdown_pct: 6.4,
    },
    recent_trades: [
      { id: 2, side: "sell", lots: 0.1, entry: 3420.5, exit: 3426, opened_at: now - 7200000, closed_at: now - 7000000, pnl: -11, r_multiple: -1.0, exit_reason: "stop_loss" },
      { id: 1, side: "buy", lots: 0.18, entry: 3415.1, exit: 3428.9, opened_at: now - 3600000, closed_at: now - 3500000, pnl: 24.84, r_multiple: 2.04, exit_reason: "take_profit" },
      { id: 4, side: "sell", lots: 0.2, entry: 3430, exit: 3433, opened_at: now - 1800000, closed_at: now - 1700000, pnl: -12, r_multiple: -1.2, exit_reason: "stop_out" },
      { id: 3, side: "buy", lots: 0.05, entry: 3410, exit: 3412, opened_at: now - 5400000, closed_at: now - 5300000, pnl: 1, r_multiple: 0.4, exit_reason: "signal_flip" },
    ],
    note: "opened buy 0.18 lot(s) at 3415.10",
    ts: now,
    ...overrides,
  };
}

const strategyHtml = () => elements.get("strategy-body").innerHTML;
const positionHtml = () => elements.get("position-body").innerHTML;
const performHtml = () => elements.get("performance-body").innerHTML;
const tradesHtml = () => elements.get("trades-body").innerHTML;

test("the strategy panel survives a boot with no decision yet", () => {
  App.strategy.render();
  assert.match(strategyHtml(), /Waiting for the first evaluation/);
});

test("a buy decision renders action, gauge, score, chips and formatted numbers", () => {
  App.strategy.applySnapshot({
    symbol: "XAUUSD", enabled: true, params,
    last: decision(),
    history: [decision({ ts: now - 15000, action: "flat", confidence: 0.3 })],
  });

  const html = strategyHtml();
  assert.match(html, /action-badge is-buy">BUY/);
  assert.match(html, /Confidence <b>62\.0%<\/b>/);
  assert.match(html, /left:55\.00%/, "threshold tick comes from params.min_confidence");
  assert.match(html, /\+0\.34/);
  assert.match(html, /width:17\.00%;left:50\.00%/);
  assert.match(html, /Trend up/);
  assert.match(html, /Expanding/);
  assert.match(html, /3,415\.10/);
  assert.match(html, /3,408\.20/);
  assert.match(html, /3,428\.90/);
  assert.match(html, /0\.18 lots/);
  assert.match(html, /\$48\.60 at risk/);
  assert.match(html, /ENGINE ON/);
  assert.match(html, /by contribution/);

  // reasons sorted by |contribution|: trend, momentum, value location
  const trend = html.indexOf("Trend");
  const momentum = html.indexOf("Momentum");
  const value = html.indexOf("Value location");
  assert.ok(trend >= 0 && trend < momentum && momentum < value);
  assert.match(html, /25\.0%/);
  assert.match(html, /EMA9 3415\.10 vs EMA21 3409\.32/);

  // history strip: one bar per stored decision, keyed by ts
  assert.equal(App.strategy.getState().history.length, 2);
  assert.equal((html.match(/<rect/g) || []).length, 2);
  assert.doesNotMatch(html, /undefined|NaN/);
});

test("a sell decision renders the mirrored badge, score and reason bars", () => {
  App.strategy.applyDecision(decision({
    ts: now + 500,
    action: "sell",
    score: -0.42,
    confidence: 0.71,
    stop_loss: 3428.9,
    take_profit: 3408.2,
    lots: 0.12,
    risk_usd: 31.2,
    notes: ["momentum fading"],
    reasons: [
      reason("trend", -0.68, 0.25, -0.17, "EMA9 below EMA21"),
      reason("tick_pressure", -0.4, 0.15, -0.06, "sell-side tick pressure"),
    ],
  }));

  const html = strategyHtml();
  assert.match(html, /action-badge is-sell">SELL/);
  assert.match(html, /\u22120\.42|${MINUS}0\.42/);
  assert.match(html, /width:21\.00%;left:29\.00%/);
  assert.match(html, /Confidence <b>71\.0%<\/b>/);
  assert.match(html, /0\.12 lots/);
  assert.match(html, /\$31\.20 at risk/);
  assert.match(html, /Tick pressure/);
  assert.match(html, /class="notes"/);
  assert.doesNotMatch(html, /notes is-flat/);
});

test("a late REST seed updates evaluator config without rewinding newer decisions", () => {
  const before = App.strategy.getState().last.ts;
  const applied = App.strategy.applySnapshot({
    symbol: "XAUUSD",
    enabled: true,
    params: { ...params, min_confidence: 0.66, risk_pct: 0.75 },
    last: decision({ ts: now - 5000, action: "flat", confidence: 0.2 }),
    history: [decision({ ts: now - 5000, action: "flat", confidence: 0.2 })],
  }, "rest", -1);

  const state = App.strategy.getState();
  assert.equal(applied, false, "stale decision data is not applied");
  assert.equal(state.params.min_confidence, 0.66, "config is still seeded");
  assert.equal(state.params.risk_pct, 0.75);
  assert.equal(state.last.ts, before, "a newer WS decision is preserved");
  assert.match(strategyHtml(), /left:66\.00%/, "the gauge threshold follows the seed");
});

test("a flat decision shows notes in amber and the blackout countdown", () => {
  App.strategy.applyDecision(decision({
    ts: now + 1000,
    action: "flat",
    score: -0.12,
    confidence: 0.42,
    lots: 0,
    risk_usd: 0,
    stop_loss: null,
    take_profit: null,
    notes: ["confidence 0.42 below threshold", "no edge"],
    next_event_ts: now + 12 * 60000,
  }));

  const html = strategyHtml();
  assert.match(html, /action-badge is-flat">FLAT/);
  assert.match(html, /class="notes is-flat"/);
  assert.match(html, /confidence 0\.42 below threshold/);
  assert.match(html, /no edge/);
  assert.match(html, /event in 12 min \u2014 standing aside/);
  assert.match(html, /no position \u2014 flat/);
  assert.doesNotMatch(html, /0\.00 lots/, "a flat decision never advertises a zero-lot position");
});

test("a reconnect replay of the same ts is not duplicated", () => {
  const replay = decision({ ts: now + 1000, action: "flat", confidence: 0.42 });
  const before = App.strategy.getState().history.length;
  App.strategy.applyDecision(replay);
  assert.equal(App.strategy.getState().history.length, before);
});

test("the demo strip reads leverage, spread and every account value from the payload", () => {
  App.demo.applySnapshot(demoSnapshot(), "ws");
  assert.equal(elements.get("demo-mode").textContent, "DEMO");
  assert.equal(elements.get("demo-spec").textContent, "1:1000 \u00b7 spread 0.10");
  assert.equal(elements.get("demo-balance").textContent, "$10,000.00");
  assert.equal(elements.get("demo-equity").textContent, "$10,012.40");
  assert.equal(elements.get("demo-free-margin").textContent, "$9,950.70");
  assert.equal(elements.get("demo-open-pnl").textContent, "+$12.40");
  assert.match(elements.get("demo-margin-level").textContent, /%/);
  assert.match(elements.get("demo-strip").title, /no broker, no real money/);
  assert.equal(elements.get("demo-engine").hidden, true);
});

test("a flat demo account shows an em dash for margin level and the empty position state", () => {
  App.demo.applySnapshot(demoSnapshot({
    ts: now + 1,
    account: {
      currency: "USD", balance: 9950, equity: 9940, margin: 0, free_margin: 9940,
      margin_level: 0, unrealized_pnl: -10, peak_equity: 10000, max_drawdown_pct: 0.6,
    },
    position: null,
    note: "waiting for a qualifying signal",
  }), "ws");
  assert.equal(elements.get("demo-margin-level").textContent, "\u2014");
  assert.match(positionHtml(), /No open position/);
  assert.match(positionHtml(), /waiting for a qualifying signal/);
  assert.match(positionHtml(), /No open position/, "history can exist without a position");

  // The contract pins margin_level === 0 to an em dash even if margin is not 0.
  App.demo.applySnapshot(demoSnapshot({
    ts: now + 1.5,
    account: {
      currency: "USD", balance: 9950, equity: 9940, margin: 50, free_margin: 9890,
      margin_level: 0, unrealized_pnl: -10, peak_equity: 10000, max_drawdown_pct: 0.6,
    },
    position: null,
  }), "ws");
  assert.equal(elements.get("demo-margin-level").textContent, "\u2014");
});

test("the open position card shows the trade, its progress and the engine reason", () => {
  App.demo.applySnapshot(demoSnapshot({ ts: now + 2 }), "ws");
  const html = positionHtml();
  assert.match(html, /side-chip is-buy">BUY/);
  assert.match(html, /0\.18 lots/);
  assert.match(html, /\+ \$12\.40|\+\$12\.40/);
  assert.match(html, /pos-bar/);
  assert.match(html, /pos-zone is-profit/);
  assert.match(html, /pos-zone is-loss/);
  assert.match(html, /Entry 3,415\.10/);
  assert.match(html, /SL 3,408\.20/);
  assert.match(html, /TP 3,428\.90/);
  assert.match(html, /\$61\.70/, "margin comes from position.margin");
  assert.match(html, /\$48\.60/);
  assert.match(html, /trend \+ value_location \+ momentum/);
  assert.match(html, /Engine:/);
});

test("performance shows the stats, caps profit factor and handles zero trades", () => {
  App.demo.applySnapshot(demoSnapshot({ ts: now + 3 }), "ws");
  let html = performHtml();
  assert.match(html, /Closed trades[\s\S]*12/);
  assert.match(html, /58\.3%/);
  assert.match(html, /\+ \$100\.00|\+\$100\.00/);
  assert.match(html, /1\.83/);
  assert.match(html, /\+0\.42R/);
  assert.match(html, /6\.4%/);

  App.demo.applySnapshot(demoSnapshot({
    ts: now + 4,
    stats: {
      closed_trades: 0, wins: 0, losses: 0, win_rate: 0, gross_profit: 0, gross_loss: 0,
      net_pnl: 0, profit_factor: 0, avg_r: 0, max_drawdown_pct: 0,
    },
    recent_trades: [],
  }), "ws");
  html = performHtml();
  assert.match(html, /0\.0%/);
  assert.match(html, /\u2014/);
  assert.equal((html.match(/99\+/g) || []).length, 0);
  assert.match(elements.get("trades-body").innerHTML, /No closed demo trades yet\./);

  App.demo.applySnapshot(demoSnapshot({
    ts: now + 5,
    stats: { ...demoSnapshot().stats, profit_factor: 120, closed_trades: 12 },
  }), "ws");
  assert.match(performHtml(), /99\+/);
});

test("trade history renders every exit reason, signed P&L and R newest first", () => {
  App.demo.applySnapshot(demoSnapshot({ ts: now + 6 }), "ws");
  const html = tradesHtml();
  assert.match(html, /exit-chip is-sl">SL/);
  assert.match(html, /exit-chip is-tp">TP/);
  assert.match(html, /exit-chip is-flip">flip/);
  assert.match(html, /exit-chip is-stop-out">stop-out/);
  assert.match(html, /\+\$24\.84/);
  assert.match(html, new RegExp(`${MINUS}\\$11\\.00`));
  assert.match(html, /\+2\.04R/);
  assert.match(html, new RegExp(`${MINUS}1\\.00R`));

  const first = html.indexOf(">1</td>");
  const second = html.indexOf(">2</td>");
  assert.ok(first >= 0 && second >= 0 && first < second, "newest closed trade first");
  assert.equal((html.match(/<tr>/g) || []).length, 5, "header + four trades");
});

test("an engine-off demo dims the strip and says so", () => {
  App.demo.applySnapshot(demoSnapshot({ ts: now + 7, enabled: false }), "ws");
  assert.equal(elements.get("demo-strip").classList.contains("is-off"), true);
  assert.equal(elements.get("demo-engine").hidden, false);
  assert.equal(elements.get("demo-engine").textContent, "demo engine off");
});

test("hostile strings are escaped before they reach innerHTML", () => {
  App.strategy.applyDecision(decision({
    ts: now + 8000,
    action: "flat",
    notes: ["<img src=x onerror=alert(1)>"],
    reasons: [reason("trend", 0.2, 0.3, 0.06, "<script>alert(2)</script>")],
  }));
  const html = strategyHtml();
  assert.match(html, /&lt;img src=x onerror=alert\(1\)&gt;/);
  assert.match(html, /&lt;script&gt;alert\(2\)&lt;\/script&gt;/);
  assert.doesNotMatch(html, /<img src=x onerror/);
  assert.doesNotMatch(html, /<script>alert/);
});

test("no order-flow surface is rendered anywhere", () => {
  const combined = `${strategyHtml()}${positionHtml()}${performHtml()}${tradesHtml()}`;
  assert.doesNotMatch(combined, /bubble|absorption|order.?flow|delta/i);
});
