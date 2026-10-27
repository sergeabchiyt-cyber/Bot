"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

function element() {
  const classes = new Set();
  return {
    innerHTML: "",
    textContent: "",
    title: "",
    hidden: false,
    value: "",
    dataset: {},
    classList: {
      toggle(name, force) {
        if (force === undefined) classes.has(name) ? classes.delete(name) : classes.add(name);
        else if (force) classes.add(name);
        else classes.delete(name);
      },
      add(name) { classes.add(name); },
      remove(name) { classes.delete(name); },
      contains(name) { return classes.has(name); },
    },
    setAttribute() {},
    removeAttribute() {},
    addEventListener() {},
    querySelector() { return null; },
    focus() {},
  };
}

const ids = [
  "node3-badge", "freshness-badge", "contract-badges", "socket-badge",
  "node3-stream-badge", "delayed-badge", "status", "system-uptime",
  "system-pending", "system-node1", "system-vp", "system-updated",
  "connection-banner", "levels-list", "levels-count", "scanning-list",
  "scanning-count", "signals-list", "signals-count", "tick-volume-list",
  "tick-volume-count", "diagnostics-list", "diagnostics-count", "vp-list",
  "drawings-legend", "settings-body", "settings-stamp", "settings-drawer",
  "settings-backdrop", "settings-open", "settings-close",
];
const elements = new Map(ids.map((id) => [id, element()]));

global.window = global;
global.App = {};
global.document = {
  visibilityState: "visible",
  getElementById: (id) => elements.get(id) || null,
  querySelectorAll: () => [],
  addEventListener() {},
  removeEventListener() {},
};

require("../js/config.js");
require("../js/normalize.js");
require("../js/format.js");
require("../js/utils.js");
require("../js/client.js");
require("../js/panels.js");
require("../js/drawings.js");

const App = global.App;
App.client.configure(App.config);
const freshnessRefetches = [];
App.chartControls = {
  handleFreshness(status) {
    freshnessRefetches.push(status.state);
    return Promise.resolve(true);
  },
};

function fixture(relative) {
  return JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures", relative), "utf8"));
}
const body = (id) => elements.get(id).innerHTML;

function seedPanels() {
  App.panels.applyTopic("health", fixture("node3/health.json"), "rest");
  App.panels.applyTopic("levels", fixture("node3/levels.json"), "rest");
  App.panels.applyTopic("drawing", fixture("node3/drawings.json"), "rest");
  App.panels.applyTopic("scanning", fixture("node3/scanning.json"), "rest");
  App.panels.applyTopic("signals", fixture("node3/signals.json"), "rest");
  App.panels.applyTopic("diagnostics", fixture("node3/diagnostics.json"), "rest");
  App.panels.applyTopic("status", fixture("node1/status.json"), "rest");
  App.panels.applyTopic("tick_volume", fixture("node1/tick-volume.json"), "rest");
  App.panels.renderAll();
}

seedPanels();

test("header shows both contract versions, Node 3 health and Node 1 freshness", () => {
  assert.equal(elements.get("node3-badge").textContent, "NODE 3 OK");
  assert.equal(elements.get("node3-badge").dataset.state, "live");
  assert.equal(elements.get("freshness-badge").textContent, "FEED LIVE");
  assert.equal(elements.get("freshness-badge").dataset.state, "live");
  assert.match(elements.get("contract-badges").textContent, /N3 v2\.0 · N1 v2\.0/);
  assert.equal(elements.get("system-node1").textContent, "connected");
  assert.equal(elements.get("system-vp").textContent, "ready");
});

test("PS renders PoC only with no blank VaH/VaL value", () => {
  const markup = body("levels-list");
  const ps = markup.split('level-window">PS</span>')[1].split("</div>")[0];
  assert.match(ps, /PoC only/);
  assert.doesNotMatch(ps, /VaH|VaL|undefined|null/);
  assert.match(markup, /level-window">PW/);
  assert.match(markup, /4,201\.13/);
});

test("setups group by window and label projections as display only", () => {
  const markup = body("scanning-list");
  assert.match(markup, /class="setup-group"><h3>PW/);
  assert.match(markup, /class="setup-group"><h3>PS/);
  assert.match(markup, /Node 4 sizes this — display only/);
  assert.match(markup, /node4_managed/);
  assert.match(markup, /Broken above — awaiting retest/);
  assert.match(markup, /Retest/);
  assert.match(markup, /Invalidation/);
  assert.doesNotMatch(markup, /<(form|input|textarea|select)\b/i);
  assert.doesNotMatch(markup, /<button\b/i);
});

test("signals and diagnostics are read-only and show the last violation", () => {
  const signals = body("signals-list");
  assert.match(signals, /Pending intents/);
  assert.match(signals, /Recent intents/);
  assert.match(signals, /Execution reports/);
  assert.match(signals, /intent-PW-20261026-001/);
  assert.doesNotMatch(signals, /<(form|input|button)\b/i);

  const diagnostics = body("diagnostics-list");
  assert.match(diagnostics, /Contract violations: 1/);
  assert.match(diagnostics, /candle frame carried an unknown interval/);
  assert.match(diagnostics, /Published windows/);
  assert.match(elements.get("connection-banner").innerHTML, /Last: candle frame carried an unknown interval/);
});

test("each drawing id + timeframe survives dedupe and the legend uses payload style", () => {
  const drawings = App.drawings.apply(fixture("node3/drawings.json"));
  assert.equal(drawings.items.length, 5);
  assert.equal(drawings.keys.filter((key) => key.startsWith("PW:poc:")).length, 2);
  assert.equal(App.drawings.getState().renderedKeys.length, 5);
  const legend = body("drawings-legend");
  assert.match(legend, /background:#F0B90B/);
  assert.match(legend, /data-style="rectangle"/);
  assert.match(legend, /data-style="dotted"/);
});

test("stale freshness, a gap, and an unreachable Node 3 are visible warnings", () => {
  freshnessRefetches.length = 0;
  App.panels.applyTopic("status", fixture("node1/status-stale.json"), "ws");
  assert.match(elements.get("connection-banner").innerHTML, /Node 1 feed is stale/);
  assert.equal(elements.get("freshness-badge").dataset.state, "stale");
  assert.deepEqual(freshnessRefetches, ["stale"], "a bad freshness transition refetches candles");
  App.panels.applyTopic("status", fixture("node1/status-stale.json"), "ws");
  assert.deepEqual(freshnessRefetches, ["stale"], "repeated frames do not refetch again");

  App.panels.showGap({ service: "node1", source: "frame", dropped: 7, reason: "Node 1 dropped 7 frames" });
  assert.equal(elements.get("connection-banner").hidden, false);
  assert.match(elements.get("connection-banner").innerHTML, /Data incomplete/);
  App.panels.resolveGap();
  assert.match(elements.get("connection-banner").innerHTML, /refetched/i);

  App.panels.setError("node3", { message: "Failed to fetch", status: 0 });
  const banner = elements.get("connection-banner").innerHTML;
  assert.match(banner, /Node 3 unreachable/);
  assert.match(banner, /Last good:/);
  assert.match(banner, /Chart retains|Node 3 is disconnected|Node 3 unreachable/);
});

test("the drawer is a read-only service view with no form controls", () => {
  App.panels.setDrawer(true);
  const markup = body("settings-body");
  assert.match(markup, /Contracts/);
  assert.match(markup, /Node 3 health/);
  assert.match(markup, /Node 1 feed/);
  assert.match(markup, /Streams/);
  assert.doesNotMatch(markup, /<(form|input|select|textarea|button)\b/i);
  assert.match(markup, /Read-only dashboard/);
});
