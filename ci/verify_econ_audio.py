#!/usr/bin/env python3
"""Proof (runtime): the /ws stream now serves the Node3 AI contract.

Against a live engine booted with ECON_TEST_TONE=1 (econ monitor in synthetic
tone mode — no ffmpeg/yt-dlp/network needed):

  1. subscribing to ["audio_chunk","learn","transcript"] delivers
     `audio_chunk` frames carrying base64 f32le 16kHz mono PCM
  2. a loose `learn` ingest frame is rebroadcast in the canonical envelope
     (the frame Node3's rill_learner consumes)
  3. a `transcript` frame (the Node3 -> engine direction) is echoed on the
     topic and cached — GET /ai reports it as what the news delivered today

Raw socket + minimal frame codec, same approach as verify_ws.py (CI has no
websocket client library installed).

usage: verify_econ_audio.py <host:port>
"""

import base64
import json
import os
import select
import socket
import struct
import sys
import time
import urllib.request

HOSTPORT = sys.argv[1]
host, _, port = HOSTPORT.partition(":")


def fail(msg):
    print(f"ECON AUDIO CHECK FAILED: {msg}")
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
        "\r\n"
    ).encode()
)
buf = b""
while b"\r\n\r\n" not in buf:
    chunk = sock.recv(4096)
    if not chunk:
        fail("connection closed during upgrade")
    buf += chunk
header, _, leftover = buf.partition(b"\r\n\r\n")
status_line = header.split(b"\r\n", 1)[0].decode("latin-1")
if " 101 " not in status_line + " ":
    fail(f"/ws did not upgrade: {status_line}")
print(f" <- /ws upgraded ({status_line.strip()})")

reader = Reader(sock, leftover)

# ------------------------------------------------- subscribe to the AI topics
sock.sendall(
    encode_frame(
        json.dumps(
            {"type": "subscribe", "topics": ["audio_chunk", "learn", "transcript"]}
        ).encode()
    )
)
print(' -> subscribed ["audio_chunk","learn","transcript"]')

# ------------------------------------------- 1) audio_chunk frames must flow
audio_frames = []
deadline = time.monotonic() + 20
while len(audio_frames) < 3 and time.monotonic() < deadline:
    text = reader.frame(min(deadline, time.monotonic() + 5))
    if text is None:
        continue
    frame = json.loads(text)
    if frame.get("type") == "audio_chunk":
        audio_frames.append(frame)

if len(audio_frames) < 3:
    fail(f"received {len(audio_frames)} audio_chunk frames, expected >= 3 (is the econ monitor running?)")

for frame in audio_frames:
    data = frame.get("data")
    if not isinstance(data, dict):
        fail(f"audio_chunk data must be an object, got {type(data).__name__}")
    if data.get("sample_rate") != 16000:
        fail(f"audio_chunk sample_rate must be 16000, got {data.get('sample_rate')}")
    if data.get("format") != "f32le":
        fail(f"audio_chunk format must be f32le, got {data.get('format')}")
    try:
        pcm = base64.b64decode(data.get("data", ""))
    except Exception as e:
        fail(f"audio_chunk data.data is not valid base64: {e}")
    if len(pcm) == 0 or len(pcm) % 4 != 0:
        fail(f"audio_chunk PCM must be non-empty f32le, got {len(pcm)} bytes")
    samples = struct.unpack(f"<{len(pcm) // 4}f", pcm)
    if max(abs(s) for s in samples) == 0.0:
        fail("audio_chunk PCM is all silence")
print(f" <- {len(audio_frames)} audio_chunk frames (valid base64 f32le 16kHz, non-silent)")
print(f"    e.g. event={audio_frames[0]['data'].get('event')!r} source={audio_frames[0]['data'].get('source')!r}")

# --------------------------- 2) loose learn ingest -> canonical learn rebroadcast
sock.sendall(
    encode_frame(
        json.dumps({"type": "learn", "features": [1.5, -0.4, 0.8, 0.68], "target": 1}).encode()
    )
)
learn_seen = None
deadline = time.monotonic() + 10
while time.monotonic() < deadline:
    text = reader.frame(min(deadline, time.monotonic() + 5))
    if text is None:
        continue
    frame = json.loads(text)
    if frame.get("type") == "learn":
        learn_seen = frame
        break
if learn_seen is None:
    fail("learn frame was not rebroadcast on the bus")
data = learn_seen.get("data") or {}
if data.get("features") != [1.5, -0.4, 0.8, 0.68] or data.get("target") != 1:
    fail(f"learn rebroadcast lost the payload: {learn_seen}")
print(" <- learn frame rebroadcast in canonical envelope {data:{features,target}}")

# --------------------- 3) transcript ingest -> /ai answers "delivered today"
now_ms = int(time.time() * 1000)
sock.sendall(
    encode_frame(
        json.dumps(
            {
                "type": "transcript",
                "data": {"text": "verify: inflation remains elevated", "ts": now_ms, "tier": "test"},
            }
        ).encode()
    )
)
transcript_seen = False
deadline = time.monotonic() + 10
while time.monotonic() < deadline:
    text = reader.frame(min(deadline, time.monotonic() + 5))
    if text is None:
        continue
    frame = json.loads(text)
    if frame.get("type") == "transcript":
        transcript_seen = True
        break
if not transcript_seen:
    fail("transcript frame was not fanned out on the topic")
print(" <- transcript frame fanned out on the `transcript` topic")

with urllib.request.urlopen(f"http://{HOSTPORT}/ai", timeout=10) as res:
    digest = json.load(res)
for key in ("day_start", "generated", "transcripts", "sentiments", "predictions", "health", "counts"):
    if key not in digest:
        fail(f"/ai missing key {key}")
if not any(t.get("text", "").startswith("verify:") for t in digest["transcripts"]):
    fail(f"/ai did not cache the transcript: {digest['transcripts']}")
print(f" <- /ai digest ok (today: {digest['counts']['transcripts']} transcripts, {digest['counts']['sentiments']} sentiments)")

print("ECON AUDIO CHECK OK")
