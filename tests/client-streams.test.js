"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

const client = require("../js/client.js");

class FakeWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  constructor(url) {
    this.url = url;
    this.readyState = FakeWebSocket.CONNECTING;
    this.sent = [];
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.onclose = null;
    sockets.push(this);
  }
  open() {
    this.readyState = FakeWebSocket.OPEN;
    if (this.onopen) this.onopen();
  }
  send(text) {
    this.sent.push(JSON.parse(text));
  }
  receive(frame) {
    if (this.onmessage) this.onmessage({ data: JSON.stringify(frame) });
  }
  close() {
    this.readyState = FakeWebSocket.CLOSED;
    if (this.onclose) this.onclose();
  }
}

let sockets = [];

client.configure({
  baseUrls: { node1: "https://node1.example", node3: "https://node3.example" },
  services: {
    node1: { topics: ["candle", "tick_volume", "calendar", "status"] },
    node3: { topics: ["levels", "drawing", "vp", "scanning", "signals", "diagnostics"] },
  },
  stream: {
    reconnectMinMs: 1000,
    reconnectMaxMs: 30000,
    heartbeatEveryMs: 60000,
    heartbeatTimeoutMs: 120000,
    staleAfterMs: 120000,
    watchdogEveryMs: 60000,
  },
});

function fresh(testOptions = {}) {
  sockets = [];
  global.WebSocket = FakeWebSocket;
  return client.createStream("node1", {
    heartbeatEveryMs: 60000,
    heartbeatTimeoutMs: 120000,
    staleAfterMs: 120000,
    ...testOptions,
  });
}

test("both public sockets subscribe to exactly their configured topics", () => {
  sockets = [];
  global.WebSocket = FakeWebSocket;
  const node1 = client.createStream("node1", { heartbeatEveryMs: 60000, heartbeatTimeoutMs: 120000 }).start();
  const node3 = client.createStream("node3", { heartbeatEveryMs: 60000, heartbeatTimeoutMs: 120000 }).start();
  sockets[0].open();
  sockets[1].open();

  assert.equal(sockets[0].url, "wss://node1.example/ws");
  assert.equal(sockets[1].url, "wss://node3.example/ws");
  assert.deepEqual(sockets[0].sent[0], {
    type: "subscribe",
    topics: ["candle", "tick_volume", "calendar", "status"],
  });
  assert.deepEqual(sockets[1].sent[0], {
    type: "subscribe",
    topics: ["levels", "drawing", "vp", "scanning", "signals", "diagnostics"],
  });

  node1.sendHeartbeat();
  node3.sendHeartbeat();
  for (const socket of sockets) {
    assert.equal(socket.sent.length, 2);
    assert.deepEqual(socket.sent[1], { type: "heartbeat" });
  }
  node1.stop();
  node3.stop();
});

test("replay is distinct from live seq, gaps are visible, and unknown frames are harmless", () => {
  const frames = [];
  const gaps = [];
  const stream = fresh({
    onFrame: (frame) => frames.push(frame),
    onGap: (info) => gaps.push(info),
  });
  stream.start();
  const socket = sockets[0];
  socket.open();

  socket.receive({ type: "status", replay: true, data: { contract_version: "2.0" } });
  socket.receive({ type: "candle", seq: 1, data: { interval: "1m", closed: true } });
  socket.receive({ type: "candle", seq: 4, data: { interval: "1m", closed: true } });
  socket.receive({ type: "unknown_topic", seq: 5, data: { ignored: true } });
  socket.receive({ type: "gap", data: { dropped: 7 } });
  socket.receive({ type: "candle", seq: 20, data: { interval: "1m", closed: true } });

  assert.equal(frames[0].replay, true);
  assert.equal(frames[0].seq, null);
  assert.equal(frames[0].data.contract_version, "2.0");
  assert.equal(frames[1].seq, 1);
  assert.equal(frames.some((frame) => frame.type === "unknown_topic"), true);
  assert.equal(gaps.length, 2);
  assert.equal(gaps[0].source, "seq");
  assert.equal(gaps[0].dropped, 2);
  assert.equal(gaps[1].source, "frame");
  assert.equal(gaps[1].dropped, 7);
  assert.match(gaps[1].reason, /dropped 7/);
  assert.equal(socket.sent.length, 1, "no client frame was sent in response to a gap");
  assert.deepEqual(socket.sent[0], { type: "subscribe", topics: ["candle", "tick_volume", "calendar", "status"] });
  assert.equal(stream.getState().lastSeq, 20);
  stream.stop();
});

test("a malformed frame is ignored without closing a healthy stream", () => {
  const stream = fresh({ onFrame: () => {} });
  stream.start();
  const socket = sockets[0];
  socket.open();
  socket.onmessage({ data: "not json" });
  assert.equal(stream.getState().connected, true);
  assert.match(stream.getState().lastError, /malformed/);
  stream.stop();
});
