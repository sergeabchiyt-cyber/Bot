// freshness.test.js — stale market data classification.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  FRESHNESS, classifyHeartbeat, classifySocket, classifyCandles,
  intervalMinutes, classifyFeedTone, freshnessLabel, freshnessTone,
} from "../public/src/lib/freshness.js";

const NOW = 1_700_000_000_000; // ms

test("intervalMinutes: parses m/h/d/w", () => {
  assert.equal(intervalMinutes("15m"), 15);
  assert.equal(intervalMinutes("5m"), 5);
  assert.equal(intervalMinutes("1h"), 60);
  assert.equal(intervalMinutes("4h"), 240);
  assert.equal(intervalMinutes("1d"), 1440);
  assert.equal(intervalMinutes("1w"), 10080);
  assert.equal(intervalMinutes("bogus"), null);
  assert.equal(intervalMinutes(undefined), null);
});

test("classifyHeartbeat: live within tolerance, stale beyond", () => {
  const hb = 30000;
  // 5s old -> live
  assert.equal(classifyHeartbeat(NOW - 5000, NOW, hb), FRESHNESS.LIVE);
  // 60s old (2x heartbeat) -> still live (limit is 2.5x = 75s)
  assert.equal(classifyHeartbeat(NOW - 60000, NOW, hb), FRESHNESS.LIVE);
  // 80s old -> stale
  assert.equal(classifyHeartbeat(NOW - 80000, NOW, hb), FRESHNESS.STALE);
  // no messages yet
  assert.equal(classifyHeartbeat(0, NOW, hb), FRESHNESS.UNKNOWN);
  // future timestamp (clock skew) -> unknown, not stale
  assert.equal(classifyHeartbeat(NOW + 5000, NOW, hb), FRESHNESS.UNKNOWN);
});

test("classifyHeartbeat: custom staleAfterMs overrides", () => {
  assert.equal(classifyHeartbeat(NOW - 20000, NOW, 30000, 15000), FRESHNESS.STALE);
  assert.equal(classifyHeartbeat(NOW - 10000, NOW, 30000, 15000), FRESHNESS.LIVE);
});

test("classifySocket: transport states map correctly", () => {
  const opts = { nowMs: NOW, heartbeatMs: 30000 };
  assert.equal(
    classifySocket({ state: "open", lastMsgTs: NOW - 5000, ...opts }), FRESHNESS.LIVE);
  assert.equal(
    classifySocket({ state: "open", lastMsgTs: NOW - 90000, ...opts }), FRESHNESS.STALE);
  assert.equal(
    classifySocket({ state: "connecting", lastMsgTs: 0, ...opts }), FRESHNESS.CONNECTING);
  assert.equal(
    classifySocket({ state: "reconnecting", lastMsgTs: 0, ...opts }), FRESHNESS.RECONNECTING);
  assert.equal(
    classifySocket({ state: "closed", lastMsgTs: 0, ...opts }), FRESHNESS.DISCONNECTED);
});

test("classifyCandles: edge lag beyond 2x interval is stale", () => {
  // classifyCandles consumes the parsed (normalized) status shape.
  const status = { candles: { interval: "15m", edgeLagMinutes: 27 } };
  const candles = [{ time: Math.floor(NOW / 1000) - 1800, open: 1, high: 2, low: 0.5, close: 1.5 }];
  const result = classifyCandles({ status, candles, nowMs: NOW });
  assert.equal(result.state, FRESHNESS.LIVE);
  assert.equal(result.edgeLagMin, 27);

  const staleStatus = { candles: { interval: "15m", edgeLagMinutes: 60 } };
  const staleResult = classifyCandles({ status: staleStatus, candles, nowMs: NOW });
  assert.equal(staleResult.state, FRESHNESS.STALE);
  assert.match(staleResult.reason, /edge lag/);
});

test("classifyCandles: old newest bar is stale", () => {
  const status = { candles: { interval: "15m", edgeLagMinutes: null } };
  const oldBar = { time: Math.floor((NOW - 1200000) / 1000), open: 1, high: 2, low: 0.5, close: 1.5 };
  const result = classifyCandles({ status, candles: [oldBar], nowMs: NOW });
  // 20 min old bar > 3x interval (45 min)? No — 20 min < 45 min, live.
  assert.equal(result.state, FRESHNESS.LIVE);

  const veryOld = { time: Math.floor((NOW - 4000000) / 1000), open: 1, high: 2, low: 0.5, close: 1.5 };
  const staleResult = classifyCandles({ status, candles: [veryOld], nowMs: NOW });
  assert.equal(staleResult.state, FRESHNESS.STALE);
});

test("classifyCandles: no candles or unknown interval", () => {
  assert.equal(
    classifyCandles({ status: { candles: { interval: "15m" } }, candles: [], nowMs: NOW }).state,
    FRESHNESS.UNKNOWN);
  assert.equal(
    classifyCandles({ status: { candles: { interval: "15m" } }, candles: null, nowMs: NOW }).state,
    FRESHNESS.UNKNOWN);
  assert.equal(
    classifyCandles({ status: null, candles: [{ time: 1 }], nowMs: NOW }).state,
    FRESHNESS.UNKNOWN);
});

test("classifyFeedTone + labels", () => {
  assert.equal(classifyFeedTone("connected"), "good");
  assert.equal(classifyFeedTone("error"), "bad");
  assert.equal(classifyFeedTone("degraded"), "warn");
  assert.equal(classifyFeedTone("off"), "muted");
  assert.equal(classifyFeedTone("idle"), "muted");
  assert.equal(classifyFeedTone("weird"), "muted");
  assert.equal(freshnessLabel(FRESHNESS.LIVE), "Live");
  assert.equal(freshnessLabel(FRESHNESS.STALE), "Stale");
  assert.equal(freshnessLabel(FRESHNESS.DISCONNECTED), "Disconnected");
  assert.equal(freshnessTone(FRESHNESS.LIVE), "good");
  assert.equal(freshnessTone(FRESHNESS.DISCONNECTED), "bad");
});
