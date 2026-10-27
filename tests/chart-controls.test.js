"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

global.window = global;
global.App = {
  config: {
    chart: {
      intervals: ["1m", "15m", "1h"],
      interval: "15m",
      bucketSeconds: { "1m": 60, "15m": 900, "1h": 3600 },
    },
  },
  utils: { $: () => null },
};
global.document = { querySelectorAll: () => [], addEventListener() {} };

const calls = { intervals: [], loads: 0, gaps: [] };
App.chart = {
  setInterval(interval) { calls.intervals.push(interval); },
  loadHistory() { calls.loads += 1; return Promise.resolve(); },
};
App.panels = {
  showGap(info) { calls.gaps.push(info); },
  resolveGap() { calls.gaps.push({ resolved: true }); },
};
App.drawings = { requestUpdate() {} };

const controls = require("../js/chart-controls.js");

test("timeframe switch changes the typed candle interval and refetches history", async () => {
  assert.equal(controls.getInterval(), "15m");
  assert.equal(await controls.selectInterval("bad"), false);
  assert.equal(await controls.selectInterval("1h"), true);
  assert.equal(controls.getInterval(), "1h");
  assert.deepEqual(calls.intervals, ["1h"]);
  assert.equal(calls.loads, 1);
  assert.equal(controls.acceptsCandle({ interval: "15m" }), false);
  assert.equal(controls.acceptsCandle({ interval: "1h" }), true);
  assert.equal(controls.acceptsCandle({}), true, "legacy frames without interval remain accepted");
});

test("gap and non-live freshness transitions both refetch candle history", async () => {
  const before = calls.loads;
  assert.equal(await controls.handleGap({ dropped: 7, reason: "Node 1 dropped 7 frames" }), true);
  assert.equal(calls.loads, before + 1);
  assert.equal(calls.gaps.some((item) => item.dropped === 7), true);
  assert.equal(calls.gaps.some((item) => item.resolved === true), true);

  assert.equal(await controls.handleFreshness({ state: "live", reason: "ok" }), false);
  assert.equal(calls.loads, before + 1);
  assert.equal(await controls.handleFreshness({ state: "stale", reason: "old tick" }), true);
  assert.equal(calls.loads, before + 2);
  assert.equal(await controls.handleFreshness({ state: "offline", reason: "feed down" }), true);
  assert.equal(await controls.handleFreshness({ state: "closed_market", reason: "weekend" }), true);
  assert.equal(calls.loads, before + 4);
});

test("forming-bar state is driven by the candle payload", () => {
  controls.setCandles([{ closed: true }, { closed: false }]);
  assert.equal(controls.state.forming, true);
  controls.setCandleState(false);
  assert.equal(controls.state.forming, false);
});
