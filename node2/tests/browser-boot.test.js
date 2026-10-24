// browser-boot.test.js — boots the actual browser bundle (public/src/main.js)
// in a simulated browser environment against the running server and verifies
// every panel renders from live (mock-mode) data.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SERVER = path.join(__dirname, "../serve.mjs");

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let child;
let base;
const trackedSockets = [];
const trackedIntervals = [];

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

  // Simulated browser globals BEFORE the bundle loads.
  const { installDomShim } = await import("./helpers/dom-shim.js");
  const shim = installDomShim({ innerWidth: 1500 });
  const app = shim.document.createElement("div");
  app.id = "app";
  const canvas = shim.document.createElement("canvas");
  canvas.id = "chart";
  const byId = { app, chart: canvas };
  shim.document.getElementById = (id) => byId[id] || null;
  shim.document.querySelector = () => null;
  globalThis.window.__RUNTIME_CONFIG__ = { mock: true, demo: true };
  globalThis.location = { search: "", protocol: "http:", host: "localhost:3000" };

  // Browsers resolve relative URLs against the page origin; Node does not.
  const realFetch = globalThis.fetch;
  globalThis.fetch = (url, opts) => realFetch(String(url).startsWith("/") ? base + url : url, {
    ...opts,
    headers: { Connection: "close", ...(opts && opts.headers) },
  });

  const WSImpl = (await import("ws")).default;
  class BrowserWS extends WSImpl {
    constructor(url, protocols, options) {
      super(String(url).startsWith("/") ? base.replace("http", "ws") + url : url, protocols, options);
      trackedSockets.push(this);
    }
  }
  globalThis.window.WebSocket = BrowserWS;

  // Track timers so the process can exit cleanly after the test.
  const realSetInterval = globalThis.setInterval;
  globalThis.setInterval = (fn, ms, ...args) => {
    const id = realSetInterval(fn, ms, ...args);
    trackedIntervals.push(id);
    return id;
  };
});

after(() => {
  for (const id of trackedIntervals) clearInterval(id);
  for (const s of trackedSockets) {
    try { s.close(); } catch (err) { /* ignore */ }
  }
  if (child) child.kill("SIGTERM");
  // Undici keeps a keep-alive socket open, which would hang the test
  // process; the file's work is done here, so exit cleanly.
  process.exit(0);
});

test("browser bundle boots and renders every panel from the feed", async () => {
  await import("../public/src/main.js");
  // Wait for REST seeds + WS replay + the first panel render.
  await sleep(3000);

  const shell = document.getElementById("app");
  const text = shell.textContent;

  const checks = [
    ["dashboard title", "Node 2 · Demo Market Dashboard"],
    ["demo badge", "DEMO"],
    ["strategy panel with confidence gauge", "Confidence"],
    ["demo account panel", "Demo Account"],
    ["execution panel", "Execution · Node 4"],
    ["venue state", "demo account"],
    ["open trades panel", "Open Trades"],
    ["mt5 positions panel", "MT5 Positions"],
    ["mt5 history panel", "MT5 History"],
    ["bridge event log", "Bridge Event Log"],
    ["levels panel", "Levels · POC/VAH/VAL"],
    ["tick volume panel", "Tick Volume"],
    ["calendar panel", "Calendar"],
    ["performance panel", "Performance"],
    ["trade history panel", "Trade History"],
    ["read-only footer", "No trading decisions, no order submission"],
  ];
  for (const [name, needle] of checks) {
    assert.match(text, new RegExp(needle), `panel missing: ${name}`);
  }

  // Lifecycle badges from the Node 4 open-trades feed.
  assert.match(text, /Accepted/, "accepted lifecycle badge rendered");
  assert.match(text, /Unknown/, "unknown lifecycle badge rendered");
  assert.match(text, /Filled/, "filled lifecycle badge rendered");

  // A disconnected feed never presents data as fresh: the header shows the
  // live/freshness pills from the transport state.
  assert.match(text, /freshness--|Live|Connecting|Reconnecting|Disconnected/);
});
