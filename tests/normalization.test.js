"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const normalise = require("../js/normalize.js");

test("unwraps typed WebSocket frames and leaves direct REST snapshots intact", () => {
  const snapshot = { timestamp: 1700000000, work: { uptime_secs: 0 } };
  assert.deepEqual(normalise.unwrapFrame({ type: "strategy", data: snapshot }), snapshot);
  assert.deepEqual(normalise.unwrapFrame({ data: { data: snapshot } }), snapshot);
  assert.deepEqual(normalise.unwrapFrame(snapshot), snapshot);
  assert.equal(normalise.unwrapFrame(null), null);
});

test("normalizes epoch seconds, milliseconds, numeric strings, and ISO timestamps", () => {
  assert.equal(normalise.timestampMs(1700000000), 1700000000000);
  assert.equal(normalise.timestampMs(1700000000123), 1700000000123);
  assert.equal(normalise.timestampMs("1700000000"), 1700000000000);
  assert.equal(normalise.timestampMs("2026-10-07T12:30:00.000Z"), Date.parse("2026-10-07T12:30:00.000Z"));
  assert.equal(normalise.timestampMs(""), null);
  assert.equal(normalise.timestampMs("not a timestamp"), null);
  assert.equal(normalise.timestampMs(Number.MAX_VALUE), null);
});

test("preserves zero-valued balances, lots, timestamps, and prices", () => {
  assert.equal(normalise.numberOrNull(0), 0);
  assert.equal(normalise.numberOrNull("0"), 0);
  assert.equal(normalise.numberOrNull(null), null);
  assert.equal(normalise.numberOrNull("   "), null);
  assert.equal(normalise.numberOrNull("NaN"), null);
  assert.equal(normalise.timestampMs(0), 0);
});

test("escapes strings before HTML insertion", () => {
  assert.equal(normalise.escapeHtml(`<script x='1'>&"`), "&lt;script x=&#39;1&#39;&gt;&amp;&quot;");
  assert.equal(normalise.escapeHtml(null), "");
  assert.equal(normalise.escapeHtml(undefined), "");
});
