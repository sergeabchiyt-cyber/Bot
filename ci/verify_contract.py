#!/usr/bin/env python3
"""Contract matrix: every C1 v2 payload the engine serves, checked against the
signed schema in `contracts/`, plus the rules the schema cannot express.

usage: verify_contract.py <payload-dir>

The directory is what `ci/smoke.sh` and `ci/live-probe.sh` write:

    status.json            body of GET /status
    candles-1m.json        body of GET /candles?interval=1m
    candles-15m.json       body of GET /candles?interval=15m
    candles-1h.json        body of GET /candles?interval=1h
    tick-volume.json       body of GET /tick-volume
    calendar.json          body of GET /calendar
    frames.jsonl           every frame a WS client received (one per line)

Checks beyond JSON-Schema:

  * `contract_version` is exactly 2.0 and `boot_id` is a non-empty string;
  * `/status.freshness.state` is one of the four documented states and
    `market_open` agrees with the session block;
  * candles are ascending, unique, bucket-aligned, all `closed: true`, and the
    REST body carries no forming bucket;
  * live frames have a strictly increasing `seq` across all topics, replay
    frames carry `replay: true` and no `seq`, and control frames carry neither;
  * no published frame or payload carries trade/execution vocabulary anywhere.
"""

import json
import os
import sys

import c1_schema

CONTRACT_VERSION = "2.0"
SYMBOL = "XAUUSD"
INTERVAL_MS = {"1m": 60_000, "15m": 15 * 60_000, "1h": 60 * 60_000}
FRESHNESS_STATES = {"live", "stale", "offline", "closed_market"}
FEED_TOPICS = {"candle", "tick_volume", "calendar", "status"}
CONTROL_TOPICS = {"gap", "heartbeat"}
CANDLE_KEYS = {
    "symbol", "interval", "time", "open", "high", "low", "close", "volume",
    "source", "closed",
}

here = os.path.dirname(os.path.abspath(__file__))
SCHEMA = c1_schema.load(os.path.join(here, "..", "contracts", "c1-market-feed.schema.json"))

problems = []


def fail(message):
    problems.append(message)


def check(condition, message):
    if not condition:
        fail(message)
    return condition


def load_json(directory, name):
    path = os.path.join(directory, name)
    if not os.path.exists(path):
        fail(f"{name} is missing from {directory}")
        return None
    try:
        return c1_schema.load(path)
    except json.JSONDecodeError as error:
        fail(f"{name} is not valid JSON: {error}")
        return None


def schema_check(instance, label):
    """Validate against the signed schema, recording (not raising) a violation."""
    try:
        c1_schema.validate(instance, SCHEMA, label)
    except AssertionError as error:
        fail(str(error))


def sweep(document, label):
    banned = c1_schema.forbidden_keys(document)
    if banned:
        fail(f"{label} carries trade vocabulary: {', '.join(banned[:6])}")


# ---------------------------------------------------------------------------
# /status
# ---------------------------------------------------------------------------

status = load_json(sys.argv[1] if len(sys.argv) > 1 else ".", "status.json")
if status is not None:
    sweep(status, "status.json")
    schema_check({"type": "status", "seq": 1, "data": status}, "status.json")

    check(status.get("contract_version") == CONTRACT_VERSION,
          f"contract_version is {status.get('contract_version')!r}, expected {CONTRACT_VERSION!r}")
    boot = status.get("boot_id")
    check(isinstance(boot, str) and boot.strip(), f"boot_id is not a non-empty string: {boot!r}")
    check(isinstance(status.get("uptime_ms"), int) and status["uptime_ms"] >= 0,
          f"uptime_ms is not a non-negative integer: {status.get('uptime_ms')!r}")
    check(isinstance(status.get("service"), str) and status["service"] == "xauusd-engine",
          f"service is {status.get('service')!r}")

    freshness = status.get("freshness") or {}
    check(freshness.get("state") in FRESHNESS_STATES,
          f"freshness.state is {freshness.get('state')!r}")
    check(freshness.get("feed") == "sifting", f"freshness.feed is {freshness.get('feed')!r}")
    check(isinstance(freshness.get("age_ms"), int) and freshness["age_ms"] >= 0,
          f"freshness.age_ms is {freshness.get('age_ms')!r}")
    check(freshness.get("market_open") == status.get("market_open"),
          "freshness.market_open disagrees with /status.market_open")

    session = status.get("session") or {}
    for key in ("open_ms", "close_ms", "market_open"):
        check(key in session, f"/status.session is missing {key!r}")
    check(session.get("market_open") == status.get("market_open"),
          "session.market_open disagrees with /status.market_open")
    if isinstance(session.get("open_ms"), int) and isinstance(session.get("close_ms"), int):
        check(session["close_ms"] > session["open_ms"],
              f"session closes before it opens: {session['open_ms']}..{session['close_ms']}")

    intervals = ((status.get("candles") or {}).get("intervals") or {})
    check(set(intervals) == set(INTERVAL_MS), f"/status.candles.intervals is {sorted(intervals)}")
    for name, block in intervals.items():
        for key in ("bars", "capacity", "forming", "edge_lag_minutes", "first_bar", "last_bar"):
            check(key in block, f"/status.candles.intervals.{name} is missing {key!r}")

    ws = status.get("ws") or {}
    topics = set(ws.get("topics") or [])
    check(topics == FEED_TOPICS, f"/status.ws.topics is {sorted(topics)}")
    check(ws.get("heartbeat_secs") == 30, f"heartbeat_secs is {ws.get('heartbeat_secs')!r}")
    check(ws.get("broadcast_capacity") == 4096, f"broadcast_capacity is {ws.get('broadcast_capacity')!r}")

    # Retired surfaces: a v2 /status must not describe them at all.
    for retired in ("strategy", "demo", "vp", "levels", "volume_profile", "ai"):
        check(retired not in status, f"/status still carries the retired {retired!r} section")

    calendar = status.get("calendar") or {}
    if calendar:
        check("source" in calendar and "count" in calendar and "updated" in calendar,
              f"/status.calendar summary is {sorted(calendar)}")

# ---------------------------------------------------------------------------
# /candles, one resolution at a time
# ---------------------------------------------------------------------------

for name, step in INTERVAL_MS.items():
    candles = load_json(sys.argv[1] if len(sys.argv) > 1 else ".", f"candles-{name}.json")
    if candles is None:
        continue
    sweep(candles, f"candles-{name}.json")
    check(isinstance(candles, list), f"candles-{name}.json is not an array")
    if not isinstance(candles, list) or not candles:
        fail(f"candles-{name}.json is empty (expected a seeded history)")
        continue

    for index, candle in enumerate(candles):
        where = f"candles-{name}[{index}]"
        if not isinstance(candle, dict):
            fail(f"{where}: not an object")
            continue
        schema_check({"type": "candle", "seq": 1, "data": candle}, where)
        if index > 0:
            previous = candles[index - 1]
            if isinstance(previous, dict) and isinstance(candle.get("time"), int):
                if isinstance(previous.get("time"), int):
                    check(
                        candle["time"] > previous["time"],
                        f"{where}: time is not strictly ascending",
                    )
        check(candle.get("interval") == name, f"{where}: interval is {candle.get('interval')!r}")
        check(candle.get("symbol") == SYMBOL, f"{where}: symbol is {candle.get('symbol')!r}")
        time = candle.get("time")
        check(
            isinstance(time, int) and time % step == 0,
            f"{where}: time {time!r} is off the {name} grid",
        )
        check(candle.get("closed") is True, f"{where}: /candles served a forming bucket")
        body = [candle.get(key) for key in ("open", "high", "low", "close")]
        if all(isinstance(value, (int, float)) for value in body):
            open_, high, low, close = body
            check(low <= min(open_, close), f"{where}: body below the low")
            check(high >= max(open_, close), f"{where}: body above the high")
        volume = candle.get("volume")
        check(isinstance(volume, (int, float)) and volume >= 0, f"{where}: bad volume {volume!r}")

    # The most recent bar may not be the bucket the live tick is filling: a
    # forming bar is a WS-only concept in C1.
    newest = candles[-1].get("time") if isinstance(candles[-1], dict) else None
    forming = (
        ((status or {}).get("candles") or {}).get("intervals", {}).get(name, {}).get("forming")
    )
    check(forming != newest, f"candles-{name}: /candles served the forming bucket at {newest}")

# ---------------------------------------------------------------------------
# /tick-volume + /calendar
# ---------------------------------------------------------------------------

tick_volume = load_json(sys.argv[1] if len(sys.argv) > 1 else ".", "tick-volume.json")
if tick_volume is not None:
    sweep(tick_volume, "tick-volume.json")
    check(isinstance(tick_volume, list), "tick-volume.json is not an array")
    bars = [bar for bar in tick_volume or [] if isinstance(bar, dict)]
    times = [bar.get("time") for bar in bars]
    check(times == sorted(set(times)), "tick-volume bars are not unique and oldest-first")
    for index, bar in enumerate(bars):
        check(bar.get("symbol") == SYMBOL, f"tick-volume[{index}]: symbol is {bar.get('symbol')!r}")
        split = [bar.get(key) for key in ("up_ticks", "down_ticks", "flat_ticks", "ticks")]
        if all(isinstance(value, int) for value in split):
            up, down, flat, ticks = split
            check(up + down + flat == ticks,
                  f"tick-volume[{index}]: the up/down/flat split does not add up to ticks")

calendar = load_json(sys.argv[1] if len(sys.argv) > 1 else ".", "calendar.json")
if calendar is not None:
    sweep(calendar, "calendar.json")
    check(isinstance(calendar.get("events"), list), "calendar.json has no events array")
    check(calendar.get("count") == len(calendar.get("events") or []),
          f"calendar count {calendar.get('count')} != {len(calendar.get('events') or [])} events")

# ---------------------------------------------------------------------------
# WS frames: envelope, ordering, and silence on everything else
# ---------------------------------------------------------------------------

frames_path = os.path.join(sys.argv[1] if len(sys.argv) > 1 else ".", "frames.jsonl")
if os.path.exists(frames_path):
    live_seqs, seen_kinds, replay_count = [], set(), 0
    with open(frames_path, encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            line = line.strip()
            if not line:
                continue
            try:
                item = json.loads(line)
            except json.JSONDecodeError as error:
                fail(f"frames.jsonl:{line_number} is not JSON: {error}")
                continue
            label = f"frames.jsonl:{line_number}"
            schema_check(item, label)
            sweep(item, label)

            kind = item.get("type")
            seen_kinds.add(kind)
            if kind in FEED_TOPICS:
                if item.get("replay") is True:
                    replay_count += 1
                    check("seq" not in item, f"{label}: a replay frame carries seq")
                else:
                    seq = item.get("seq")
                    check(isinstance(seq, int), f"{label}: a live frame has no integer seq")
                    if isinstance(seq, int):
                        live_seqs.append(seq)
                    check("replay" not in item, f"{label}: a live frame carries replay")
            else:
                check("seq" not in item, f"{label}: control frame {kind!r} carries seq")
                check("replay" not in item, f"{label}: control frame {kind!r} carries replay")

    unknown = seen_kinds - FEED_TOPICS - CONTROL_TOPICS - {"subscribe"}
    check(not unknown, f"frames.jsonl published unknown frame types: {sorted(unknown)}")
    check(live_seqs == sorted(set(live_seqs)),
          f"seq is not strictly increasing across topics: {live_seqs[:12]}")
    print(f"frames: {len(seen_kinds)} kind(s) {sorted(seen_kinds)}, "
          f"{replay_count} replay, {len(live_seqs)} live (seq {live_seqs[:1]}..{live_seqs[-1:]})")
else:
    print("frames.jsonl not present — envelope checks skipped")

if problems:
    print("\nCONTRACT CHECK FAILED:")
    for problem in problems:
        print(f"  - {problem}")
    sys.exit(1)

print("C1 CONTRACT MATRIX PASSED")
