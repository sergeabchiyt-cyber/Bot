"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const client = require("../js/client.js");

function fixture(relative) {
  return JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures", relative), "utf8"));
}

test("health and Node 1 status preserve contract truth and freshness", () => {
  const health = client.parseHealth(fixture("node3/health.json"));
  assert.equal(health.status, "ok");
  assert.equal(health.contractVersion, "2.0");
  assert.equal(health.node1Connected, true);
  assert.equal(health.vpReady, true);
  assert.equal(health.contractViolations, 1);
  assert.equal(health.intentsPending, 1);
  assert.equal(health.linkConfigured, true);

  const status = client.parseNode1Status(fixture("node1/status.json"));
  assert.equal(status.contractVersion, "2.0");
  assert.equal(status.bootId, "boot-20261026-001");
  assert.equal(status.state, "live");
  assert.equal(status.freshness.feed, "sifting");
  assert.equal(status.freshness.state, "live");
  assert.equal(status.marketOpen, true);

  const stale = client.parseNode1Status(fixture("node1/status-stale.json"));
  assert.equal(stale.state, "stale");
  assert.match(stale.reason, /93000|93,000/);
});

test("PS levels keep PoC only; VaH/VaL keys are absent rather than blank", () => {
  const levels = client.parseLevels(fixture("node3/levels.json"));
  assert.equal(levels.length, 3);
  const ps = levels.find((level) => level.window === "PS");
  assert.equal(ps.poc, 4212.625);
  assert.equal(Object.prototype.hasOwnProperty.call(ps, "vah"), false);
  assert.equal(Object.prototype.hasOwnProperty.call(ps, "val"), false);
  const pw = levels.find((level) => level.window === "PW");
  assert.equal(pw.vah, 4238.25);
  assert.equal(pw.val, 4164.0);
  assert.equal(pw.sunday_open, 4197.5);
  assert.equal(pw.meta.rows, 128);
});

test("drawings dedupe by id plus timeframe and keep both timeframes", () => {
  const drawings = client.parseDrawings(fixture("node3/drawings.json"));
  const pocKeys = drawings.keys.filter((key) => key.startsWith("PW:poc:"));
  assert.deepEqual(pocKeys, [
    "PW:poc:1792584000000:1793016000000::1H",
    "PW:poc:1792584000000:1793016000000::daily",
  ]);
  assert.equal(drawings.items.length, 5, "one exact duplicate is removed");
  assert.equal(drawings.pending.length, 1);
  const frvp = drawings.items.find((item) => item.kind === "frvp");
  assert.equal(frvp.rows.length, 128);
  assert.equal(frvp.color, "#F0B90B");
  assert.equal(frvp.line_style, "rectangle");
  assert.ok(frvp.rows.every((row) => row.up_volume >= 0 && row.down_volume >= 0));
  assert.equal(client.parseDrawings({ type: "drawing", data: fixture("node3/drawings.json") }).items.length, 5);
});

test("scanning preserves every display value and never derives a size", () => {
  const scanning = client.parseScanning(fixture("node3/scanning.json"));
  assert.equal(scanning.setups.length, 2);
  assert.equal(scanning.atr, 7.6);
  assert.equal(scanning.atr_pips, 76);
  assert.equal(scanning.sl_pips, 76);
  assert.equal(scanning.tp_pips, 152);
  assert.equal(scanning.rr, 2);
  const setup = scanning.setups[0];
  assert.equal(setup.state_label, "Broken above — awaiting retest");
  assert.equal(setup.volume_confirmed, true);
  assert.equal(setup.active_projection.sizing, "node4_managed");
  assert.equal(setup.active_projection.sl_pips, 76);
  assert.equal(scanning.setups[1].window, "PS");
});

test("signals and diagnostics expose the full read-only wire shape", () => {
  const signals = client.parseSignals(fixture("node3/signals.json"));
  assert.equal(signals.pending_count, 1);
  assert.equal(signals.pending[0].intent_id, "intent-PW-20261026-001");
  assert.equal(signals.recent.length, 1);
  assert.equal(signals.execution_reports[0].status, "filled");
  assert.equal(signals.linkConfigured, true);

  const diagnostics = client.parseDiagnostics(fixture("node3/diagnostics.json"));
  assert.equal(diagnostics.contractViolations, 1);
  assert.equal(diagnostics.lastViolation, "candle frame carried an unknown interval");
  assert.equal(diagnostics.work.vp_ready, true);
  assert.equal(diagnostics.recent_events.length, 2);
});

test("VpAudit frames and REST bodies parse to the same 128-row view", () => {
  const vp = client.parseVp(fixture("node3/vp-PW.json"));
  assert.equal(vp.window, "PW");
  assert.equal(vp.rowsCount, 128);
  assert.equal(vp.histogram.length, 128);
  assert.equal(vp.poc, 4201.125);
  assert.equal(client.parseVp({ type: "vp", data: fixture("node3/vp-PW.json") }).histogram.length, 128);
});

test("each recorded Node 1 candle interval is ascending, unique and typed", () => {
  for (const interval of ["1m", "15m", "1h"]) {
    const candles = client.parseCandles(fixture(`node1/candles-${interval}.json`), interval);
    assert.ok(candles.length >= 4, interval);
    assert.deepEqual(candles.map((candle) => candle.time), [...new Set(candles.map((candle) => candle.time))].sort((a, b) => a - b));
    assert.ok(candles.every((candle) => candle.interval === interval && candle.symbol === "XAUUSD"));
    assert.equal(candles.filter((candle) => candle.closed === false).length, interval === "1m" ? 1 : 0);
  }

  const duplicated = [...fixture("node1/candles-15m.json"), fixture("node1/candles-15m.json")[0]];
  assert.equal(client.parseCandles(duplicated, "15m").length, 4);
  assert.equal(client.parseCandles(fixture("node1/candles-1m.json"), "15m").length, 0,
    "an interval mismatch is not mixed into the chart");
});

test("tick-volume accepts bar arrays and a single in-progress bar", () => {
  const snapshot = client.parseTickVolume(fixture("node1/tick-volume.json"));
  assert.equal(snapshot.bars.length, 4);
  assert.equal(snapshot.last.closed, false);
  assert.equal(snapshot.ticks, snapshot.last.ticks);
  assert.equal(snapshot.last.flat_ticks + snapshot.last.up_ticks + snapshot.last.down_ticks, snapshot.last.ticks);
  const single = client.parseTickVolume({ ticks: 12, up_ticks: 5, down_ticks: 4, flat_ticks: 3, last_tick: 1793016000000, closed: false });
  assert.equal(single.bars.length, 1);
  assert.equal(single.closed, false);
});

test("calendar, replay, live and gap frames are classified without throwing", () => {
  const calendar = client.parseCalendar(fixture("node1/calendar.json"));
  assert.equal(calendar.events.length, 2);
  assert.equal(calendar.count, 2);

  const replay = client.parseFrame({ type: "status", replay: true, data: fixture("node1/status.json") }, "node1");
  assert.equal(replay.replay, true);
  assert.equal(replay.seq, null);
  assert.equal(replay.data.contract_version, "2.0");

  const live = client.parseFrame({ type: "candle", seq: 42, data: { interval: "1m" } }, "node1");
  assert.equal(live.seq, 42);
  assert.equal(live.replay, false);

  const gap = client.parseFrame(fixture("node1/gap-frame.json"), "node1");
  assert.equal(gap.type, "gap");
  assert.equal(gap.data.dropped, 7);
  assert.equal(client.parseCalendar(null).events.length, 0);
});
