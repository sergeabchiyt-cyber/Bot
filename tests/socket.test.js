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
    if (this.onmessage) {
      this.onmessage({ data: typeof frame === "string" ? frame : JSON.stringify(frame) });
    }
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

require("../js/config.js");
require("../js/format.js");

const App = global.window.App;
App.config.socket.reconnectMinMs = 250;
App.config.socket.reconnectMaxMs = 500;
require("../js/socket.js");

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function freshSocket() {
  App.socket.stop();
  sockets.length = 0;
  return App.socket;
}

test("the single socket subscribes to the full topic list on every open", async () => {
  const socket = freshSocket();
  const states = [];
  socket.onState((state) => states.push(state.status));

  socket.start();
  assert.equal(sockets.length, 1);
  assert.equal(sockets[0].url, "wss://engine-southeastasia-sng-main.onrender.com/ws");
  assert.equal(socket.getState().status, "connecting");

  const live = sockets[0];
  live.open();
  assert.deepEqual(live.sent[0], {
    type: "subscribe",
    topics: ["levels", "candle", "tick_volume", "strategy", "demo", "calendar", "status"],
  });
  assert.equal(live.sent[0].topics.includes("bubbles"), false);
  assert.equal(socket.getState().connected, true);
  assert.equal(socket.getState().status, "live");

  socket.stop();
  assert.equal(socket.getState().status, "offline");
  assert.ok(states.includes("live"));
});

test("frames dispatch by type, heartbeats update only the message clock", () => {
  const socket = freshSocket();
  const received = {};
  for (const type of ["strategy", "demo", "status", "levels", "candle", "tick_volume", "calendar"]) {
    socket.on(type, (payload) => { received[type] = payload; });
  }
  socket.start();
  const live = sockets[0];
  live.open();

  live.receive({ type: "strategy", data: { ts: 1, action: "buy" } });
  assert.deepEqual(received.strategy, { ts: 1, action: "buy" });
  assert.ok(socket.getState().lastDataAt);

  const dataAt = socket.getState().lastDataAt;
  live.receive({ type: "heartbeat" });
  assert.equal(socket.getState().lastDataAt, dataAt, "heartbeat is not data");
  assert.ok(socket.getState().lastMessageAt);

  live.receive("not json");
  assert.match(socket.getState().lastError, /malformed/i);
  assert.equal(socket.getState().connected, true);

  assert.equal(socket.sendHeartbeat(), true);
  assert.deepEqual(live.sent.at(-1), { type: "heartbeat" });
  socket.stop();
});

test("a dropped socket reconnects and re-sends the full subscribe payload", async () => {
  const socket = freshSocket();
  socket.start();
  const first = sockets[0];
  first.open();
  assert.equal(socket.getState().status, "live");

  first.close();
  assert.equal(socket.getState().connected, false);
  assert.equal(socket.getState().status, "reconnecting");

  await delay(400);
  assert.equal(sockets.length, 2, "backoff schedules exactly one reconnect");
  const second = sockets[1];
  second.open();
  assert.deepEqual(second.sent[0], {
    type: "subscribe",
    topics: ["levels", "candle", "tick_volume", "strategy", "demo", "calendar", "status"],
  });
  assert.equal(socket.getState().status, "live");
  socket.stop();
});

test("returning to the tab reconnects without waiting for the backoff timer", () => {
  const socket = freshSocket();
  socket.start();
  sockets[0].open();
  sockets[0].close();
  const before = sockets.length;

  global.document.visibilityState = "visible";
  for (const listener of visibilityListeners.slice()) listener();
  assert.equal(sockets.length, before + 1);
  assert.equal(socket.getState().status, "reconnecting");
  socket.stop();
});
