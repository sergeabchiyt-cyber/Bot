#!/usr/bin/env python3
"""Proof (runtime): /tick-volume returns live tick-volume bars in the documented shape.

CI has no SiftingIO key, so the array is normally empty there; any bars that
are present must be well-formed and consistent (up + down + flat == ticks).

usage: verify_tick_volume.py <path-to-json>
"""

import json
import sys

KEYS = {
    "time", "ticks", "up_ticks", "down_ticks", "flat_ticks",
    "close", "last_tick", "ticks_per_sec", "closed", "source",
}

bars = json.load(open(sys.argv[1]))
assert isinstance(bars, list), "/tick-volume must return a JSON array"

for i, b in enumerate(bars):
    assert set(b) == KEYS, f"bar {i} keys {sorted(b)}"
    assert b["up_ticks"] + b["down_ticks"] + b["flat_ticks"] == b["ticks"], f"bar {i} split != ticks"
    assert b["ticks"] >= 1 and b["ticks_per_sec"] >= 0, f"bar {i} impossible counts"
    assert b["source"] == "sifting", f"bar {i} source {b['source']}"

times = [b["time"] for b in bars]
assert times == sorted(set(times)), "bars must be unique and oldest-first"
assert all(b["closed"] for b in bars[:-1]), "only the newest bar may be in progress"

print(f"tick-volume: {len(bars)} live bar(s)" + (f", last ticks={bars[-1]['ticks']}" if bars else " (no Sifting feed in this run)"))
print("TICK VOLUME CHECK PASSED")
