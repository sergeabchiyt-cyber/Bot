"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

function loadConfig(runtime = {}) {
  global.window = { XAUUSD_CONFIG: runtime };
  const path = require.resolve("../js/config.js");
  delete require.cache[path];
  return require(path);
}

test("one config exposes exactly the Node 1 and Node 3 base URLs", () => {
  const config = loadConfig();
  assert.equal(config.baseUrls.node1, "https://engine-southeastasia-sng-main.onrender.com");
  assert.equal(config.baseUrls.node3, "https://strategy-api-vp-levels.onrender.com");
  assert.equal(config.services.node1.base, config.baseUrls.node1);
  assert.equal(config.services.node3.base, config.baseUrls.node3);
  assert.equal(config.expectedContractVersion, "2.0");
  assert.equal(config.endpoints, undefined, "paths belong to the typed client, not config");
  assert.equal(config.NODE4_HOST, undefined);
});

test("public topics are split by service and contain no private topic", () => {
  const config = loadConfig();
  assert.deepEqual(config.services.node1.topics, ["candle", "tick_volume", "calendar", "status"]);
  assert.deepEqual(config.services.node3.topics, ["levels", "drawing", "vp", "scanning", "signals", "diagnostics"]);
  for (const service of ["node1", "node3"]) {
    for (const topic of config.services[service].topics) {
      assert.doesNotMatch(topic, /trade_intent|execution|private/i);
    }
  }
  assert.deepEqual(config.chart.intervals, ["1m", "15m", "1h"]);
});

test("host overrides are origin-only and independent", () => {
  const config = loadConfig({
    NODE1_HOST: "https://node1.example",
    NODE3_HOST: "https://node3.example",
  });
  assert.equal(config.baseUrls.node1, "https://node1.example");
  assert.equal(config.baseUrls.node3, "https://node3.example");
  assert.equal(config.MARKET_HOST, "https://node1.example");
});

test("a legacy Node 4 override is ignored and cannot create a third service", () => {
  const config = loadConfig({
    NODE4_HOST: "https://private.example",
    STRATEGY_HOST: "https://legacy.example",
  });
  assert.equal(config.NODE4_HOST, undefined);
  assert.equal(config.services.node4, undefined);
  assert.equal(config.baseUrls.node1, "https://engine-southeastasia-sng-main.onrender.com");
  assert.equal(config.baseUrls.node3, "https://strategy-api-vp-levels.onrender.com");
});

test("relative proxy prefixes are accepted as single base URLs", () => {
  const config = loadConfig({ NODE1_HOST: "/api/node1", NODE3_HOST: "/api/node3/" });
  assert.equal(config.baseUrls.node1, "/api/node1");
  assert.equal(config.baseUrls.node3, "/api/node3");
});

test("credentials, queries, fragments and custom paths are rejected", () => {
  for (const value of [
    "https://user:password@node1.example",
    "https://node1.example/custom/path",
    "https://node1.example/?q=1",
    "https://node1.example/#fragment",
  ]) {
    const config = loadConfig({ NODE1_HOST: value });
    assert.equal(config.baseUrls.node1, "", value);
  }
});
