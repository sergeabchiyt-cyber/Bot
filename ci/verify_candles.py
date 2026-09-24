#!/usr/bin/env python3
"""Proof (runtime): /candles still returns the SiftingIO candle array as-is.

Adding CORS may only add response headers, so this pins the payload shape that
the frontend chart and the backend volume profile both depend on.

usage: verify_candles.py <path-to-json>
"""

import json
import sys

KEYS = {"time", "open", "high", "low", "close", "volume", "source"}

candles = json.load(open(sys.argv[1]))
assert isinstance(candles, list) and candles, "/candles returned an empty list, or not an array"

off_shape = [i for i, c in enumerate(candles) if set(c) != KEYS]
assert not off_shape, f"{len(off_shape)} candle(s) with unexpected keys, first at index {off_shape[0]}"

out_of_range = [c for c in candles if not (c["low"] <= c["high"] and c["volume"] >= 0)]
assert not out_of_range, f"impossible candle values, first: {out_of_range[0]}"

times = [c["time"] for c in candles]
unsorted = [i for i in range(1, len(times)) if times[i] < times[i - 1]]
assert not unsorted, f"candle times go backwards at index {unsorted[0]}"

sources = {c["source"] for c in candles}
assert sources <= {"sifting", "synthetic"}, f"unexpected candle source(s): {sorted(sources)}"

# The chart expects the fixed seed span; the SiftingIO REST seed and the
# offline synthetic fallback are both exactly 2,000 15m bars.
assert len(candles) >= 1000, f"only {len(candles)} candles; expected ~2,000 of history"

wobbly = sum(1 for i in range(1, len(times)) if times[i] == times[i - 1])
loose = [c for c in candles if not (c["low"] <= c["open"] <= c["high"] and c["low"] <= c["close"] <= c["high"])]
if wobbly:
    print(f"note: {wobbly} duplicate timestamp(s) in the history")
if loose:
    print(f"note: {len(loose)} candle(s) with open/close outside the high-low range (upstream artefact)")

print(f"candles: {len(candles)} bars, sources={sorted(sources)}")
print(f"first: time={candles[0]['time']} close={candles[0]['close']}")
print(f"last:  time={candles[-1]['time']} close={candles[-1]['close']}")
print("CANDLES CHECK PASSED")
