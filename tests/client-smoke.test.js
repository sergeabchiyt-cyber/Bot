"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const client = require("../js/client.js");

const N1 = "https://node1.example";
const N3 = "https://node3.example";
const requests = [];

function fixture(relative) {
  return JSON.parse(fs.readFileSync(path.join(__dirname, "fixtures", relative), "utf8"));
}

function jsonResponse(body, status = 200) {
  const text = JSON.stringify(body);
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
    text: async () => text,
  };
}

function installFetch() {
  global.fetch = async (url, options) => {
    const parsed = new URL(url);
    requests.push({ url: String(url), options, host: parsed.host, path: parsed.pathname, query: Object.fromEntries(parsed.searchParams) });
    if (parsed.host === "node1.example") {
      if (parsed.pathname === "/candles") {
        return jsonResponse(fixture(`node1/candles-${parsed.searchParams.get("interval")}.json`));
      }
      if (parsed.pathname === "/tick-volume") return jsonResponse(fixture("node1/tick-volume.json"));
      if (parsed.pathname === "/calendar") return jsonResponse(fixture("node1/calendar.json"));
      if (parsed.pathname === "/status") return jsonResponse(fixture("node1/status.json"));
    }
    if (parsed.host === "node3.example") {
      if (parsed.pathname === "/health") return jsonResponse(fixture("node3/health.json"));
      if (parsed.pathname === "/levels") return jsonResponse(fixture("node3/levels.json"));
      if (parsed.pathname === "/drawings") return jsonResponse(fixture("node3/drawings.json"));
      if (parsed.pathname === "/scanning") return jsonResponse(fixture("node3/scanning.json"));
      if (parsed.pathname === "/signals") return jsonResponse(fixture("node3/signals.json"));
      if (parsed.pathname === "/diagnostics") return jsonResponse(fixture("node3/diagnostics.json"));
      if (parsed.pathname === "/vp") {
        if (parsed.searchParams.get("start") === "400") {
          return jsonResponse(fixture("node3/vp-error.json"), 400);
        }
        return jsonResponse(fixture("node3/vp-PW.json"));
      }
    }
    return jsonResponse({ error: "not found" }, 404);
  };
}

client.configure({
  baseUrls: { node1: N1, node3: N3 },
  services: { node1: { topics: [] }, node3: { topics: [] } },
  requestTimeoutMs: 1000,
});
installFetch();

test("recorded Node 3 snapshots and every Node 1 candle interval load through one GET client", async () => {
  requests.length = 0;
  const [health, levels, drawings, scanning, signals, diagnostics, vp, c1, c15, c1h, tickVolume, calendar, status] = await Promise.all([
    client.getHealth(),
    client.getLevels(),
    client.getDrawings(),
    client.getScanning(),
    client.getSignals(),
    client.getDiagnostics(),
    client.getVp({ window: "PW", rows: 128 }),
    client.getCandles("1m"),
    client.getCandles("15m"),
    client.getCandles("1h"),
    client.getTickVolume(),
    client.getCalendar(),
    client.getStatus(),
  ]);

  assert.equal(health.contractVersion, "2.0");
  assert.equal(levels.find((level) => level.window === "PS").vah, undefined);
  assert.equal(drawings.keys.length, 5);
  assert.equal(scanning.setups.length, 2);
  assert.equal(signals.pending_count, 1);
  assert.equal(diagnostics.lastViolation, "candle frame carried an unknown interval");
  assert.equal(vp.histogram.length, 128);
  assert.equal(c1.length, 5);
  assert.equal(c15.length, 4);
  assert.equal(c1h.length, 4);
  assert.equal(tickVolume.bars.length, 4);
  assert.equal(calendar.events.length, 2);
  assert.equal(status.state, "live");

  assert.equal(requests.length, 13);
  for (const request of requests) {
    assert.equal(request.options.method, "GET");
    assert.equal(request.options.cache, "no-store");
    assert.equal(request.options.credentials, "omit");
    assert.equal(request.options.body, undefined);
    assert.deepEqual(request.options.headers, { Accept: "application/json" });
  }
  assert.deepEqual(requests.filter((r) => r.path === "/candles").map((r) => r.query.interval).sort(), ["15m", "1h", "1m"]);
  assert.ok(requests.some((r) => r.path === "/vp" && r.query.window === "PW" && r.query.rows === "128"));
  assert.ok(requests.some((r) => r.path === "/levels" && r.host === "node3.example"));
  assert.equal(requests.some((r) => r.path === "/levels" && r.host === "node1.example"), false);
  assert.ok(client.getLastGood("node3:health"));
  assert.ok(client.getLastGood("node1:candles"));
});

test("custom /vp ranges are encoded and an HTTP 400 error body is surfaced", async () => {
  requests.length = 0;
  const start = 1792584000000;
  const end = 1793016000000;
  const custom = await client.getVp({ start, end, rows: 64 });
  assert.equal(custom.histogram.length, 128);
  const vpRequest = requests.find((request) => request.path === "/vp");
  assert.equal(vpRequest.query.start, String(start));
  assert.equal(vpRequest.query.end, String(end));
  assert.equal(vpRequest.query.rows, "64");

  await assert.rejects(
    client.getVp({ start: 400, end: 500 }),
    (error) => {
      assert.equal(error.status, 400);
      assert.equal(error.body.error, "start and end must be epoch milliseconds");
      assert.match(error.message, /start and end/);
      return true;
    }
  );

  await assert.rejects(client.getVp({ start: end, end: start }), /start < end/);
  await assert.rejects(client.getVp({ rows: 1 }), /rows must be an integer from 2 to 5000/);
  await assert.rejects(client.getVp({ window: "bad" }), /window must be PW, PS, or CW/);
});

test("URL construction is service-scoped and has no Node 1 levels route", () => {
  assert.equal(client.urlFor("node3", "levels"), `${N3}/levels`);
  assert.equal(client.urlFor("node1", "candles", { interval: "1h" }), `${N1}/candles?interval=1h`);
  assert.equal(client.wsUrlFor("node3"), "wss://node3.example/ws");
  assert.equal(client.wsUrlFor("node1"), "wss://node1.example/ws");
  assert.throws(() => client.urlFor("node1", "levels"));
  assert.throws(() => client.urlFor("node1", "execution"));
  assert.throws(() => client.urlFor("node3", "execution"));
  assert.deepEqual(Object.keys(client.ROUTES.node1), ["candles", "tickVolume", "calendar", "status", "ws"]);
  assert.deepEqual(Object.keys(client.ROUTES.node3), ["health", "levels", "drawings", "vp", "scanning", "signals", "diagnostics", "ws"]);
});
