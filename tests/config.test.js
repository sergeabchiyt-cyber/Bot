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
  // The only service Node 2 knows about is Node 1.
  assert.equal(config.MARKET_HOST, host);
  assert.equal(config.STRATEGY_HOST, undefined);
  assert.equal(config.EXECUTION_HOST, undefined);
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

test("an empty host disables every endpoint", () => {
  const config = loadConfig({ MARKET_HOST: "" });
  assert.equal(config.endpoints.ws, "");
  assert.ok(Object.values(config.endpoints).every((url) => url === ""));
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
