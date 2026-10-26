"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fixtures = require("./fixtures/dashboard-contracts.json");
const clientModule = require("../js/node2-client.js");

const clientWithFixtures = () => {
  const seen = [];
  const client = clientModule.createClient({}, async (url, options) => {
    const parsed = new URL(url);
    seen.push({ url: parsed, options });
    const path = parsed.pathname;
    let value;
    if (parsed.host === "strategy-api-vp-levels.onrender.com") {
      value = ({ "/health": fixtures.node3.health, "/levels": fixtures.node3.levels,
        "/drawings": fixtures.node3.drawings, "/scanning": fixtures.node3.scanning,
        "/signals": fixtures.node3.signals, "/diagnostics": fixtures.node3.diagnostics,
        "/vp": fixtures.node3.vp })[path];
    } else {
      value = ({ "/candles": fixtures.node1.candles[parsed.searchParams.get("interval")],
        "/tick-volume": fixtures.node1.tick_volume, "/calendar": fixtures.node1.calendar,
        "/status": fixtures.node1.status })[path];
    }
    if (value === undefined) return { ok: false, status: 404, text: async () => "not recorded" };
    return { ok: true, json: async () => structuredClone(value) };
  });
  return { client, seen };
};

test("base configuration accepts only two safe service origins", () => {
  assert.deepEqual(clientModule.baseConfig({ NODE1_BASE_URL: "http://market.test/", NODE3_BASE_URL: "https://strategy.test" }),
    { node1: "http://market.test", node3: "https://strategy.test" });
  assert.equal(clientModule.baseConfig({ NODE1_BASE_URL: "https://u:p@market.test" }).node1, "");
  assert.equal(clientModule.baseConfig({ NODE3_BASE_URL: "https://strategy.test/path" }).node3, "");
});

test("Node 3 parsers accept smoke snapshots, preserve drawings by timeframe, and allow PS PoC only", () => {
  const p = clientModule;
  assert.equal(p.parseHealth(fixtures.node3.health).contract_version, "2.0");
  const levels = p.parseLevels(fixtures.node3.levels);
  const ps = levels.find((level) => level.window === "PS");
  assert.equal(ps.poc, 3298);
  assert.equal(ps.vah, null);
  assert.equal(ps.val, null);
  const drawings = p.parseDrawings(fixtures.node3.drawings);
  assert.equal(drawings.filter((item) => item.id.includes(":level:")).length, 2);
  assert.deepEqual(new Set(drawings.filter((item) => item.id.includes(":level:")).map((item) => item.timeframe)), new Set(["1H", "daily"]));
  assert.equal(drawings.find((item) => item.kind === "frvp").rows[0].up_volume, 150);
  const largePayload = structuredClone(fixtures.node3.drawings);
  const frvp = largePayload.items.find((item) => item.kind === "frvp");
  frvp.rows = Array.from({ length: 128 }, (_, index) => ({ low: 3200 + index, high: 3201 + index,
    price: 3200.5 + index, volume: 100 + index, up_volume: 60 + index, down_volume: 40 }));
  assert.equal(p.parseDrawings(largePayload).find((item) => item.kind === "frvp").rows.length, 128);
  assert.equal(p.parseScanning(fixtures.node3.scanning).setups[0].projected_buy.sizing, "node4_managed");
  assert.equal(p.parseSignals(fixtures.node3.signals).pending_count, 1);
  assert.equal(p.parseDiagnostics(fixtures.node3.diagnostics).service, "node3");
  const safeDiagnostics = p.parseDiagnostics({ status: "ok", NODE4_SHARED_TOKEN: "must-not-be-retained", nested: { access_token: "also-secret", state: "ready" } });
  assert.deepEqual(safeDiagnostics, { status: "ok", nested: { state: "ready" } });
  assert.equal(p.parseVp(fixtures.node3.vp).rows.length, 1);
});

test("Node 1 candle fixtures parse for all supported intervals and retain forming state", () => {
  for (const interval of ["1m", "15m", "1h"]) {
    const candles = clientModule.parseCandles(fixtures.node1.candles[interval]);
    assert.equal(candles.length, 1);
    assert.equal(candles[0].interval, interval);
    assert.equal(candles[0].closed, true);
  }
  assert.throws(() => clientModule.parseCandles([{ time: "bad", open: 1, high: 2, low: 0, close: 1 }]), /candle.time/);
});

test("smoke reads all required Node 3 snapshots and Node 1 market data via GET only", async () => {
  const { client, seen } = clientWithFixtures();
  await client.node3.health(); await client.node3.levels(); await client.node3.drawings();
  await client.node3.scanning(); await client.node3.signals(); await client.node3.diagnostics();
  await client.node3.vp({ window: "PW", rows: 128 });
  for (const interval of ["1m", "15m", "1h"]) await client.node1.candles(interval);
  await client.node1.tickVolume(); await client.node1.calendar(); await client.node1.status();
  assert.equal(seen.length, 13);
  assert.ok(seen.every(({ options }) => options.method === "GET" && options.cache === "no-store"));
  assert.deepEqual(seen.filter(({ url }) => url.pathname === "/candles").map(({ url }) => url.searchParams.get("interval")), ["1m", "15m", "1h"]);
  assert.equal(seen.find(({ url }) => url.pathname === "/vp").url.searchParams.get("rows"), "128");
  assert.ok(seen.every(({ url }) => !/execution|demo|strategy$/.test(url.pathname)));
});

test("HTTP errors preserve error body text for custom range FRVP failures", async () => {
  const client = clientModule.createClient({}, async () => ({ ok: false, status: 400, text: async () => "end must be after start" }));
  await assert.rejects(client.node3.vp({ start: 20, end: 10 }), /HTTP 400: end must be after start/);
});

test("Node 1 WebSocket URL uses configured service origin", () => {
  const client = clientModule.createClient({ NODE1_BASE_URL: "https://market.example", NODE3_BASE_URL: "https://node3.example" }, async () => {});
  assert.equal(client.websocketUrl("node1"), "wss://market.example/ws");
  assert.equal(client.websocketUrl("node3"), "wss://node3.example/ws");
});
