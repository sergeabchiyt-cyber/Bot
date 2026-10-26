"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { baseConfig, createClient } = require("../js/node2-client.js");

test("the dashboard uses configurable safe Node 1 and Node 3 origins", () => {
  assert.deepEqual(baseConfig({}), { node1: "https://engine-southeastasia-sng-main.onrender.com", node3: "https://strategy-api-vp-levels.onrender.com" });
  assert.deepEqual(baseConfig({ NODE1_BASE_URL: "http://market.test/", NODE3_BASE_URL: "https://strategy.test" }), { node1: "http://market.test", node3: "https://strategy.test" });
  const client = createClient({ NODE1_BASE_URL: "https://market.test", NODE3_BASE_URL: "https://strategy.test" }, async () => {});
  assert.equal(client.websocketUrl("node1"), "wss://market.test/ws");
  assert.equal(client.websocketUrl("node3"), "wss://strategy.test/ws");
});

test("service origins reject credentials, paths, queries, and fragments", () => {
  for (const origin of ["https://u:p@host.test", "https://host.test/path", "https://host.test/?token=x", "https://host.test/#x"]) {
    assert.equal(baseConfig({ NODE1_BASE_URL: origin }).node1, "");
    assert.equal(baseConfig({ NODE3_BASE_URL: origin }).node3, "");
  }
});
