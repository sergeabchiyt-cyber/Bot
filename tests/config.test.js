"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

function loadConfig(runtime = {}) {
  global.window = { App: {}, XAUUSD_CONFIG: runtime };
  const path = require.resolve("../js/config.js");
  delete require.cache[path];
  require(path);
  return global.window.App.config;
}

test("all endpoints point at Node 1's documented public routes", () => {
  const config = loadConfig();
  const host = "https://engine-southeastasia-sng-main.onrender.com";
  assert.equal(config.endpoints.ws, "wss://engine-southeastasia-sng-main.onrender.com/ws");
  assert.equal(config.endpoints.strategy, `${host}/strategy`);
  assert.equal(config.endpoints.demo, `${host}/demo`);
  assert.equal(config.endpoints.status, `${host}/status`);
  assert.equal(config.endpoints.levels, `${host}/levels`);
  assert.equal(config.endpoints.calendar, `${host}/calendar`);
  assert.equal(config.endpoints.candles, `${host}/candles`);
  assert.equal(config.endpoints.tickVolume, `${host}/tick-volume`);
  assert.equal(config.MARKET_HOST, host);
  assert.equal(config.STRATEGY_HOST, undefined);
  assert.equal(config.EXECUTION_HOST, undefined);
  // Node 4 is a second, independent service: diagnostics only, no write path.
  assert.equal(config.NODE4_HOST, "https://execution-southeastasia-sng-main.onrender.com");
  assert.equal(config.endpoints.executionDiagnostics, "https://execution-southeastasia-sng-main.onrender.com/diagnostics");
  assert.equal(config.endpoints.executionWs, "wss://execution-southeastasia-sng-main.onrender.com/ws");
  assert.deepEqual(config.execution.topics, ["diagnostics"]);
});

test("the single socket subscribes to strategy and demo and never to bubbles", () => {
  const config = loadConfig();
  assert.deepEqual(config.socket.topics, [
    "levels", "candle", "tick_volume", "strategy", "demo", "calendar", "status",
  ]);
  assert.equal(config.socket.topics.includes("bubbles"), false);
  assert.equal(config.socket.heartbeatEveryMs, 30000);
  assert.equal(config.socket.pollFallbackMs, 5000);
  assert.equal(config.strategy.historyLimit, 120);
});

test("a runtime host override changes only the Node 1 origin", () => {
  const config = loadConfig({ MARKET_HOST: "http://127.0.0.1:8081/" });
  assert.equal(config.endpoints.ws, "ws://127.0.0.1:8081/ws");
  assert.equal(config.endpoints.strategy, "http://127.0.0.1:8081/strategy");
  assert.equal(config.endpoints.demo, "http://127.0.0.1:8081/demo");
  assert.equal(config.endpoints.status, "http://127.0.0.1:8081/status");
});

test("an empty Node 1 host disables every Node 1 endpoint", () => {
  const config = loadConfig({ MARKET_HOST: "" });
  const node1 = ["ws", "candles", "tickVolume", "levels", "calendar", "strategy", "demo", "status"];
  for (const key of node1) assert.equal(config.endpoints[key], "", key);
  // Node 4 is configured separately; it is not disabled by MARKET_HOST.
  assert.equal(config.endpoints.executionDiagnostics, "https://execution-southeastasia-sng-main.onrender.com/diagnostics");
});

test("an empty Node 4 host disables only the execution endpoints", () => {
  const config = loadConfig({ NODE4_HOST: "" });
  assert.equal(config.endpoints.executionDiagnostics, "");
  assert.equal(config.endpoints.executionWs, "");
  assert.equal(config.execution.diagnostics, "");
  assert.equal(config.endpoints.ws, "wss://engine-southeastasia-sng-main.onrender.com/ws");
});

test("a Node 4 host override changes only the execution origin", () => {
  const config = loadConfig({ NODE4_HOST: "http://127.0.0.1:8090/" });
  assert.equal(config.endpoints.executionDiagnostics, "http://127.0.0.1:8090/diagnostics");
  assert.equal(config.endpoints.executionWs, "ws://127.0.0.1:8090/ws");
  assert.equal(config.execution.diagnostics, "http://127.0.0.1:8090/diagnostics");
  assert.equal(config.execution.ws, "ws://127.0.0.1:8090/ws");
  assert.equal(config.endpoints.strategy, "https://engine-southeastasia-sng-main.onrender.com/strategy");
});

test("rejects Node 4 overrides embedding credentials, queries, fragments or paths", () => {
  for (const value of [
    "https://user:password@execution.example",
    "https://execution.example/custom/path",
    "https://execution.example/?token=secret",
    "https://execution.example/#fragment",
  ]) {
    const config = loadConfig({ NODE4_HOST: value });
    assert.equal(config.endpoints.executionDiagnostics, "", value);
    assert.equal(config.endpoints.executionWs, "", value);
  }
});

test("rejects overrides embedding credentials, queries, fragments, or paths", () => {
  for (const value of [
    "https://user:password@market.example",
    "https://market.example/custom/path",
    "https://market.example/?token=secret",
    "https://market.example/#fragment",
  ]) {
    const config = loadConfig({ MARKET_HOST: value });
    assert.equal(config.endpoints.ws, "", value);
  }
});

test("legacy multi-service overrides are ignored", () => {
  const config = loadConfig({ STRATEGY_HOST: "https://strategy.example", EXECUTION_HOST: "https://execution.example" });
  assert.equal(config.endpoints.strategy, "https://engine-southeastasia-sng-main.onrender.com/strategy");
  assert.equal(config.endpoints.demo, "https://engine-southeastasia-sng-main.onrender.com/demo");
});
