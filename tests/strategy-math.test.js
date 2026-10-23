"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const math = require("../js/strategy-math.js");

const decision = (ts, action, extra = {}) => ({
  ts, action, score: 0, confidence: 0.5, price: 3415, atr: 4, reasons: [], notes: [], ...extra,
});

test("action, regime and factor names are normalised for display", () => {
  assert.equal(math.actionOf({ action: "BUY" }), "buy");
  assert.equal(math.actionOf({ action: "short" }), "flat");
  assert.equal(math.regimeLabel("trend_up"), "Trend up");
  assert.equal(math.regimeLabel("balance"), "Balance");
  assert.equal(math.volatilityLabel("contracting"), "Contracting");
  assert.equal(math.factorLabel("value_location"), "Value location");
  assert.equal(math.factorLabel("tick_pressure"), "Tick pressure");
  assert.equal(math.factorLabel("custom_factor"), "Custom factor");
});

test("confidence is clamped to 0..100 and reasons sort by |contribution|", () => {
  assert.equal(math.confidencePct(0.62), 62);
  assert.equal(math.confidencePct(1.5), 100);
  assert.equal(math.confidencePct(null), null);
  const reasons = [
    { name: "trend", value: 0.2, weight: 0.3, contribution: 0.06 },
    { name: "momentum", value: 0.9, weight: 0.2, contribution: 0.18 },
    { name: "level_reaction", value: -0.7, weight: 0.25, contribution: -0.175 },
  ];
  assert.deepEqual(math.sortReasons(reasons).map((r) => r.name), ["momentum", "level_reaction", "trend"]);
});

test("the score bar is zero-centred and clamped", () => {
  assert.deepEqual(math.scoreBar(0.34), { value: 0.34, sign: "bull", widthPct: 17 });
  assert.deepEqual(math.scoreBar(-0.4), { value: -0.4, sign: "bear", widthPct: 20 });
  assert.deepEqual(math.scoreBar(0), { value: 0, sign: "zero", widthPct: 0 });
  assert.equal(math.scoreBar(2).widthPct, 50);
  assert.equal(math.scoreBar(null).value, null);
});

test("history is keyed on ts: a replay replaces, never duplicates", () => {
  const first = decision(1000, "buy", { confidence: 0.4 });
  const second = decision(2000, "sell", { confidence: 0.6 });
  let state = { last: null, history: [] };
  state = math.mergeDecision(state, first, 120);
  state = math.mergeDecision(state, second, 120);
  assert.equal(state.history.length, 2);
  assert.equal(state.last.ts, 2000);

  const replay = { ...first, confidence: 0.44 };
  state = math.mergeDecision(state, replay, 120);
  assert.equal(state.history.length, 2, "reconnect replay must not duplicate");
  assert.equal(state.history[0].confidence, 0.44, "replay replaces the keyed entry");
  assert.equal(state.last.ts, 2000, "an older replay does not rewind last");

  const older = decision(500, "flat");
  state = math.mergeDecision(state, older, 120);
  assert.equal(state.last.ts, 2000);
  assert.equal(state.history.length, 3);
  assert.deepEqual(state.history.map((d) => d.ts), [500, 1000, 2000], "history stays ascending");
});

test("history is capped oldest-first and ignores decisions without ts", () => {
  let state = { last: null, history: [] };
  for (const ts of [1000, 2000, 3000]) state = math.mergeDecision(state, decision(ts, "flat"), 2);
  assert.deepEqual(state.history.map((d) => d.ts), [2000, 3000]);

  const unchanged = math.mergeDecision(state, { action: "buy" }, 2);
  assert.equal(unchanged.changed, false);
  assert.equal(unchanged.history.length, 2);
});

test("the marker only survives while the current action is directional", () => {
  const buy = decision(1000, "buy");
  const buy2 = decision(2000, "buy");
  const sell = decision(3000, "sell");
  const flat = decision(4000, "flat");
  assert.equal(math.lastDirectionalChange([buy, buy2, sell]), sell);
  assert.equal(math.lastDirectionalChange([buy, flat]), null);
  assert.equal(math.lastDirectionalChange([]), null);
});

test("long position geometry spans stop to target with entry in the middle", () => {
  const geometry = math.progressToTarget({
    side: "buy", entry: 3415, stop: 3408, target: 3429, price: 3422,
  });
  assert.equal(geometry.available, true);
  assert.equal(geometry.hasStop, true);
  assert.equal(geometry.entryPct, (7 / 21) * 100);
  assert.equal(geometry.stopPct, 0);
  assert.equal(geometry.targetPct, 100);
  assert.equal(geometry.pricePct, (14 / 21) * 100);
  assert.equal(geometry.progressPct, 50);
  assert.equal(geometry.inProfit, true);
});

test("short position geometry puts the target on the low side", () => {
  const geometry = math.progressToTarget({
    side: "sell", entry: 3415, stop: 3422, target: 3408, price: 3412,
  });
  assert.equal(geometry.entryPct, 50);
  assert.equal(geometry.targetPct, 0);
  assert.equal(geometry.stopPct, 100);
  assert.equal(geometry.pricePct, (4 / 14) * 100);
  assert.ok(Math.abs(geometry.progressPct - (3 / 7) * 100) < 1e-9);
  assert.equal(geometry.inProfit, true);
});

test("missing stops or targets degrade instead of inventing prices", () => {
  const noStop = math.progressToTarget({ side: "buy", entry: 3415, stop: null, target: 3429, price: 3422 });
  assert.equal(noStop.available, true);
  assert.equal(noStop.hasStop, false);
  assert.equal(noStop.entryPct, 0);
  assert.equal(noStop.targetPct, 100);
  assert.equal(noStop.progressPct, 50);

  assert.equal(math.progressToTarget({ side: "buy", entry: 3415, target: null }).available, false);
  assert.equal(math.progressToTarget({ side: "buy", entry: 3415, target: 3415 }).available, false);
});

test("staleness compares decision ts with the 60 s budget", () => {
  const now = Date.now();
  assert.equal(math.isStale(now - 30000, now, 60000), false);
  assert.equal(math.isStale(now - 61000, now, 60000), true);
  assert.equal(math.isStale(null, now, 60000), true);
});
