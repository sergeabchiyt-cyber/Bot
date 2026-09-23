#!/usr/bin/env python3
"""Proof 1 (runtime): /levels exposes a bounded, rolling PS session window."""
import datetime as dt
import json
import sys

levels = json.load(open(sys.argv[1]))
windows = {lv["window"]: lv for lv in levels}
print("windows present:", sorted(windows))

assert "PS" in windows, f"no PS level computed: {levels}"
ps = windows["PS"]


def fmt(ms):
    return dt.datetime.fromtimestamp(ms / 1000, dt.timezone.utc).isoformat()


assert ps["start"] > 0 and ps["end"] > ps["start"], f"PS window not bounded: {ps}"
span_h = (ps["end"] - ps["start"]) / 3_600_000
assert 23 <= span_h <= 25, f"PS window is not one session ({span_h}h): {ps}"
assert ps["val"] <= ps["poc"] <= ps["vah"], f"bad PS value area: {ps}"
assert ps["poc"] > 0, f"PS poc not computed: {ps}"

# The PS window must END at a 17:00 America/New_York session close.
try:
    from zoneinfo import ZoneInfo

    ny_end = dt.datetime.fromtimestamp(ps["end"] / 1000, ZoneInfo("America/New_York"))
    print("PS window ends at", ny_end.isoformat(), "(New York)")
    assert (ny_end.hour, ny_end.minute) == (17, 0), f"PS does not end at a 17:00 NY close: {ny_end}"
except ImportError:
    print("zoneinfo unavailable; skipped NY-hour assertion")

print(f"PS session window: {fmt(ps['start'])} -> {fmt(ps['end'])} ({span_h:.1f}h)")
print(f"PS poc={ps['poc']:.3f} vah={ps['vah']:.3f} val={ps['val']:.3f}")
for name in ("PW", "CW"):
    if name in windows:
        w = windows[name]
        print(f"{name} poc={w['poc']:.3f} vah={w['vah']:.3f} val={w['val']:.3f}")
print("LEVELS CHECK PASSED")
