// security.test.js — static guarantees about the shipped browser code:
// no tokens, no broker control endpoints, no localhost targets, GET-only.

import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PUBLIC = path.join(__dirname, "../public");

function browserFiles() {
  const out = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (/\.(js|html|css)$/.test(entry.name)) out.push(full);
    }
  };
  walk(PUBLIC);
  return out;
}

const ALL = browserFiles().map((f) => ({
  file: f,
  rel: path.relative(PUBLIC, f),
  content: fs.readFileSync(f, "utf8"),
}));

test("browser code contains no private tokens", () => {
  const forbidden = [
    "NODE4_SHARED_TOKEN",
    "MT5_BRIDGE_TOKEN",
    "MT5_CONTROL_TOKEN",
    "bridge_token",
    "control_token",
    "shared_token",
    "api_key",
    "Authorization",
    "x-bridge-token",
    "x-node4-token",
  ];
  for (const { rel, content } of ALL) {
    for (const token of forbidden) {
      assert.doesNotMatch(content, new RegExp(token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
        `${rel} must not reference ${token}`);
    }
  }
});

test("browser code never targets localhost or loopback addresses", () => {
  const loopbackUrl = /(wss?|https?):\/\/(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]|::1)(:\d+)?/;
  for (const { rel, content } of ALL) {
    assert.doesNotMatch(content, loopbackUrl, `${rel} must not target loopback`);
  }
});

test("config.js actively rejects loopback overrides", () => {
  const cfg = ALL.find((f) => f.rel.endsWith("src/config.js"));
  assert.ok(cfg, "config.js exists");
  assert.match(cfg.content, /isLoopbackHost/);
  assert.match(cfg.content, /localhost/); // the guard list itself is expected
  assert.match(cfg.content, /rejected/);
});

test("browser code never calls /mt5/control or /mt5/bridge", () => {
  for (const { rel, content } of ALL) {
    assert.doesNotMatch(content, /\/mt5\/control/, `${rel} must not reference /mt5/control`);
    assert.doesNotMatch(content, /\/mt5\/bridge/, `${rel} must not reference /mt5/bridge`);
  }
});

test("REST layer is GET-only", () => {
  const rest = ALL.find((f) => f.rel.endsWith("src/api/rest.js"));
  assert.ok(rest, "rest.js exists");
  assert.match(rest.content, /method: "GET"/);
  assert.doesNotMatch(rest.content, /method: "POST"/);
  assert.doesNotMatch(rest.content, /method: "PUT"/);
  assert.doesNotMatch(rest.content, /method: "DELETE"/);
  assert.doesNotMatch(rest.content, /method: "PATCH"/);
});

test("browser code issues no writes anywhere", () => {
  for (const { rel, content } of ALL) {
    assert.doesNotMatch(content, /method:\s*["'](POST|PUT|PATCH|DELETE)["']/,
      `${rel} must not perform write requests`);
  }
});

test("no WebSocket target is hardcoded to a broker or exchange host", () => {
  // Node 1/Node 4 WS endpoints must come from config/endpoints, not literals.
  for (const { rel, content } of ALL) {
    assert.doesNotMatch(content, /wss:\/\/[a-z0-9.-]+\.onrender\.com/,
      `${rel} must not hardcode a service host`);
  }
});

test("server refuses non-GET on the proxy (read-only enforcement)", () => {
  const serve = fs.readFileSync(path.join(__dirname, "../serve.mjs"), "utf8");
  assert.match(serve, /405/);
  assert.match(serve, /read-only/);
});

test("index.html contains no inline secrets or tokens", () => {
  const html = ALL.find((f) => f.rel === "index.html");
  assert.ok(html);
  assert.doesNotMatch(html.content, /token/i);
});

test("mock data is clearly labeled synthetic", () => {
  const mock = fs.readFileSync(path.join(__dirname, "../mock/feed.js"), "utf8");
  assert.match(mock, /synthetic|mock/i);
  const demo = mock;
  assert.match(demo, /Simulated demo engine/);
});
