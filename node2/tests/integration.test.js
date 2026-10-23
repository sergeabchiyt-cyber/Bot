// integration.test.js — end-to-end server test: static assets, REST
// resources (mock mode), read-only enforcement, and the WebSocket replay
// order for both feeds.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import WebSocket from "ws";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SERVER = path.join(__dirname, "../serve.mjs");

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function getJson(base, urlPath) {
  const res = await fetch(base + urlPath);
  const text = await res.text();
  let data = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch (err) {
    data = text;
  }
  return { status: res.status, data };
}

function connectWs(base, pathname, subscribeTopics) {
  return new Promise((resolve, reject) => {
    const frames = [];
    const ws = new WebSocket(base.replace("http", "ws") + pathname);
    const timer = setTimeout(() => {
      ws.close();
      reject(new Error("ws timed out"));
    }, 8000);
    ws.on("open", () => {
      ws.send(JSON.stringify({ type: "subscribe", topics: subscribeTopics }));
    });
    ws.on("message", (d) => {
      frames.push(JSON.parse(d.toString()));
    });
    ws.on("error", (err) => {
      clearTimeout(timer);
      reject(err);
    });
    resolve({
      ws,
      frames,
      waitFor: (count, timeoutMs = 6000) => new Promise((res2, rej2) => {
        const check = () => {
          if (frames.length >= count) {
            clearTimeout(timer);
            ws.close();
            res2(frames);
          } else {
            setTimeout(check, 50);
          }
        };
        setTimeout(() => {
          clearTimeout(timer);
          ws.close();
          rej2(new Error(`expected ${count} frames, got ${frames.length}`));
        }, timeoutMs);
        check();
      }),
    });
  });
}

let child;
let base;

before(async () => {
  child = spawn(process.execPath, [SERVER], {
    cwd: path.dirname(SERVER),
    env: { ...process.env, PORT: "0", NODE1_MODE: "mock", NODE4_MODE: "mock" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (chunk) => {
    const m = chunk.toString().match(/http:\/\/localhost:(\d+)/);
    if (m) child.__port = Number(m[1]);
  });
  const deadline = Date.now() + 10000;
  while (!child.__port && Date.now() < deadline) {
    await sleep(100);
  }
  if (!child.__port) throw new Error("server did not start");
  base = `http://localhost:${child.__port}`;
});

after(() => {
  if (child) child.kill("SIGTERM");
});

test("serves the dashboard shell", async () => {
  const res = await fetch(base + "/");
  assert.equal(res.status, 200);
  const html = await res.text();
  assert.match(html, /Node 2 · Demo Market Dashboard/);
  assert.match(html, /type="module"/);
});

test("serves runtime config advertising mock mode", async () => {
  const res = await fetch(base + "/runtime-config.js");
  assert.equal(res.status, 200);
  const text = await res.text();
  assert.match(text, /window\.__RUNTIME_CONFIG__\s*=\s*/);
  assert.match(text, /"mock":true/);
});

test("node1 REST resources all respond with contract shapes", async () => {
  const status = await getJson(base, "/api/node1/status");
  assert.equal(status.status, 200);
  assert.equal(status.data.candles.interval, "15m");
  assert.ok(Number.isFinite(status.data.candles.edge_lag_minutes));

  const levels = await getJson(base, "/api/node1/levels");
  assert.equal(levels.status, 200);
  assert.ok(Array.isArray(levels.data));
  assert.ok(levels.data[0].window && levels.data[0].poc != null);

  const candles = await getJson(base, "/api/node1/candles");
  assert.equal(candles.status, 200);
  assert.ok(Array.isArray(candles.data));
  assert.ok(candles.data[0].time && candles.data[0].close != null);

  const strategy = await getJson(base, "/api/node1/strategy");
  assert.equal(strategy.status, 200);
  assert.equal(strategy.data.symbol, "XAUUSD");
  assert.ok(strategy.data.last.reasons.length > 0);

  const demo = await getJson(base, "/api/node1/demo");
  assert.equal(demo.status, 200);
  assert.equal(demo.data.mode, "demo");
  assert.ok(demo.data.account.equity != null);
});

test("node4 REST resources all respond with contract shapes", async () => {
  const diag = await getJson(base, "/api/node4/diagnostics");
  assert.equal(diag.status, 200);
  assert.ok(diag.data.work && diag.data.node3);

  const trades = await getJson(base, "/api/node4/open-trades");
  assert.equal(trades.status, 200);
  assert.ok(Array.isArray(trades.data));

  const account = await getJson(base, "/api/node4/mt5/account");
  assert.equal(account.status, 200);
  assert.equal(account.data.account_type, "demo");

  const status = await getJson(base, "/api/node4/mt5/status");
  assert.equal(status.status, 200);
  assert.ok(status.data.orders_unknown != null);
});

test("unknown paths 404 and non-GET methods are refused (read-only)", async () => {
  const notFound = await fetch(base + "/api/node1/nonexistent");
  assert.equal(notFound.status, 404);

  const post = await fetch(base + "/api/node1/status", { method: "POST" });
  assert.equal(post.status, 405);

  const put = await fetch(base + "/api/node4/diagnostics", { method: "PUT" });
  assert.equal(put.status, 405);
});

test("node1 WS replay order matches the contract", async () => {
  const { waitFor } = await connectWs(base, "/api/node1/ws",
    ["levels", "candle", "tick_volume", "strategy", "demo", "calendar", "status"]);
  const frames = await waitFor(7);
  const order = frames.map((f) => f.type);
  assert.deepEqual(order, ["levels", "candle", "tick_volume", "calendar", "strategy", "demo", "status"]);
});

test("node4 WS sends exactly 7 initial frames in the documented order", async () => {
  const { waitFor } = await connectWs(base, "/api/node4/ws",
    ["mt5_account", "mt5_positions", "mt5_history", "bridge_status", "bridge_event", "open_trades", "diagnostics"]);
  const frames = await waitFor(7);
  const order = frames.map((f) => f.topic);
  assert.deepEqual(order, [
    "diagnostics", "open_trades", "deriv_account",
    "mt5_account", "mt5_positions", "mt5_history", "bridge_status",
  ]);
});

test("node4 WS streams diagnostics refreshes", async () => {
  const { waitFor } = await connectWs(base, "/api/node4/ws", ["diagnostics"]);
  const frames = await waitFor(8); // 7 initial + 1 refresh
  assert.ok(frames[7].diagnostics.timestamp);
});
