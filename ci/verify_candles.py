#!/usr/bin/env python3
"""Proof (runtime): /candles still returns the SiftingIO candle array as-is.

Adding CORS may only add response headers, so this pins the payload shape that
the frontend chart and the backend volume profile both depend on.

usage: verify_candles.py <path-to-json>
"""

import json
import sys
import time

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

# The chart contract is 15M: every bar must sit on the same 15-minute grid the
# engine buckets into, and consecutive bars must be a whole number of buckets
# apart. A regression to 1m/5m bars (or a mislabelled interval) fails here.
FIFTEEN_MIN_MS = 15 * 60_000
off_grid = [t for t in times if t % FIFTEEN_MIN_MS != 0]
assert not off_grid, f"{len(off_grid)} candle(s) off the 15m grid, first: {off_grid[0]}"
deltas = [times[i] - times[i - 1] for i in range(1, len(times))]
ragged = [d for d in deltas if d <= 0 or d % FIFTEEN_MIN_MS != 0]
assert not ragged, f"{len(ragged)} gap(s) that are not whole 15m buckets, first: {ragged[0]}"
regular = sum(1 for d in deltas if d == FIFTEEN_MIN_MS)
print(f"grid: 15m ({regular}/{len(deltas)} gaps are exactly one bucket)")

# A stale history seed leaves a hole between its last bar and the live stream;
# report it instead of letting the chart discover it.
age_min = (time.time() * 1000 - times[-1]) / 60_000
print(f"history edge: {age_min:.0f} minutes behind now")
if age_min > 120:
    print(f"note: the newest candle is {age_min / 60:.1f}h old — /status edge_lag_minutes "
          f"reports this, and the boot tail fetch should have closed the gap")

print(f"candles: {len(candles)} bars, sources={sorted(sources)}")
print(f"first: time={candles[0]['time']} close={candles[0]['close']}")
print(f"last:  time={candles[-1]['time']} close={candles[-1]['close']}")
print("CANDLES CHECK PASSED")
