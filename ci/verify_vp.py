#!/usr/bin/env python3
"""Proof (runtime): /vp exposes the TradingView-parity histogram.

The audit payload is what makes a mismatch with TradingView debuggable, so the
shape and the internal consistency of the numbers are pinned here:

  * rows are contiguous price bands anchored at the profile low
  * every row's volume sums to `total_volume` (tick counts, nothing lost)
  * `up_volume + down_volume == volume` on every row
  * the POC row is really the heaviest row, and VAL <= POC <= VAH
  * the reported POC/VAL/VAH equal the row geometry (centre / low edge /
    high edge) that a TradingView profile line is drawn at

usage: verify_vp.py <path-to-json> [expected-window]
"""

import json
import sys

TOL = 1e-6
KNOWN_INTERVALS = {"1m", "5m", "15m", "30m", "1h", "4h", "1d"}


def close(a, b, tol=TOL):
    return abs(a - b) <= tol * max(1.0, abs(a), abs(b))


path = sys.argv[1]
expected = sys.argv[2] if len(sys.argv) > 2 else "PS"
audit = json.load(open(path))

assert audit["window"] == expected, f"expected window {expected}, got {audit['window']}"
assert audit["end"] > audit["start"], f"unbounded window: {audit}"

rows = audit["histogram"]
assert isinstance(rows, list) and rows, "/vp returned no histogram rows"
assert audit["rows"] == len(rows), f"rows={audit['rows']} but {len(rows)} row(s) returned"
assert audit["range_high"] > audit["range_low"], f"empty price range: {audit['range_low']}..{audit['range_high']}"
assert audit["total_volume"] > 0, "profile has no volume"
assert audit["row_mode"] in {"rows", "price"}, f"unknown row mode: {audit['row_mode']}"
assert audit["input_interval"] in KNOWN_INTERVALS, f"unexpected input interval: {audit['input_interval']}"
assert audit["input_bars"] > 0, "profile was built from zero bars"
assert 0 < audit["va_pct"] <= 1, f"value area pct out of range: {audit['va_pct']}"

# Contiguous, ascending price bands. Rows mode anchors on the profile low;
# the fixed-$ grid may start at the first multiple below it.
assert close(rows[0]["low"], audit["range_low"]) or audit["row_mode"] == "price", (
    f"first row {rows[0]['low']} does not start at the profile low {audit['range_low']}"
)
for i in range(1, len(rows)):
    assert close(rows[i]["low"], rows[i - 1]["high"]), (
        f"gap between rows {i - 1} and {i}: {rows[i - 1]['high']} -> {rows[i]['low']}"
    )
    assert rows[i]["high"] > rows[i]["low"], f"degenerate row {i}: {rows[i]}"

height = rows[0]["high"] - rows[0]["low"]
assert height > 0 and close(audit["row_height"], height), (
    f"row_height {audit['row_height']} != first row height {height}"
)
assert audit["range_high"] <= rows[-1]["high"] + TOL, (
    "the histogram does not reach the profile high"
)

total = sum(r["volume"] for r in rows)
assert close(total, audit["total_volume"], 1e-9), f"row volumes {total} != total_volume {audit['total_volume']}"
for i, row in enumerate(rows):
    assert row["volume"] >= 0 and row["up_volume"] >= 0 and row["down_volume"] >= 0, f"negative volume in row {i}"
    assert close(row["up_volume"] + row["down_volume"], row["volume"], 1e-9), (
        f"row {i}: up+down {row['up_volume'] + row['down_volume']} != total {row['volume']}"
    )
    assert rows[i]["high"] > rows[i]["price"] > rows[i]["low"] or row["low"] == row["high"], (
        f"row {i} price {row['price']} is not inside {row['low']}..{row['high']}"
    )

heaviest = max(range(len(rows)), key=lambda i: rows[i]["volume"])
assert audit["poc_row"] == heaviest, (
    f"POC row {audit['poc_row']} is not the heaviest row ({heaviest})"
)
assert audit["val_row"] <= audit["poc_row"] <= audit["vah_row"], "value area does not contain the POC"
assert close(audit["poc"], rows[audit["poc_row"]]["price"]), "poc does not match the POC row centre"
assert close(audit["val"], rows[audit["val_row"]]["low"]), "val does not match the value-area low edge"
assert close(audit["vah"], rows[audit["vah_row"]]["high"]), "vah does not match the value-area high edge"

print(f"window={audit['window']} row_mode={audit['row_mode']} rows={len(rows)} row_height={audit['row_height']:.4f}")
print(
    f"range {audit['range_low']:.3f}..{audit['range_high']:.3f} "
    f"input={audit['input_interval']} ({audit['input_bars']} bars) "
    f"total_volume={audit['total_volume']:.0f}"
)
print(f"poc={audit['poc']:.3f} (row {audit['poc_row']}) val={audit['val']:.3f} vah={audit['vah']:.3f}")
print("VP CHECK PASSED")
