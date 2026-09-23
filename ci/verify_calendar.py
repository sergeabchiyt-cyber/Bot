#!/usr/bin/env python3
"""Proof 2 (runtime): /calendar returns real, normalized ForexFactory events."""
import json
import sys

cal = json.load(open(sys.argv[1]))
print("source:", cal.get("source"), "| count:", cal.get("count"))

events = cal.get("events", [])
assert cal.get("count", 0) > 0 and events, f"calendar is EMPTY: {cal}"

print("--- first 15 events ---")
for e in events[:15]:
    print(
        f"  {str(e.get('time')):<28} | {e['currency']:>4} | {e['impact']:<8} | "
        f"{e['event']}  (F:{e.get('forecast')} P:{e.get('previous')})"
    )

valid_impacts = {"High", "Medium", "Low", "Holiday", "Unknown"}
for e in events:
    assert e["event"].strip(), f"empty event title: {e}"
    assert e["impact"] in valid_impacts, f"unnormalized impact {e['impact']!r}: {e}"

assert any(e["currency"] == "USD" for e in events), "no USD events — field mapping is wrong"

stamps = [e["timestamp"] for e in events if e.get("timestamp")]
assert stamps, "no event carried a parsed timestamp"
assert all(a <= b for a, b in zip(stamps, stamps[1:])), "events are not sorted by time"

print(f"TOTAL EVENTS: {len(events)}")
print(f"WITH TIMESTAMPS: {len(stamps)}")
print("GOLD-RELEVANT (high-impact USD):", sum(1 for e in events if e.get("gold_relevant")))
print("CALENDAR CHECK PASSED")
