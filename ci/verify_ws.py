#!/usr/bin/env python3
"""Proof (runtime): /ws still upgrades and streams frames with the CORS layer on.

The browser upgrade request is a plain GET carrying `Upgrade: websocket`, so the
layer must pass it through untouched. Implemented on a raw socket with a
minimal frame codec because CI has no websocket client library installed.

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


class Reader:
    """Accumulates socket bytes and yields unmasked server text frames."""

    def __init__(self, sock, leftover=b""):
        self.sock, self.buf = sock, bytearray(leftover)

    def _need(self, n: int) -> bool:
        while len(self.buf) < n:
            if not select.select([self.sock], [], [], max(0.1, self.deadline - time.monotonic()))[0]:
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
            # any other opcode (ping/pong/binary/continuation): keep going


# ---------------------------------------------------------------- handshake
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

head, rest = b"", b""
while b"\r\n\r\n" not in head:
    chunk = sock.recv(4096)
    if not chunk:
        fail("server closed the socket during the handshake")
    head += chunk
head, rest = head.split(b"\r\n\r\n", 1)

lines = head.decode("latin-1").split("\r\n")
status_line, headers = lines[0], {}
for line in lines[1:]:
    k, _, v = line.partition(":")
    headers[k.strip().lower()] = v.strip()
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

# ---------------------------------------------------------------- subscribe
sock.sendall(encode_frame(json.dumps({"topics": ["levels", "candle", "tick_volume", "status"], "type": "subscribe"}).encode()))

reader, seen, deadline = Reader(sock, rest), {}, time.monotonic() + 25
while time.monotonic() < deadline and not {"levels", "candle"} <= set(seen):
    text = reader.frame(deadline)
    if text is None:
        break
    try:
        frame = json.loads(text)
    except json.JSONDecodeError:
        continue
    if isinstance(frame, dict) and "type" in frame:
        seen.setdefault(frame["type"], frame)

sock.close()

print("frames received:", sorted(seen))
missing = {"levels", "candle"} - set(seen)
if missing:
    fail(f"no replay for {sorted(missing)} after subscribing: {sorted(seen)}")

# Candle replay proves /candles' payload shape is still intact (same VpCandle).
candle = seen["candle"]["data"]
if set(candle) != {"time", "open", "high", "low", "close", "volume", "source"}:
    fail(f"/ws candle keys changed: {sorted(candle)}")
levels = seen["levels"]["data"]
if not {"window", "poc", "vah", "val"} <= set(levels):
    fail(f"/ws levels keys changed: {sorted(levels)}")
print(f"candle: source={candle['source']} close={candle['close']}")
print(f"levels: {levels['window']} poc={levels['poc']}")
print("WS CHECK PASSED")
