"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

const sockets = [];
class FakeWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  constructor(url) {
    this.url = url;
    this.readyState = FakeWebSocket.CONNECTING;
    this.sent = [];
    sockets.push(this);
  }
  send(message) { this.sent.push(JSON.parse(message)); }
  open() {
    this.readyState = FakeWebSocket.OPEN;
    if (this.onopen) this.onopen();
  }
  receive(frame) {
    if (this.onmessage) this.onmessage({ data: typeof frame === "string" ? frame : JSON.stringify(frame) });
  }
  close() {
    this.readyState = FakeWebSocket.CLOSED;
    if (this.onclose) this.onclose();
  }
}

global.WebSocket = FakeWebSocket;
global.window = { App: {} };
const visibilityListeners = [];
global.document = {
  visibilityState: "visible",
  addEventListener(name, callback) { if (name === "visibilitychange") visibilityListeners.push(callback); },
  removeEventListener(name, callback) {
    if (name !== "visibilitychange") return;
    const index = visibilityListeners.indexOf(callback);
    if (index >= 0) visibilityListeners.splice(index, 1);
  },
};
require("../js/stream.js");
const createServiceStream = global.window.App.createServiceStream;

function makeStream(service, endpoint, topics) {
  return createServiceStream({
    service,
    endpoint,
    topics,
    reconnectMinMs: 250,
    reconnectMaxMs: 500,
    heartbeatIntervalMs: 5000,
    heartbeatTimeoutMs: 10000,
    staleAfterMs: 5000,
  });
}

test("service streams subscribe independently, track heartbeats/data separately, and recover on visibility", async () => {
  sockets.length = 0;
  const market = makeStream("market", "wss://market.example/ws", ["candle"]);
  const execution = makeStream("execution", "wss://execution.example/ws", ["diagnostics", "trades"]);
  market.start();
  execution.start();
  assert.equal(sockets.length, 2);

  const marketSocket = sockets[0];
  const executionSocket = sockets[1];
  marketSocket.open();
  executionSocket.open();
  assert.deepEqual(marketSocket.sent[0], { type: "subscribe", topics: ["candle"] });
  assert.deepEqual(executionSocket.sent[0], { type: "subscribe", topics: ["diagnostics", "trades"] });

  marketSocket.receive({ type: "heartbeat" });
  assert.ok(market.getState().lastHeartbeatAt);
  assert.equal(market.getState().lastDataAt, null);
  marketSocket.receive({ type: "candle", data: { timestamp: Date.now() } });
  assert.ok(market.getState().lastDataAt);
  assert.equal(execution.getState().lastDataAt, null);
  assert.equal(market.getState().status, "live");
  assert.equal(execution.getState().status, "live");

  executionSocket.close();
  assert.equal(execution.getState().status, "reconnecting");
  assert.equal(market.getState().status, "live");

  const beforeVisibility = sockets.length;
  global.document.visibilityState = "visible";
  for (const listener of visibilityListeners.slice()) listener();
  assert.equal(sockets.length, beforeVisibility + 1, "foreground visibility forces a prompt reconnect for the closed service");
  sockets.at(-1).open();
  assert.equal(execution.getState().connected, true);
  assert.equal(market.getState().connected, true);

  market.stop();
  execution.stop();
  assert.equal(market.getState().status, "offline");
  assert.equal(execution.getState().status, "offline");
});

test("an empty service endpoint remains visibly not configured without affecting another stream", () => {
  sockets.length = 0;
  const disabled = makeStream("strategy", "", ["diagnostics"]);
  const enabled = makeStream("market", "wss://market.example/ws", ["candle"]);
  disabled.start();
  enabled.start();
  assert.equal(disabled.getState().status, "not-configured");
  assert.equal(disabled.getState().endpointConfigured, false);
  assert.equal(enabled.getState().status, "reconnecting");
  assert.equal(sockets.length, 1);
  enabled.stop();
  disabled.stop();
});
