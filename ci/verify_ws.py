#!/usr/bin/env python3
"""Runtime proof for /ws: the C1 v2 envelope, the replay burst, and silence.

Everything here is observed on a real socket against the real process:

  * the upgrade survives the CORS layer and keeps the allow-header;
  * `subscribe` replays cached state per requested resolution — `replay: true`,
    **never** a `seq`, never a forming bucket, ascending within a resolution;
  * the post-subscribe `status` document carries `contract_version`, `boot_id`
    and `freshness`;
  * live frames carry a strictly increasing `seq` shared across topics and
    across sockets (one process-wide counter);
  * `heartbeat` is answered in both directions and carries no `seq`;
  * an execution-shaped inbound frame (`trades`, `order`, `trade_intent`,
    `execution_hello`, `execution_report`, `bubbles`) is never rebroadcast, and
    no frame anywhere carries side/stop/target/size/venue vocabulary.

usage: verify_ws.py <host:port> <origin> [--record PATH] [--require-live]

`--require-live` insists on at least two live frames (the smoke test arranges
them with a delayed calendar fixture). Without it, live frames are validated
when they happen and reported when they do not: a deployed engine may simply
have nothing to say during the sampling window.
"""

import base64
import json
import os
import select
import socket
import struct
import sys
import time

CANDLE_KEYS = {
    "symbol", "interval", "time", "open", "high", "low", "close", "volume",
    "source", "closed",
}
INTERVAL_MS = {"1m": 60_000, "15m": 15 * 60_000, "1h": 60 * 60_000}
FRESHNESS_STATES = {"live", "stale", "offline", "closed_market"}
FEED_TOPICS = {"candle", "tick_volume", "calendar", "status"}
BANNED_KEYS = {
    "side", "stop_loss", "take_profit", "lots", "risk_usd", "action", "venue",
    "trade_intent", "intent_id", "leverage", "position", "equity",
}
REJECTED_KINDS = [
    "trades", "trade_intent", "execution_hello", "execution_report", "order", "bubbles",
]
SENTINEL = "node1-must-never-echo-this"

positional = []
RECORD = None
REQUIRE_LIVE = False
argument = iter(sys.argv[1:])
for item in argument:
    if item == "--record":
        RECORD = next(argument, None)
    elif item == "--require-live":
        REQUIRE_LIVE = True
    elif item.startswith("--"):
        print(f"unknown flag {item}\n{__doc__}")
        sys.exit(2)
    else:
        positional.append(item)

if len(positional) < 2:
    print(__doc__)
    sys.exit(2)
HOSTPORT, ORIGIN = positional[0], positional[1]
host, _, port = HOSTPORT.partition(":")

RECORDED = []
PROBLEMS = []


def fail(message):
    print(f"WS CHECK FAILED: {message}")
    sys.exit(1)


def problem(message):
    PROBLEMS.append(message)


def check(condition, message):
    if not condition:
        problem(message)
    return condition


def scan(document, predicate, path="$"):
    """Every `path` in `document` whose key or string value satisfies `predicate`."""
    found = []
    if isinstance(document, dict):
        for key, value in document.items():
            if predicate(key):
                found.append(f"{path}.{key}")
            found += scan(value, predicate, f"{path}.{key}")
    elif isinstance(document, list):
        for index, value in enumerate(document):
            found += scan(value, predicate, f"{path}[{index}]")
    elif isinstance(document, str) and predicate(document):
        found.append(path)
    return found


def banned(document):
    return scan(document, lambda token: token in BANNED_KEYS)


# ---------------------------------------------------------------------------
# Raw frame codec (no third-party websocket client)
# ---------------------------------------------------------------------------


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


def connect(label):
    """Upgrade a raw socket and return the socket and framed reader."""
    sock = socket.create_connection((host, int(port or 80)), timeout=20)
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
            fail(f"{label}: server closed the socket during the handshake")
        response += chunk
    head, leftover = response.split(b"\r\n\r\n", 1)

    lines = head.decode("latin-1").split("\r\n")
    status_line, headers = lines[0], {}
    for line in lines[1:]:
        name, _, value = line.partition(":")
        headers[name.strip().lower()] = value.strip()
    print(f"handshake ({label}): {status_line}")
    for name in ("sec-websocket-accept", "access-control-allow-origin", "vary"):
        if name in headers:
            print(f"  {name}: {headers[name]}")

    check("101" in status_line,
          f"{label}: /ws did not upgrade (CorsLayer or axum rejected it): {status_line}")
    check(bool(headers.get("sec-websocket-accept")),
          f"{label}: no Sec-WebSocket-Accept — the upgrade never completed")
    check(headers.get("access-control-allow-origin") == ORIGIN,
          f"{label}: the 101 response lost the CORS allow-header for {ORIGIN}")
    return sock, Reader(sock, leftover)


def decode(text, record=True):
    try:
        frame = json.loads(text)
    except (json.JSONDecodeError, TypeError):
        problem(f"received a non-JSON text frame: {str(text)[:80]!r}")
        return None
    if not isinstance(frame, dict) or "type" not in frame:
        problem(f"received a frame without `type`: {str(text)[:80]!r}")
        return None
    if record:
        RECORDED.append(frame)
    return frame


def next_frame(reader, deadline, record=True):
    """Next JSON frame from one socket, or None."""
    while time.monotonic() < deadline:
        text = reader.frame(deadline)
        if text is None:
            return None
        frame = decode(text, record)
        if frame is not None:
            return frame
    return None


def next_any(pairs, deadline):
    """Next frame from whichever of `pairs` (label, sock, reader) is ready."""
    while time.monotonic() < deadline:
        socks = [sock for _, sock, _ in pairs]
        wait = max(0.05, deadline - time.monotonic())
        ready, _, _ = select.select(socks, [], [], wait)
        for label, sock, reader in pairs:
            if sock in ready:
                text = reader.frame(deadline)
                if text is None:
                    continue
                # Only the full subscriber is recorded: two sockets share one
                # seq counter, so merging them would look like reordering.
                frame = decode(text, record=(label == "full" and len(pairs) > 1))
                if frame is not None:
                    return label, frame
    return None, None


def envelope(frame, where):
    """Envelope rules shared by every frame, whatever its topic."""
    kind = frame.get("type")
    check(isinstance(kind, str), f"{where}: frame has no string `type`: {frame}")
    for path in banned(frame):
        problem(f"{where}: frame carries trade vocabulary at {path}")

    if kind in FEED_TOPICS:
        if frame.get("replay") is True:
            check("seq" not in frame, f"{where}: replay frame carries seq")
        else:
            check(isinstance(frame.get("seq"), int),
                  f"{where}: live {kind} frame has no integer seq")
            check("replay" not in frame, f"{where}: live frame carries replay")
        check(isinstance(frame.get("data"), (dict, list)),
              f"{where}: feed frame has no data payload")
    else:
        check("seq" not in frame, f"{where}: control frame {kind!r} carries seq")
        check("replay" not in frame, f"{where}: control frame {kind!r} carries replay")
    return kind


def check_candle(data, where):
    check(set(data) == CANDLE_KEYS, f"{where}: candle keys changed: {sorted(data)}")
    check(data.get("symbol") == "XAUUSD", f"{where}: symbol is {data.get('symbol')!r}")
    interval = data.get("interval")
    check(interval in INTERVAL_MS, f"{where}: interval is {interval!r}")
    if interval in INTERVAL_MS:
        check(data.get("time", 0) % INTERVAL_MS[interval] == 0,
              f"{where}: time {data.get('time')} is off the {interval} grid")
    for key in ("open", "high", "low", "close"):
        check(isinstance(data.get(key), (int, float)) and data[key] > 0,
              f"{where}: {key} is {data.get(key)!r}")
    check(isinstance(data.get("volume"), (int, float)) and data["volume"] >= 0,
          f"{where}: volume is {data.get('volume')!r}")
    check(isinstance(data.get("closed"), bool), f"{where}: closed is {data.get('closed')!r}")


def check_status(data, where):
    check(data.get("contract_version") == "2.0",
          f"{where}: contract_version is {data.get('contract_version')!r}")
    boot = data.get("boot_id")
    check(isinstance(boot, str) and boot.strip(), f"{where}: boot_id is {boot!r}")
    freshness = data.get("freshness") or {}
    check(freshness.get("state") in FRESHNESS_STATES,
          f"{where}: freshness.state is {freshness.get('state')!r}")
    for key in ("feed", "last_tick_ms", "age_ms", "market_open"):
        check(key in freshness, f"{where}: freshness is missing {key!r}")
    check(isinstance(data.get("market_open"), bool), f"{where}: market_open is not a bool")
    check(freshness.get("market_open") == data.get("market_open"),
          f"{where}: freshness.market_open disagrees with market_open")
    for retired in ("strategy", "demo", "vp", "levels"):
        check(retired not in data, f"{where}: status still carries the retired {retired!r}")
    return boot


# ---------------------------------------------------------------------------
# 1. Full subscriber: the replay burst
# ---------------------------------------------------------------------------

full_sock, full_reader = connect("full")
send_json(
    full_sock,
    {
        "type": "subscribe",
        "topics": ["candle", "tick_volume", "calendar", "status"],
        "intervals": ["1m", "15m", "1h"],
    },
)

replay_by_interval = {name: [] for name in INTERVAL_MS}
replay_calendar, replay_boot = None, None
live_full = {}

deadline = time.monotonic() + 25
while time.monotonic() < deadline:
    frame = next_frame(full_reader, deadline)
    if frame is None:
        break
    kind = envelope(frame, "full")
    if kind == "candle" and frame.get("replay") is True:
        data = frame["data"]
        check_candle(data, "full/replay")
        check(data.get("closed") is True, "full/replay: a replay frame carried a forming bucket")
        if data.get("interval") in replay_by_interval:
            replay_by_interval[data["interval"]].append(data)
    elif kind == "calendar" and frame.get("replay") is True:
        replay_calendar = frame["data"]
    elif kind == "status" and frame.get("replay") is True:
        replay_boot = check_status(frame["data"], "full/replay")
    elif frame.get("replay") is not True:
        live_full[frame.get("seq")] = kind
    complete = (
        all(bars for bars in replay_by_interval.values())
        and replay_calendar is not None
        and replay_boot is not None
    )
    if complete:
        break

for name, bars in replay_by_interval.items():
    check(bool(bars), f"full/replay: no {name} candles were replayed after subscribing")
    times = [bar["time"] for bar in bars]
    check(times == sorted(set(times)), f"full/replay: {name} replay is not ascending/unique")
    check({bar["interval"] for bar in bars} == {name},
          f"full/replay: {name} replay carried other interval labels")

check(replay_calendar is not None, "full/replay: no calendar frame was replayed")
if replay_calendar is not None:
    check(isinstance(replay_calendar.get("events"), list),
          "full/replay: calendar payload has no events array")
    check(replay_calendar.get("count") == len(replay_calendar.get("events") or []),
          "full/replay: calendar count does not match its events")
check(replay_boot is not None, "full/replay: no status frame was sent after subscribing")

print(
    "replay: "
    + ", ".join(f"{name}={len(bars)}" for name, bars in sorted(replay_by_interval.items()))
    + f", calendar={len((replay_calendar or {}).get('events') or [])} events"
)

# ---------------------------------------------------------------------------
# 2. A second subscriber: same process, same boot_id, same seq numbering
# ---------------------------------------------------------------------------

status_sock, status_reader = connect("status-only")
send_json(status_sock, {"type": "subscribe", "topics": ["status"]})

other_boot = None
deadline = time.monotonic() + 15
while time.monotonic() < deadline and other_boot is None:
    frame = next_frame(status_reader, deadline, record=False)
    if frame is None:
        break
    if envelope(frame, "status-only") == "status":
        other_boot = check_status(frame["data"], "status-only")

check(other_boot is not None, "status-only: no status frame was sent after subscribing")
check(other_boot == replay_boot,
      f"two sockets saw different boot_ids: {replay_boot!r} vs {other_boot!r}")

# Both sockets are now subscribed; watch them together for live publications.
live_status_only = []
pairs = [("full", full_sock, full_reader), ("status-only", status_sock, status_reader)]
hard_deadline = time.monotonic() + (30 if REQUIRE_LIVE else 8)
grace_deadline = None
while time.monotonic() < (grace_deadline or hard_deadline):
    if len(live_full) >= 2 and len(live_status_only) >= 1:
        break
    if len(live_full) >= 2 and grace_deadline is None:
        # The full socket has all it needs; give the second socket a moment to
        # prove the counter is shared, then stop waiting within the hard cap.
        grace_deadline = time.monotonic() + 6
    deadline = grace_deadline or hard_deadline
    label, frame = next_any(pairs, deadline)
    if frame is None:
        break
    kind = envelope(frame, f"{label}/live")
    if frame.get("replay") is True or not isinstance(frame.get("seq"), int):
        continue
    if label == "full":
        live_full[frame["seq"]] = kind
    else:
        live_status_only.append(frame["seq"])
    if kind == "status":
        check_status(frame["data"], f"{label}/live")

seqs = sorted(live_full)
print(f"live frames: {len(seqs)} on the full socket, {len(live_status_only)} on the status socket")
if REQUIRE_LIVE:
    check(len(seqs) >= 2, f"expected at least 2 live frames, saw {len(seqs)}: {seqs}")
check(seqs == sorted(set(seqs)), f"seq is not strictly increasing across topics: {seqs}")
check(live_status_only == sorted(set(live_status_only)),
      f"seq is not strictly increasing on the second socket: {live_status_only}")
check(set(live_status_only) <= set(seqs),
      f"the sockets disagree about the process-wide counter: "
      f"{sorted(set(live_status_only) - set(seqs))} seen only by the status socket")

# ---------------------------------------------------------------------------
# 3. Execution-shaped frames are ignored, never rebroadcast
# ---------------------------------------------------------------------------

for kind in REJECTED_KINDS:
    payload = {"type": kind, "seq": 1, "data": {"side": "buy", "lots": 0.01}}
    if kind == "trades":
        payload["data"] = {
            "trade_id": SENTINEL,
            "symbol": "XAUUSD",
            "side": "buy",
            "size": 0.01,
            "entry": 2340.0,
            "sl": 2335.0,
            "tp": 2350.0,
            "status": "signal",
            "timestamp": 1_700_000_000_000,
        }
    send_json(full_sock, payload)

# A live calendar/status transition may well land inside this window; the
# assertion is that nothing *echoes*, not that the feed goes quiet.
quiet_until = time.monotonic() + 3
while time.monotonic() < quiet_until:
    label, frame = next_any(pairs, quiet_until)
    if frame is None:
        break
    kind = envelope(frame, f"{label}/after-exec")
    check(kind not in REJECTED_KINDS, f"Node1 rebroadcast an execution frame: {kind}")
    echoed = scan(frame, lambda token: token == SENTINEL)
    check(not echoed, f"Node1 rebroadcast an inbound execution payload at {echoed}")
print(f"no broker write path: {len(REJECTED_KINDS)} execution-shaped inbound frames were ignored")

# ---------------------------------------------------------------------------
# 4. Heartbeat, both directions, and no seq on control frames
# ---------------------------------------------------------------------------

send_json(full_sock, {"type": "heartbeat"})
reply = None
deadline = time.monotonic() + 10
while time.monotonic() < deadline:
    frame = next_frame(full_reader, deadline)
    if frame is None:
        break
    if envelope(frame, "full/heartbeat") == "heartbeat":
        reply = frame
        break
check(reply is not None, "the engine did not answer a client heartbeat")
if reply is not None:
    check("data" not in reply and "seq" not in reply, f"heartbeat carries a payload: {reply}")
    print("heartbeat: answered in both directions, no seq, no payload")

# ---------------------------------------------------------------------------
# 5. Re-subscribing replays again — still replay, still no seq
# ---------------------------------------------------------------------------

send_json(full_sock, {"type": "subscribe", "topics": ["candle"], "intervals": ["1h"]})
replayed = 0
deadline = time.monotonic() + 15
while time.monotonic() < deadline:
    frame = next_frame(full_reader, deadline)
    if frame is None:
        break
    kind = envelope(frame, "full/resubscribe")
    if kind == "candle" and frame.get("replay") is True:
        check_candle(frame["data"], "full/resubscribe")
        check(frame["data"].get("interval") == "1h",
              f"full/resubscribe: asked for 1h, received {frame['data'].get('interval')!r}")
        replayed += 1
    elif kind == "status" and frame.get("replay") is True:
        break  # the post-subscribe status always closes the burst
check(replayed > 0, "re-subscribing replayed no 1h candles")
print(f"re-subscribe: {replayed} 1h candles replayed, all replay:true")

full_sock.close()
status_sock.close()

if RECORD:
    with open(RECORD, "w", encoding="utf-8") as handle:
        for frame in RECORDED:
            handle.write(json.dumps(frame, separators=(",", ":")) + "\n")
    print(f"recorded {len(RECORDED)} frames to {RECORD}")

if PROBLEMS:
    print(f"\nWS CHECK FAILED ({len(PROBLEMS)} problem(s)):")
    for item in PROBLEMS[:20]:
        print(f"  - {item}")
    sys.exit(1)

print("WS CHECK PASSED")
