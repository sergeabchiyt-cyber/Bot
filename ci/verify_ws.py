#!/usr/bin/env python3
"""Runtime proof for /ws replay, subscriptions, and Node3 trade fan-out.

The browser upgrade request is a plain GET carrying `Upgrade: websocket`, so the
CORS layer must pass it through untouched. A small raw-socket frame codec keeps
CI independent of an external websocket client library.

usage: verify_ws.py <host:port> <origin>
"""

import base64
import json
import os
import select
import socket
import struct
import sys
import time

HOSTPORT, ORIGIN = sys.argv[1], sys.argv[2]
host, _, port = HOSTPORT.partition(":")


def fail(msg):
    print(f"WS CHECK FAILED: {msg}")
    sys.exit(1)


def encode_frame(payload: bytes) -> bytes:
    """Client -> server text frame: FIN+text, masked (the spec requires it)."""
    mask = os.urandom(4)
    out = bytearray([0x81])
    n = len(payload)
    if n < 126:
        out.append(0x80 | n)
    elif n < 0x10000:
        out += b"\xfe" + struct.pack(">H", n)
    else:
        out += b"\xff" + struct.pack(">Q", n)
    out += mask
    out += bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    return bytes(out)


def send_json(sock, value):
    sock.sendall(encode_frame(json.dumps(value, separators=(",", ":")).encode()))


class Reader:
    """Accumulates socket bytes and yields unmasked server text frames."""

    def __init__(self, sock, leftover=b""):
        self.sock, self.buf = sock, bytearray(leftover)

    def _need(self, n: int) -> bool:
        while len(self.buf) < n:
            wait = max(0.1, self.deadline - time.monotonic())
            if not select.select([self.sock], [], [], wait)[0]:
                return False
            chunk = self.sock.recv(65536)
            if not chunk:
                return False
            self.buf += chunk
        return True

    def frame(self, deadline):
        """Next text frame as str, or None on timeout/close (control frames skipped)."""
        self.deadline = deadline
        while True:
            if not self._need(2):
                return None
            b0, b1 = self.buf[0], self.buf[1]
            opcode, length = b0 & 0x0F, b1 & 0x7F
            head = 2
            if length == 126:
                if not self._need(4):
                    return None
                (length,) = struct.unpack(">H", bytes(self.buf[2:4]))
                head = 4
            elif length == 127:
                if not self._need(10):
                    return None
                (length,) = struct.unpack(">Q", bytes(self.buf[2:10]))
                head = 10
            if not self._need(head + length):
                return None
            payload = bytes(self.buf[head : head + length])
            del self.buf[: head + length]
            if opcode == 0x1:  # text
                return payload.decode("utf-8", "replace")
            if opcode == 0x8:
                return None  # close
            # Any other opcode (ping/pong/binary/continuation): keep going.


def connect():
    """Upgrade a raw socket and return the socket and framed reader."""
    sock = socket.create_connection((host, int(port or 80)), timeout=15)
    key = base64.b64encode(os.urandom(16)).decode()
    sock.sendall(
        (
            "GET /ws HTTP/1.1\r\n"
            f"Host: {HOSTPORT}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n"
            f"Origin: {ORIGIN}\r\n"
            "\r\n"
        ).encode()
    )

    response = b""
    while b"\r\n\r\n" not in response:
        chunk = sock.recv(4096)
        if not chunk:
            fail("server closed the socket during the handshake")
        response += chunk
    head, leftover = response.split(b"\r\n\r\n", 1)

    lines = head.decode("latin-1").split("\r\n")
    status_line, headers = lines[0], {}
    for line in lines[1:]:
        key, _, value = line.partition(":")
        headers[key.strip().lower()] = value.strip()
    print("handshake:", status_line)
    for name in ("sec-websocket-accept", "access-control-allow-origin", "vary"):
        if name in headers:
            print(f"  {name}: {headers[name]}")

    if "101" not in status_line:
        fail(f"/ws did not upgrade (CorsLayer or axum rejected the request): {status_line}")
    if not headers.get("sec-websocket-accept"):
        fail("no Sec-WebSocket-Accept — the upgrade never completed")
    if headers.get("access-control-allow-origin") != ORIGIN:
        fail("the 101 response lost the CORS allow-header for the site's own origin")
    return sock, Reader(sock, leftover)


def read_json(reader, deadline):
    while time.monotonic() < deadline:
        text = reader.frame(deadline)
        if text is None:
            return None
        try:
            frame = json.loads(text)
        except json.JSONDecodeError:
            continue
        if isinstance(frame, dict) and "type" in frame:
            return frame
    return None


# Node3-style client: the subscription must accept all three market topics and
# replay the latest supported levels plus enough recent candles to seed ATR(14).
node3_sock, node3_reader = connect()
send_json(
    node3_sock,
    {"type": "subscribe", "topics": ["candle", "levels", "trades"]},
)

levels, candles = [], []
deadline = time.monotonic() + 25
while time.monotonic() < deadline and (len(candles) < 15 or not levels):
    frame = read_json(node3_reader, deadline)
    if frame is None:
        break
    if frame["type"] == "levels":
        levels.append(frame["data"])
    elif frame["type"] == "candle":
        candles.append(frame["data"])

if not levels:
    fail("no cached levels were replayed after subscribing")
if len(candles) < 15:
    fail(f"expected the 15-bar ATR(14) replay, received {len(candles)} candles")
if any(candles[i]["time"] > candles[i + 1]["time"] for i in range(len(candles) - 1)):
    fail("replayed candles are not in ascending time order")

candle = candles[-1]
if set(candle) != {"time", "open", "high", "low", "close", "volume", "source"}:
    fail(f"/ws candle keys changed: {sorted(candle)}")
if not any(
    lv.get("window") in {"PW", "PS", "CW"}
    and {"poc", "vah", "val"} <= set(lv)
    for lv in levels
):
    fail(f"no supported PW/PS/CW level payload was replayed: {levels}")

print(f"replay: {len(levels)} levels, {len(candles)} candles")
print(f"candle: source={candle['source']} close={candle['close']}")
print(f"levels: {levels[0]['window']} poc={levels[0]['poc']}")

# Node2-style dashboard: subscribe only to trades, then prove a TradeEvent sent
# by Node3 is parsed by Node1 and fanned out on the `trades` topic.
dashboard_sock, dashboard_reader = connect()
send_json(dashboard_sock, {"type": "subscribe", "topics": ["trades"]})
status_deadline = time.monotonic() + 10
status_seen = False
while time.monotonic() < status_deadline:
    frame = read_json(dashboard_reader, status_deadline)
    if frame is None:
        fail("dashboard did not receive the post-subscribe status frame")
    if frame["type"] == "status":
        status_seen = True
        break
if not status_seen:
    fail("timed out waiting for the post-subscribe status frame")

trade = {
    "type": "trades",
    "data": {
        "trade_id": "node3-ws-smoke",
        "symbol": "XAUUSD",
        "side": "buy",
        "size": 0.01,
        "entry": 2340.0,
        "sl": 2335.0,
        "tp": 2350.0,
        "status": "signal",
        "timestamp": 1_700_000_000_000,
    },
}
send_json(node3_sock, trade)
trade_deadline = time.monotonic() + 10
while time.monotonic() < trade_deadline:
    frame = read_json(dashboard_reader, trade_deadline)
    if frame is None:
        fail("Node1 did not fan the Node3 trade frame out to the trades subscriber")
    trade_data = frame.get("data")
    if (
        frame["type"] == "trades"
        and isinstance(trade_data, dict)
        and trade_data.get("trade_id") == "node3-ws-smoke"
    ):
        if trade_data != trade["data"]:
            fail(f"trade payload changed during fan-out: {frame}")
        break
else:
    fail("timed out waiting for the rebroadcast trade frame")

node3_sock.close()
dashboard_sock.close()
print("trade fan-out: Node3 TradeEvent reached the trades subscriber unchanged")
print("WS CHECK PASSED")
