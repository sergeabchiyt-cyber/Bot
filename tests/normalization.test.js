"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const normalise = require("../js/normalize.js");

test("unwraps typed WebSocket frames and leaves direct REST snapshots intact", () => {
  const snapshot = { timestamp: 1700000000, work: { uptime_secs: 0 } };
  assert.deepEqual(normalise.unwrapFrame({ type: "diagnostics", data: snapshot }), snapshot);
  assert.deepEqual(normalise.unwrapFrame({ data: { data: snapshot } }), snapshot);
  assert.deepEqual(normalise.unwrapFrame(snapshot), snapshot);
  const bridge = { type: "bridge_event", event: "halted", data: { reason: "guard" } };
  assert.deepEqual(normalise.unwrapFrame(bridge), bridge);
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

test("normalizes execution statuses without misclassifying unknown outcomes", () => {
  assert.equal(normalise.executionStatus("ACCEPTED"), "accepted");
  assert.equal(normalise.executionStatus("partially_filled"), "partial");
  assert.equal(normalise.executionStatus("canceled"), "cancelled");
  assert.equal(normalise.executionStatus("unknown"), "unknown");
  assert.equal(normalise.executionStatus("unexpected_status"), "unknown");
  assert.equal(normalise.executionStatus(null), "unknown");
});

test("deduplicates reports by their stable lifecycle key and keeps newest-first items", () => {
  const newest = {
    intent_id: "i-1", status: "filled", timestamp: 1700000000123,
    execution_id: "deal-1", error_code: null, quantity: 0.01,
  };
  const duplicate = { ...newest, quantity: 0.02 };
  const another = { ...newest, status: "partial", timestamp: 1700000000000 };
  const reports = normalise.dedupeBy([newest, duplicate, another], normalise.reportId, 10);
  assert.equal(reports.length, 2);
  assert.equal(reports[0].quantity, 0.01);
  assert.equal(reports[1].status, "partial");
  assert.equal(normalise.dedupeBy([newest, duplicate, another], normalise.reportId, 1).length, 1);
});

test("preserves zero-valued balances, quantities, timestamps, and prices", () => {
  assert.equal(normalise.numberOrNull(0), 0);
  assert.equal(normalise.numberOrNull("0"), 0);
  assert.equal(normalise.numberOrNull(null), null);
  assert.equal(normalise.numberOrNull("   "), null);
  assert.equal(normalise.numberOrNull("NaN"), null);
  assert.equal(normalise.timestampMs(0), 0);
});

test("escapes broker and error strings before HTML insertion", () => {
  assert.equal(normalise.escapeHtml(`<script x='1'>&"`), "&lt;script x=&#39;1&#39;&gt;&amp;&quot;");
  assert.equal(normalise.escapeHtml(null), "");
});

test("a late REST response cannot replace a newer WebSocket revision", () => {
  const revisions = normalise.createRevisionGuard();
  let state = { work: "initial REST" };
  const requestStartedAt = revisions.current("work");

  revisions.mark("work");
  state.work = "newer WebSocket";

  const applied = revisions.applyRest("work", requestStartedAt, () => {
    state.work = "late REST";
  });
  assert.equal(applied, false);
  assert.equal(state.work, "newer WebSocket");

  const currentRequest = revisions.current("work");
  assert.equal(revisions.applyRest("work", currentRequest, () => {
    state.work = "current REST fallback";
  }), true);
  assert.equal(state.work, "current REST fallback");
});
