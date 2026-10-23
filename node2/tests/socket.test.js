// socket.test.js — reconnect/disconnect behaviour of the WS client and the
// REST polling fallback.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createReconnectingSocket } from "../public/src/ws/socket.js";
import { createPollingFallback } from "../public/src/ws/polling.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

class FakeWebSocket {
  static instances = [];
  static reset() {
    FakeWebSocket.instances = [];
  }

  constructor(url) {
    this.url = url;
    this.readyState = 0; // CONNECTING
    this.sent = [];
    this.onopen = null;
    this.onmessage = null;
    this.onclose = null;
    this.onerror = null;
    FakeWebSocket.instances.push(this);
  }

  send(data) {
    this.sent.push(JSON.parse(data));
  }

  _open() {
    this.readyState = 1;
    if (this.onopen) this.onopen();
  }

  _message(payload) {
    if (this.onmessage) this.onmessage({ data: JSON.stringify(payload) });
  }

  _serverClose() {
    this.readyState = 3;
    if (this.onclose) this.onclose();
  }

  close() {
    this.readyState = 3;
    if (this.onclose) this.onclose();
  }
}

function makeSocket(overrides = {}) {
  const messages = [];
  const states = [];
  const socket = createReconnectingSocket({
    url: "wss://feed.example/ws",
    subscribeMessage: { type: "subscribe", topics: ["status"] },
    WebSocketImpl: FakeWebSocket,
    baseDelayMs: 1,
    heartbeatMs: 30000,
    onMessage: (data) => messages.push(JSON.parse(data)),
    onStateChange: (s) => states.push(s),
    ...overrides,
  });
  return { socket, messages, states };
}

test("socket: sends subscribe immediately on open", () => {
  FakeWebSocket.reset();
  const { socket } = makeSocket();
  const ws = FakeWebSocket.instances[0];
  assert.ok(ws, "socket connects on creation");
  ws._open();
  assert.equal(ws.sent.length, 1);
  assert.deepEqual(ws.sent[0], { type: "subscribe", topics: ["status"] });
  assert.equal(socket.state.state, "open");
  socket.close();
});

test("socket: dispatches messages and tracks heartbeat timestamps", () => {
  FakeWebSocket.reset();
  const { socket, messages } = makeSocket();
  const ws = FakeWebSocket.instances[0];
  ws._open();
  ws._message({ type: "heartbeat", ts: 123 });
  ws._message({ kind: "status", status: { ok: true } });
  assert.equal(messages.length, 2);
  assert.equal(messages[0].type, "heartbeat");
  assert.equal(messages[1].status.ok, true);
  assert.ok(socket.state.lastMsgTs > 0);
  socket.close();
});

test("socket: server close schedules a reconnect and re-subscribes", async () => {
  FakeWebSocket.reset();
  const { socket, states } = makeSocket();
  const ws0 = FakeWebSocket.instances[0];
  ws0._open();
  ws0._serverClose();
  assert.equal(socket.state.state, "reconnecting");
  assert.ok(states.includes("reconnecting"));

  await sleep(40);
  // The attempt counter increments per failed cycle (reset on successful open).
  assert.equal(socket._getAttempt(), 1);
  const ws1 = FakeWebSocket.instances[1];
  assert.ok(ws1, "a second socket is created after backoff");
  ws1._open();
  assert.equal(ws1.sent.length, 1);
  assert.equal(ws1.sent[0].type, "subscribe");
  socket.close();
});

test("socket: repeated drops grow the attempt counter", async () => {
  FakeWebSocket.reset();
  const { socket } = makeSocket();
  FakeWebSocket.instances[0]._open();
  FakeWebSocket.instances[0]._serverClose();
  await sleep(40);
  // Drop again before the reconnected socket opens (consecutive failures).
  FakeWebSocket.instances[1]._serverClose();
  await sleep(60);
  assert.ok(FakeWebSocket.instances[2], "third attempt created");
  assert.equal(socket._getAttempt(), 2);
  socket.close();
});

test("socket: user close stops reconnection entirely", async () => {
  FakeWebSocket.reset();
  const { socket } = makeSocket();
  FakeWebSocket.instances[0]._open();
  socket.close();
  assert.equal(socket.state.state, "closed");
  await sleep(40);
  assert.equal(FakeWebSocket.instances.length, 1, "no new sockets after user close");
});

test("socket: initial state is connecting before open", () => {
  FakeWebSocket.reset();
  const { socket } = makeSocket();
  assert.equal(socket.state.state, "connecting");
  socket.close();
});

test("socket: reconnecting after reconnect resets attempt counter", async () => {
  FakeWebSocket.reset();
  const { socket } = makeSocket();
  FakeWebSocket.instances[0]._open();
  FakeWebSocket.instances[0]._serverClose();
  await sleep(40);
  FakeWebSocket.instances[1]._open();
  assert.equal(socket._getAttempt(), 0);
  socket.close();
});

test("polling fallback: starts, polls, stops", async () => {
  let polls = 0;
  const poll = async () => {
    polls += 1;
  };
  const p = createPollingFallback({ poll, intervalMs: 5 });
  assert.equal(p.isDelayed(), false);
  p.start();
  assert.equal(p.isDelayed(), true);
  await sleep(25);
  assert.ok(polls >= 2, `polls ran: ${polls}`);
  p.stop();
  assert.equal(p.isDelayed(), false);
  const afterStop = polls;
  await sleep(15);
  assert.equal(polls, afterStop, "no polls after stop");
});

test("polling fallback: start is idempotent, poll errors never propagate", async () => {
  let polls = 0;
  const p = createPollingFallback({
    poll: async () => {
      polls += 1;
      if (polls === 1) throw new Error("upstream 502");
    },
    intervalMs: 5,
  });
  p.start();
  p.start(); // idempotent
  await sleep(20);
  assert.ok(polls >= 2, "recovered from poll error");
  p.stop();
});
