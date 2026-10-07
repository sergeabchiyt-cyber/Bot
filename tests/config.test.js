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

test("default hosts map only to the documented public route families", () => {
  const config = loadConfig();
  assert.equal(config.endpoints.strategyWs, "wss://strategy-southeastasia-sng-main.onrender.com/ws");
  assert.equal(config.endpoints.strategyDiagnostics, "https://strategy-southeastasia-sng-main.onrender.com/diagnostics");
  assert.equal(config.endpoints.executionWs, "wss://execution-southeastasia-sng-main.onrender.com/ws");
  assert.equal(config.endpoints.executionDiagnostics, "https://execution-southeastasia-sng-main.onrender.com/diagnostics");
  assert.equal(config.endpoints.executionOpenTrades, "https://execution-southeastasia-sng-main.onrender.com/open-trades");
  assert.equal(config.endpoints.executionDeriv, "https://execution-southeastasia-sng-main.onrender.com/deriv");
  assert.equal(config.endpoints.mt5Status, "https://execution-southeastasia-sng-main.onrender.com/mt5/status");
  assert.ok(Object.values(config.endpoints).every((url) => !url.includes("/mt5/bridge") && !url.includes("/mt5/control")));
});

test("runtime overrides change only service origins; an empty host disables just that service", () => {
  const config = loadConfig({
    MARKET_HOST: "http://127.0.0.1:8081/",
    STRATEGY_HOST: "",
    EXECUTION_HOST: "ws://localhost:8082",
  });
  assert.equal(config.endpoints.marketWs, "ws://127.0.0.1:8081/ws");
  assert.equal(config.endpoints.candles, "http://127.0.0.1:8081/candles");
  assert.equal(config.endpoints.strategyWs, "");
  assert.equal(config.endpoints.strategyDiagnostics, "");
  assert.equal(config.endpoints.executionWs, "ws://localhost:8082/ws");
  assert.equal(config.endpoints.executionDiagnostics, "http://localhost:8082/diagnostics");
});

test("rejects host overrides that embed credentials, queries, fragments, or custom paths", () => {
  const config = loadConfig({
    MARKET_HOST: "https://user:password@market.example",
    STRATEGY_HOST: "https://strategy.example/custom/path",
    EXECUTION_HOST: "https://execution.example/?token=secret",
  });
  assert.equal(config.endpoints.marketWs, "");
  assert.equal(config.endpoints.strategyDiagnostics, "");
  assert.equal(config.endpoints.executionWs, "");
});
