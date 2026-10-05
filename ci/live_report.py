#!/usr/bin/env python3
"""Turn a `ci/live/*` probe into a human-readable markdown report.

usage: live_report.py ci/live > ci/LIVE.md

Everything here is derived from the recorded payloads: it never claims a
timeframe the engine did not actually serve. The point is to answer "what
candles does the API get?" with the timestamps as they came off the wire.
"""

import json
import os
import statistics
import sys
from collections import Counter
from datetime import datetime, timezone


def load(path):
    try:
        with open(path) as fh:
            return json.load(fh)
    except Exception:
        return None


def code(out, name):
    try:
        with open(os.path.join(out, f"{name}.code")) as fh:
            return fh.read().strip()
    except Exception:
        return "----"


def ts(ms):
    if ms is None:
        return "?"
    return datetime.fromtimestamp(ms / 1000, tz=timezone.utc).strftime("%Y-%m-%d %H:%M UTC")


def deltas_min(times):
    return [round((times[i] - times[i - 1]) / 60000.0, 3) for i in range(1, len(times))]


def tf_label(minutes):
    if not minutes:
        return "no bars"
    for span, label in ((1, "1m"), (5, "5m"), (15, "15m"), (30, "30m"), (60, "1h"),
                        (240, "4h"), (1440, "1d")):
        if abs(minutes - span) <= span * 0.1:
            return label
    return f"{minutes:g}m"


def main() -> int:
    out = sys.argv[1] if len(sys.argv) > 1 else "ci/live"
    print("# Live engine probe")
    print()
    print(f"generated: {datetime.now(tz=timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}")
    print()

    # ---- REST status -------------------------------------------------
    status = load(os.path.join(out, "status.json"))
    print("## /status")
    print()
    if status:
        feeds = (status.get("status") or {}).get("feeds") or {}
        candles = status.get("candles")
        vp = status.get("volume_profile")
        print(f"* version `{status.get('version')}` venue `{status.get('venue')}`")
        print(f"* feeds connected: {sum(1 for f in feeds.values() if f.get('state') == 'connected')}"
              f" / {len(feeds)}")
        if candles:
            print(f"* **candles: interval={candles.get('interval')!r} "
                  f"seed_bars={candles.get('seed_bars')}**")
            if candles.get("last_bar"):
                print(f"* history edge: {ts(candles['last_bar'])} "
                      f"(**{candles.get('edge_lag_minutes')} min behind now**) "
                      f"over {candles.get('bars')} bars")
        else:
            print("* no `candles` block — the deployed build predates the candle contract")
        if vp:
            print(f"* volume profile: row_mode={vp.get('row_mode')} rows={vp.get('rows')} "
                  f"bin={vp.get('bin_size')} va={vp.get('va_pct')} input={vp.get('input')}")
        else:
            print("* no `volume_profile` block — the deployed build predates the VP settings report")
    else:
        print(f"unavailable (http {code(out, 'status')})")
    print()

    # ---- candles -----------------------------------------------------
    candles = load(os.path.join(out, "candles.json"))
    print("## /candles")
    print()
    if isinstance(candles, list) and candles:
        times = [c["time"] for c in candles]
        minutes = deltas_min(times)
        common = Counter(minutes).most_common(5)
        fills = sum(1 for c in candles if c["open"] == c["close"])
        runs = []
        i = 0
        while i < len(candles):
            if candles[i]["open"] != candles[i]["close"]:
                i += 1
                continue
            j = i
            while j < len(candles) and candles[j]["open"] == candles[j]["close"]:
                j += 1
            runs.append(j - i)
            i = j
        print(f"* **{len(candles)} bars**, {ts(times[0])} -> {ts(times[-1])}")
        print(f"* deltas (min): {common}  -> **{tf_label(statistics.median(minutes))}**")
        print(f"* open==close bars: {fills} ({fills * 100 // len(candles)}%) in {len(runs)} runs"
              f"{'; longest ' + str(max(runs)) + ' bars' if runs else ''}")
        print(f"* price range: {min(c['low'] for c in candles):.2f} .. "
              f"{max(c['high'] for c in candles):.2f}")
        age_min = (time.time() * 1000 - times[-1]) / 60_000
        print(f"* newest bar is **{age_min:.0f} minutes behind now**"
              + (" -- the REST history seed lags the live stream" if age_min > 120 else ""))
    else:
        print(f"unavailable (http {code(out, 'candles')})")
    print()

    # ---- levels + vp -------------------------------------------------
    print("## /levels")
    print()
    levels = load(os.path.join(out, "levels.json"))
    if isinstance(levels, list) and levels:
        has_meta = any("meta" in lv for lv in levels)
        print(f"* windows: {', '.join(lv.get('window', '?') for lv in levels)}")
        print(f"* `meta` present: **{has_meta}** (the branch adds it)")
        print()
        print("| window | poc | vah | val | start | end |")
        print("|---|---|---|---|---|---|")
        for lv in levels:
            print(f"| {lv.get('window')} | {lv.get('poc')} | {lv.get('vah')} | {lv.get('val')} | "
                  f"{ts(lv.get('start'))} | {ts(lv.get('end'))} |")
    else:
        print(f"unavailable (http {code(out, 'levels')})")
    print()

    print("## /vp audit")
    print()
    for name in ("vp-PW", "vp-PS", "vp-CW", "vp-PW-64", "vp-6h", "vp-24h",
                 "vp-bad", "vp-bad-range"):
        vp = load(os.path.join(out, f"{name}.json"))
        http = code(out, name)
        if http != "200" or not isinstance(vp, dict):
            print(f"* `{name}`: http {http} -> {json.dumps(vp)[:120] if vp else 'no body'}")
            continue
        rows = [r for r in vp.get("histogram", []) if isinstance(r, dict)]
        if name.startswith("vp-6h") or name.startswith("vp-24h"):
            if vp.get("window") != "CUSTOM":
                print(f"* `{name}`: the deployed build predates `?start=&end=` "
                      f"(answered with window={vp.get('window')}) — deploy the branch to "
                      f"check whether SiftingIO's history covers the last hours")
                continue
        print(f"* `{name}`: window={vp.get('window')} rows={vp.get('rows')} "
              f"row_height={vp.get('row_height')} row_mode={vp.get('row_mode')}")
        print(f"  range {vp.get('range_low')}..{vp.get('range_high')} "
              f"input={vp.get('input_interval')} ({vp.get('input_bars')} bars) "
              f"volume={vp.get('total_volume')} va={vp.get('va_pct')}")
        print(f"  poc={vp.get('poc')} (row {vp.get('poc_row')}) "
              f"val={vp.get('val')} (row {vp.get('val_row')}) "
              f"vah={vp.get('vah')} (row {vp.get('vah_row')}) "
              f"histogram rows in payload: {len(rows)}")
    print()

    # ---- websocket ---------------------------------------------------
    print("## WebSocket sample")
    print()
    ws_path = os.path.join(out, "ws-sample.jsonl")
    if not os.path.exists(ws_path):
        print("* no sample recorded")
    else:
        kinds = Counter()
        candle_times, tick_times = [], []
        meta_seen = False
        first_candle = None
        with open(ws_path) as fh:
            for line in fh:
                try:
                    frame = json.loads(line)
                except ValueError:
                    continue
                kind = frame.get("type", frame.get("kind", "?"))
                kinds[kind] += 1
                data = frame.get("data") or {}
                if kind == "candle" and isinstance(data, dict) and "time" in data:
                    candle_times.append(data["time"])
                    if first_candle is None:
                        first_candle = data
                if kind == "tick_volume" and isinstance(data, dict) and "time" in data:
                    tick_times.append(data["time"])
                if kind == "levels" and isinstance(data, dict) and data.get("meta"):
                    meta_seen = True
        print(f"* frames: {dict(kinds)}")
        if candle_times:
            minutes = deltas_min(candle_times)
            print(f"* **live `candle` frames: {len(candle_times)}, deltas (min) "
                  f"{Counter(minutes).most_common(5)} -> "
                  f"{tf_label(statistics.median(minutes)) if minutes else 'single frame'}**")
            print(f"* first frame: {json.dumps(first_candle)[:200]}")
        else:
            print("* no `candle` frame arrived in the sample window (weekend/closed market?)")
        if tick_times:
            minutes = deltas_min(tick_times)
            print(f"* live `tick_volume` deltas (min): {Counter(minutes).most_common(5)} -> "
                  f"{tf_label(statistics.median(minutes)) if minutes else 'single frame'}")
        print(f"* `levels` frames carrying `meta`: **{meta_seen}**")
    print()

    # ---- verdict -----------------------------------------------------
    print("## Verdict")
    print()
    if isinstance(candles, list) and candles:
        minutes = deltas_min([c["time"] for c in candles])
        med = statistics.median(minutes) if minutes else 0
        print(f"* the deployed engine serves **{tf_label(med)}** chart candles "
              f"({len(candles)} bars, {ts(candles[0]['time'])} -> {ts(candles[-1]['time'])})")
    if status and not status.get("candles"):
        print("* the deployed build **predates the current branch** (no `/status.candles`, "
              "no `/vp`): redeploy to pick up the TradingView-parity profile and the audit endpoint")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
