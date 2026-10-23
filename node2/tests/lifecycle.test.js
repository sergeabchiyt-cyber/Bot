// lifecycle.test.js — broker-authoritative lifecycle semantics.
// The critical invariants: accepted/unknown are NEVER confirmed fills.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  LIFECYCLE_STATUSES, LIFECYCLE, lifecycleBadge, isConfirmedFill,
  tradeStatusKind, isTerminalStatus,
} from "../public/src/lib/lifecycle.js";

test("every lifecycle status has a distinct label and tone", () => {
  const badges = LIFECYCLE_STATUSES.map(lifecycleBadge);
  const labels = new Set(badges.map((b) => b.label));
  const tones = new Set(badges.map((b) => b.tone));
  assert.equal(labels.size, LIFECYCLE_STATUSES.length, "all 7 labels distinct");
  // 5 tones: good (filled), info (partial), warn (accepted),
  // bad (rejected + unknown), muted (cancelled + closed).
  assert.equal(tones.size, 5);
  for (const b of badges) {
    assert.ok(b.label.length > 0);
    assert.ok(b.icon);
    assert.ok(["good", "warn", "bad", "info", "muted"].includes(b.tone));
  }
});

test("isConfirmedFill mirrors Mt5OrderOutcome::is_confirmed_fill", () => {
  // filled + volume > 0 -> confirmed
  assert.equal(isConfirmedFill("filled", 0.5), true);
  // partial + volume > 0 -> confirmed (partial is still broker-confirmed)
  assert.equal(isConfirmedFill("partial", 0.15), true);
  // filled with volume 0 -> NOT confirmed
  assert.equal(isConfirmedFill("filled", 0), false);
  // partial with volume 0 -> NOT confirmed
  assert.equal(isConfirmedFill("partial", 0), false);
  // accepted NEVER confirmed
  assert.equal(isConfirmedFill("accepted", 0.5), false);
  assert.equal(isConfirmedFill("accepted", 5), false);
  // unknown NEVER confirmed — the core invariant
  assert.equal(isConfirmedFill("unknown", 0.5), false);
  assert.equal(isConfirmedFill("unknown", 999), false);
  // other statuses
  assert.equal(isConfirmedFill("rejected", 0.5), false);
  assert.equal(isConfirmedFill("cancelled", 0.5), false);
  assert.equal(isConfirmedFill("closed", 0.5), false);
});

test("lifecycleBadge: accepted and unknown never claim broker confirmation", () => {
  for (const status of ["accepted", "unknown"]) {
    const badge = lifecycleBadge(status);
    assert.equal(badge.brokerConfirmed, false);
    assert.equal(badge.fill, undefined);
    assert.notEqual(badge.label, "Filled");
  }
  assert.equal(lifecycleBadge("filled").brokerConfirmed, true);
  assert.equal(lifecycleBadge("partial").brokerConfirmed, true);
});

test("lifecycleBadge: unknown/garbage status strings fall back to Unknown", () => {
  const unknown = lifecycleBadge(undefined);
  assert.equal(unknown.label, "Unknown");
  assert.equal(unknown.status, "unknown");
  const garbage = lifecycleBadge("SOME_BOGUS_STATUS");
  assert.equal(garbage.status, "unknown");
  assert.equal(garbage.label, "Unknown");
  assert.equal(garbage.brokerConfirmed, false);
});

test("lifecycleBadge: case-insensitive, never fabricates a status", () => {
  assert.equal(lifecycleBadge("FILLED").status, "filled");
  assert.equal(lifecycleBadge("Accepted").status, "accepted");
  assert.equal(lifecycleBadge("").status, "unknown");
});

test("LIFECYCLE metadata has the required invariants", () => {
  assert.equal(LIFECYCLE.unknown.title.includes("NOT a confirmed fill"), true);
  assert.equal(LIFECYCLE.accepted.title.includes("pending"), true);
  assert.equal(LIFECYCLE.filled.brokerConfirmed, true);
  assert.equal(LIFECYCLE.partial.brokerConfirmed, true);
});

test("tradeStatusKind: never maps accepted/unknown to open/filled", () => {
  assert.equal(tradeStatusKind("filled", 0.5), "open");
  assert.equal(tradeStatusKind("accepted", 0), "accepted");
  assert.equal(tradeStatusKind("unknown", 0), "unknown");
  assert.equal(tradeStatusKind("rejected", 0), "rejected");
});

test("isTerminalStatus", () => {
  for (const s of ["filled", "rejected", "unknown", "cancelled", "closed"]) {
    assert.equal(isTerminalStatus(s), true);
  }
  assert.equal(isTerminalStatus("accepted"), false);
  assert.equal(isTerminalStatus("partial"), false);
});
