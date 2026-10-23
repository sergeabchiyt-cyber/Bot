#!/usr/bin/env node
// serve.mjs — Node 2 dev/demo server.
//
// - Serves the static dashboard (public/) on 0.0.0.0
// - /api/node1/* and /api/node4/* — REST: proxies to the live services
//   (GET-only, 405 on other methods) or answers from the contract-shaped mock
// - /api/node1/ws and /api/node4/ws — WebSocket: proxies to the live
//   services or runs the mock session
// - /runtime-config.js — window.__RUNTIME_CONFIG__ consumed by config.js
//
// Modes per service: NODE1_MODE=mock|live, NODE4_MODE=mock|live
// Bases:         NODE1_BASE, NODE4_BASE (http(s) URLs)
//
// The dashboard is read-only: this server forwards nothing but GETs and
// subscribe-only frames, and never holds tokens.

import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { WebSocketServer, WebSocket } from "ws";
import {
  makeCandles, makeLevels, makeTickBars, makeCalendar, makeStatus,
  makeStrategy, makeDemo, mockNode1Session, mockNode4Session,
  makeDiagnostics, makeOpenTrades, makeMt5Account, makeMt5Positions,
  makeMt5History, makeMt5Status, makeDerivAccount,
} from "./mock/feed.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PUBLIC_DIR = path.join(__dirname, "public");

const PORT = Number(process.env.PORT || 3000);
const HOST = process.env.HOST || "0.0.0.0";

const NODE1_MODE = (process.env.NODE1_MODE || "mock").toLowerCase();
const NODE4_MODE = (process.env.NODE4_MODE || "mock").toLowerCase();
const NODE1_BASE = (process.env.NODE1_BASE || "https://engine-southeastasia-sng-main.onrender.com").replace(/\/+$/, "");
const NODE4_BASE = (process.env.NODE4_BASE || "https://execution-southeastasia-sng-main.onrender.com").replace(/\/+$/, "");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
  ".png": "image/png",
};

function json(res, status, data) {
  const body = JSON.stringify(data);
  res.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Cache-Control": "no-store",
    "Access-Control-Allow-Origin": "*",
  });
  res.end(body);
}

function now() {
  return Math.floor(Date.now() / 1000);
}

// ---- mock REST ---------------------------------------------------------------

function mockNode1(req, res, pathname) {
  const t = now();
  const price = 2655 + ((Math.floor(t / 30) % 12) - 6);
  switch (pathname) {
    case "/status":
      return json(res, 200, makeStatus(t, makeCandles(t)));
    case "/levels":
      return json(res, 200, makeLevels(t, price));
    case "/candles":
      return json(res, 200, makeCandles(t));
    case "/tick-volume":
      return json(res, 200, makeTickBars(t));
    case "/calendar":
      return json(res, 200, makeCalendar(t));
    case "/strategy":
      return json(res, 200, makeStrategy(t, price));
    case "/demo":
      return json(res, 200, makeDemo(t, price));
    default:
      return json(res, 404, { error: "unknown mock node1 path" });
  }
}

function mockNode4(req, res, pathname) {
  const t = now();
  switch (pathname) {
    case "/diagnostics":
    case "/status":
      return json(res, 200, makeDiagnostics(t));
    case "/open-trades":
    case "/trades":
      return json(res, 200, makeOpenTrades(t));
    case "/deriv":
    case "/account":
      return json(res, 200, makeDerivAccount(t));
    case "/mt5/account":
      return json(res, 200, makeMt5Account(t));
    case "/mt5/positions":
      return json(res, 200, makeMt5Positions(t));
    case "/mt5/history":
      return json(res, 200, makeMt5History(t));
    case "/mt5/status":
      return json(res, 200, makeMt5Status(t));
    default:
      return json(res, 404, { error: "unknown mock node4 path" });
  }
}

// ---- live REST proxy ---------------------------------------------------------

function proxyRest(req, res, base, pathname) {
  const target = new URL(base + pathname);
  const options = {
    hostname: target.hostname,
    port: target.port || (target.protocol === "https:" ? 443 : 80),
    path: target.pathname + target.search,
    method: req.method,
    headers: { ...req.headers, host: target.host },
  };
  const upstream = http.request(options, (upRes) => {
    res.writeHead(upRes.statusCode || 502, {
      "Content-Type": upRes.headers["content-type"] || "application/json",
      "Cache-Control": "no-store",
    });
    upRes.pipe(res);
  });
  upstream.on("error", (err) => {
    json(res, 502, { error: `upstream error: ${err.message}` });
  });
  req.pipe(upstream);
}

// ---- static files ------------------------------------------------------------

function serveStatic(req, res, pathname) {
  if (req.method !== "GET" && req.method !== "HEAD") {
    return json(res, 405, { error: "method not allowed (dashboard is read-only)" });
  }
  const rel = pathname === "/" ? "index.html" : pathname.slice(1);
  const filePath = path.normalize(path.join(PUBLIC_DIR, rel));
  if (!filePath.startsWith(PUBLIC_DIR)) {
    return json(res, 403, { error: "forbidden" });
  }
  fs.readFile(filePath, (err, data) => {
    if (err) {
      return json(res, 404, { error: "not found" });
    }
    const ext = path.extname(filePath).toLowerCase();
    res.writeHead(200, {
      "Content-Type": MIME[ext] || "application/octet-stream",
      "Cache-Control": "no-store",
    });
    res.end(data);
  });
}

// ---- server -----------------------------------------------------------------

const server = http.createServer((req, res) => {
  let pathname;
  try {
    pathname = new URL(req.url, "http://localhost").pathname;
  } catch (err) {
    return json(res, 400, { error: "bad request" });
  }

  if (pathname === "/runtime-config.js") {
    res.writeHead(200, { "Content-Type": "text/javascript; charset=utf-8", "Cache-Control": "no-store" });
    res.end(`window.__RUNTIME_CONFIG__ = ${JSON.stringify({
      node1Rest: "/api/node1",
      node4Rest: "/api/node4",
      node1Ws: "/api/node1/ws",
      node4Ws: "/api/node4/ws",
      mock: NODE1_MODE === "mock" && NODE4_MODE === "mock",
      demo: true,
    })};\n`);
    return;
  }

  if (pathname.startsWith("/api/node1/")) {
    const suffix = pathname.slice("/api/node1".length);
    if (req.method !== "GET" && req.method !== "HEAD") {
      return json(res, 405, { error: "method not allowed (read-only)" });
    }
    if (NODE1_MODE === "mock") return mockNode1(req, res, suffix);
    return proxyRest(req, res, NODE1_BASE, suffix);
  }

  if (pathname.startsWith("/api/node4/")) {
    const suffix = pathname.slice("/api/node4".length);
    if (req.method !== "GET" && req.method !== "HEAD") {
      return json(res, 405, { error: "method not allowed (read-only)" });
    }
    if (NODE4_MODE === "mock") return mockNode4(req, res, suffix);
    return proxyRest(req, res, NODE4_BASE, suffix);
  }

  return serveStatic(req, res, pathname);
});

// ---- WebSocket upgrade --------------------------------------------------------

const wss = new WebSocketServer({ noServer: true });

function proxyWs(client, targetUrl) {
  let closed = false;
  const upstream = new WebSocket(targetUrl, { perMessageDeflate: false });
  const relay = (from, to) => (data) => {
    if (!closed && to.readyState === WebSocket.OPEN) to.send(data);
  };
  upstream.on("message", relay(upstream, client));
  upstream.on("close", () => {
    if (!closed) {
      closed = true;
      try { client.close(); } catch (err) { /* ignore */ }
    }
  });
  upstream.on("error", () => {
    if (!closed) {
      closed = true;
      try { client.close(); } catch (err) { /* ignore */ }
    }
  });
  client.on("message", relay(client, upstream));
  client.on("close", () => {
    if (!closed) {
      closed = true;
      try { upstream.close(); } catch (err) { /* ignore */ }
    }
  });
}

server.on("upgrade", (req, socket, head) => {
  let pathname;
  try {
    pathname = new URL(req.url, "http://localhost").pathname;
  } catch (err) {
    socket.destroy();
    return;
  }
  const isNode1 = pathname === "/api/node1/ws";
  const isNode4 = pathname === "/api/node4/ws";
  if (!isNode1 && !isNode4) {
    socket.destroy();
    return;
  }
  wss.handleUpgrade(req, socket, head, (client) => {
    if (isNode1 && NODE1_MODE === "mock") {
      mockNode1Session(client, { intervalSec: 900 });
    } else if (isNode4 && NODE4_MODE === "mock") {
      mockNode4Session(client);
    } else if (isNode1) {
      proxyWs(client, "wss://" + new URL(NODE1_BASE).host + "/ws");
    } else {
      proxyWs(client, "wss://" + new URL(NODE4_BASE).host + "/ws");
    }
  });
});

server.listen(PORT, HOST, () => {
  const bound = server.address();
  const port = bound && bound.port ? bound.port : PORT;
  console.log(`Node 2 dashboard: http://localhost:${port} (mock: node1=${NODE1_MODE}, node4=${NODE4_MODE})`);
});
