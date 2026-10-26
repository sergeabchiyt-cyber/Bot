#!/usr/bin/env python3
"""Proof (runtime): /candles serves one resolution, correctly.

Node 3 draws PW/PS/CW boundaries from 1h bars and computes FRVP from 1m/5m
precision, so the *same* endpoint has to be exact on every resolution:
ascending, unique, bucket-aligned, fully closed, and labelled with the interval
that was asked for.

usage: verify_candles.py <path-to-json> [interval=15m]
"""

import json
import sys
import time

INTERVAL_MS = {"1m": 60_000, "15m": 15 * 60_000, "1h": 60 * 60_000}
KEYS = {
    "symbol", "interval", "time", "open", "high", "low", "close", "volume",
    "source", "closed",
}

path = sys.argv[1]
interval = sys.argv[2] if len(sys.argv) > 2 else "15m"
assert interval in INTERVAL_MS, f"unknown interval {interval!r}"
step = INTERVAL_MS[interval]

candles = json.load(open(path))
assert isinstance(candles, list), "/candles must return a JSON array"
assert candles, "/candles returned an empty list — the history seed never landed"

off_shape = [i for i, c in enumerate(candles) if set(c) != KEYS]
assert not off_shape, f"{len(off_shape)} candle(s) with unexpected keys, first at index {off_shape[0]}"

wrong_interval = [i for i, c in enumerate(candles) if c["interval"] != interval]
assert not wrong_interval, (
    f"{len(wrong_interval)} candle(s) labelled with another interval, first at index "
    f"{wrong_interval[0]}: {candles[wrong_interval[0]]['interval']!r} != {interval!r}"
)

wrong_symbol = [c for c in candles if c["symbol"] != "XAUUSD"]
assert not wrong_symbol, f"unexpected symbol: {wrong_symbol[0]['symbol']!r}"

forming = [i for i, c in enumerate(candles) if not c["closed"]]
assert not forming, (
    f"/candles served {len(forming)} forming bucket(s), first at index {forming[0]}; "
    "a forming bar only exists on the WS stream"
)

times = [c["time"] for c in candles]
duplicates = [i for i in range(1, len(times)) if times[i] == times[i - 1]]
assert not duplicates, f"duplicate candle time at index {duplicates[0]}: {times[duplicates[0]]}"
backwards = [i for i in range(1, len(times)) if times[i] < times[i - 1]]
assert not backwards, f"candle times go backwards at index {backwards[0]}"

off_grid = [t for t in times if t % step != 0]
assert not off_grid, f"{len(off_grid)} candle(s) off the {interval} grid, first: {off_grid[0]}"
ragged = [t for i, t in enumerate(times[1:], 1) if (t - times[i - 1]) % step != 0]
assert not ragged, f"{len(ragged)} gap(s) that are not whole {interval} buckets, first after {ragged[0]}"

impossible = [
    c for c in candles
    if not (
        c["low"] <= c["high"]
        and c["low"] <= min(c["open"], c["close"])
        and c["high"] >= max(c["open"], c["close"])
        and c["volume"] >= 0
    )
]
assert not impossible, f"impossible candle values, first: {impossible[0]}"

sources = {c["source"] for c in candles}
assert sources <= {"sifting", "synthetic"}, f"unexpected candle source(s): {sorted(sources)}"

# Node 3 needs enough history to anchor PW (a whole week of 1h bars) and to
# fold a 128-row profile out of minutes.
if interval == "15m":
    assert len(candles) >= 1000, f"only {len(candles)} 15m bars; expected the ~2,000-bar seed"
elif interval == "1h":
    assert len(candles) >= 48, f"only {len(candles)} 1h bars; a week of boundary history is 120"
else:
    assert len(candles) >= 240, f"only {len(candles)} 1m bars; expected a seeded minute history"

regular = sum(1 for i in range(1, len(times)) if times[i] - times[i - 1] == step)
age_min = (time.time() * 1000 - times[-1]) / 60_000
print(f"interval: {interval} ({step // 60_000}m buckets)")
print(f"grid: {regular}/{len(times) - 1} gaps are exactly one bucket")
print(f"history edge: {age_min:.0f} minutes behind now")
if age_min > 120:
    print(f"note: the newest candle is {age_min / 60:.1f}h old — /status edge_lag_minutes reports "
          f"this, and the boot tail fetch should have closed the gap")
print(f"candles: {len(candles)} bars, sources={sorted(sources)}")
print(f"first: time={candles[0]['time']} close={candles[0]['close']}")
print(f"last:  time={candles[-1]['time']} close={candles[-1]['close']}")
print(f"CANDLES CHECK PASSED ({interval})")
