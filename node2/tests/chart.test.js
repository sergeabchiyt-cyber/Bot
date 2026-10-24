// chart.test.js — pure chart geometry (drawing is exercised by the browser).

import { test } from "node:test";
import assert from "node:assert/strict";
import { computeChartLayout, PAD } from "../public/src/render/chart.js";

const BARS = [
  { time: 100, open: 10, high: 12, low: 9, close: 11 },
  { time: 190, open: 11, high: 13, low: 10, close: 12.5 },
  { time: 280, open: 12.5, high: 14, low: 11, close: 11.5 },
];

test("layout: price range includes candle extremes with headroom", () => {
  const l = computeChartLayout(BARS, { width: 800, height: 400 });
  assert.ok(l.min < 9);
  assert.ok(l.max > 14);
  assert.ok((l.max - l.min) > (14 - 9)); // headroom added
});

test("layout: y() maps price to pixels, decreasing upward", () => {
  const l = computeChartLayout(BARS, { width: 800, height: 400 });
  const yLow = l.y(9);
  const yHigh = l.y(14);
  assert.ok(yLow > yHigh);
  assert.ok(yLow <= 400);
  assert.ok(yHigh >= 0);
});

test("layout: x() spreads bars across the plot width", () => {
  const l = computeChartLayout(BARS, { width: 800, height: 400 });
  const xs = BARS.map((_, i) => l.x(i));
  assert.ok(xs[0] < xs[1]);
  assert.ok(xs[1] < xs[2]);
  assert.ok(l.x(0) >= PAD.left);
});

test("layout: bar width is positive and bounded", () => {
  const l = computeChartLayout(BARS, { width: 800, height: 400 });
  assert.ok(l.barW >= 1);
  assert.ok(l.barW < l.stepX);
});

test("layout: position lines are included in the price range", () => {
  const position = { side: "buy", entry: 11, stopLoss: 9.5, takeProfit: 13.5 };
  const l = computeChartLayout(BARS, { width: 800, height: 400, position });
  assert.ok(l.min <= 9.5);
  assert.ok(l.max >= 13.5);
  assert.equal(l.positionLines.length, 3);
  const stop = l.positionLines.find((pl) => pl.key === "sl");
  assert.equal(stop.tone, "stop");
  assert.ok(stop.y > l.y(14) - 1); // stop line drawn near its price
});

test("layout: entry→TP progress fractions", () => {
  const buy = computeChartLayout(BARS, {
    width: 800, height: 400,
    position: { side: "buy", entry: 11, stopLoss: 10, takeProfit: 12 },
  });
  assert.equal(buy.progress, 0.5); // (11-10)/(12-10)

  const sell = computeChartLayout(BARS, {
    width: 800, height: 400,
    position: { side: "sell", entry: 11, stopLoss: 12, takeProfit: 10 },
  });
  assert.equal(sell.progress, 0.5); // (10-11)/(10-12)
});

test("layout: empty bars produce a neutral, valid layout", () => {
  const l = computeChartLayout([], { width: 800, height: 400 });
  assert.equal(l.count, 0);
  assert.ok(Number.isFinite(l.min));
  assert.ok(Number.isFinite(l.max));
  assert.equal(l.positionLines.length, 0);
});

test("layout: degenerate prices (flat bars) still produce a valid range", () => {
  const flat = BARS.map((b) => ({ time: b.time, open: 10, high: 10, low: 10, close: 10 }));
  const l = computeChartLayout(flat, { width: 800, height: 400 });
  assert.ok(Number.isFinite(l.min));
  assert.ok(Number.isFinite(l.max));
  assert.ok(l.max > l.min);
});
