"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

function element() {
  return {
    innerHTML: "",
    textContent: "",
    hidden: true,
    dataset: {},
    classList: { toggle() {}, add() {}, remove() {}, contains() { return false; } },
    setAttribute() {},
    addEventListener() {},
    focus() {},
    querySelector() { return null; },
  };
}

const ids = [
  "settings-body", "settings-stamp", "settings-drawer", "settings-backdrop",
  "settings-open", "settings-close",
];
const elements = new Map(ids.map((id) => [id, element()]));

global.window = { App: {} };
global.document = {
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
require("../js/status.js");

const App = global.window.App;
const now = Date.now();

test("the drawer renders read-only strategy, demo and feed state", () => {
  const ok = App.status.ingest({
    ts: now,
    strategy: { enabled: true, eval_secs: 15, min_confidence: 0.55, risk_pct: 0.5 },
    demo: { enabled: true, mode: "demo", leverage: 1000, spread: 0.1, min_lots: 0.01, max_lots: 1 },
    feeds: {
      market: { state: "live", last_update: now },
      candles: { state: "stale" },
      strategy: { connected: false },
      calendar: true,
    },
  }, "rest");
  assert.equal(ok, true);

  App.status.render();
  const html = elements.get("settings-body").innerHTML;

  assert.match(html, /Connection/);
  assert.match(html, /Strategy/);
  assert.match(html, /Demo engine/);
  assert.match(html, /Feeds/);
  assert.match(html, /1:1000/);
  assert.match(html, /55\.0%/);
  assert.match(html, /every 15 s/);
  assert.match(html, /DEMO/);

  assert.match(html, /feed-chip is-live[^>]*>live/);
  assert.match(html, /feed-chip is-warn[^>]*>stale/);
  assert.match(html, /feed-chip is-bad[^>]*>disconnected/);

  // Read-only: the drawer contains no form control or write action.
  assert.doesNotMatch(html, /<(input|select|textarea|form)\b/i);
  assert.doesNotMatch(html, /<(button)\b/i);
  assert.match(html, /Read-only dashboard/);
});

test("a malformed status payload is ignored instead of throwing", () => {
  assert.equal(App.status.ingest(null, "ws"), false);
  assert.equal(App.status.ingest([], "rest"), false);
  App.status.render();
  const html = elements.get("settings-body").innerHTML;
  assert.match(html, /Read-only dashboard/);
});
