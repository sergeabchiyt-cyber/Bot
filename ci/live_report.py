#!/usr/bin/env python3
"""Turn a `ci/live/*` probe into a human-readable markdown report.

usage: live_report.py ci/live > ci/LIVE.md

Everything here is derived from the recorded payloads: it never claims a
timeframe the engine did not actually serve. The point is to answer "what is
the deployed engine serving?" with the timestamps as they came off the wire.
"""

import json
import os
import statistics
import sys
import time
from collections import Counter
from datetime import datetime, timezone

INTERVAL_MS = {"1m": 60_000, "15m": 15 * 60_000, "1h": 60 * 60_000}
RETIRED = ["levels", "vp", "strategy", "demo", "ai"]


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
    for span, label in ((1, "1m"), (15, "15m"), (60, "1h")):
        if abs(minutes - span) <= span * 0.1:
            return label
    return f"{minutes:g}m"


def main() -> int:
    out = sys.argv[1] if len(sys.argv) > 1 else "ci/live"
    print("# Live engine probe")
    print()
    print(f"generated: {datetime.now(tz=timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}")
    print()
    print("Read-only snapshot of the deployed `xauusd-engine` (C1 v2 market feed).")
    print()

    # ---- /status -----------------------------------------------------
    status = load(os.path.join(out, "status.json"))
    print("## /status")
    print()
    contract = (status or {}).get("contract_version")
    if status:
        print(f"* service `{status.get('service')}` version `{status.get('version')}`")
        print(f"* **contract_version `{contract}`** boot_id `{status.get('boot_id')}` "
              f"uptime {int((status.get('uptime_ms') or 0) / 1000)}s")
        if contract != "2.0":
            print("* the deployed build **predates the C1 v2 cutover** — redeploy to serve "
                  "the contract this branch implements")
        freshness = status.get("freshness") or {}
        print(f"* market_open **{status.get('market_open')}**; freshness "
              f"`{freshness.get('state')}` feed `{freshness.get('feed')}` "
              f"age {freshness.get('age_ms')} ms, last tick {ts(freshness.get('last_tick_ms'))}")
        session = status.get("session") or {}
        if session:
            print(f"* session: {ts(session.get('open_ms'))} -> {ts(session.get('close_ms'))} "
                  f"(18:00 America/New_York anchor)")
        intervals = ((status.get("candles") or {}).get("intervals") or {})
        if intervals:
            print()
            print("| interval | bars | capacity | first bar | last closed bar | edge lag |")
            print("|---|---|---|---|---|---|")
            for name in sorted(intervals):
                block = intervals[name]
                print(f"| {name} | {block.get('bars')} | {block.get('capacity')} | "
                      f"{ts(block.get('first_bar'))} | {ts(block.get('last_bar'))} | "
                      f"{block.get('edge_lag_minutes')} min |")
        counters = (status.get("candles") or {}).get("counters")
        if counters:
            print()
            print(f"* ticks: accepted {counters.get('accepted')} / dropped "
                  f"{counters.get('price', 0) + counters.get('seconds_timestamp', 0) + counters.get('future', 0) + counters.get('repeat', 0) + counters.get('late', 0)} "
                  f"(price {counters.get('price', 0)}, seconds-ts {counters.get('seconds_timestamp', 0)}, "
                  f"future {counters.get('future', 0)}, repeat {counters.get('repeat', 0)}, "
                  f"late {counters.get('late', 0)})")
        ws = status.get("ws") or {}
        if ws:
            print(f"* ws: topics {ws.get('topics')} heartbeat {ws.get('heartbeat_secs')}s "
                  f"buffer {ws.get('broadcast_capacity')}")
        calendar = status.get("calendar") or {}
        if calendar:
            print(f"* calendar: source `{calendar.get('source')}` "
                  f"count {calendar.get('count')} updated {ts(calendar.get('updated'))}")
        retired = [name for name in ("strategy", "demo", "vp", "levels", "volume_profile", "ai")
                   if name in status]
        if retired:
            print(f"* **retired sections still present**: {retired}")
    else:
        print(f"unavailable (http {code(out, 'status')})")
    print()

    # ---- /candles ----------------------------------------------------
    print("## /candles")
    print()
    for name in ("1m", "15m", "1h"):
        label = "candles" if name == "15m" else f"candles-{name}"
        candles = load(os.path.join(out, f"{label}.json"))
        print(f"### {name}")
        print()
        if not isinstance(candles, list) or not candles:
            print(f"* unavailable (http {code(out, label)})")
            print()
            continue
        times = [bar["time"] for bar in candles]
        minutes = deltas_min(times)
        step = INTERVAL_MS[name] // 60_000
        on_grid = sum(1 for t in times if t % INTERVAL_MS[name] == 0)
        closed = sum(1 for bar in candles if bar.get("closed") is True)
        sources = sorted({bar.get("source") for bar in candles})
        age_min = (time.time() * 1000 - times[-1]) / 60_000
        print(f"* **{len(candles)} bars**, {ts(times[0])} -> {ts(times[-1])}")
        print(f"* deltas (min): {Counter(minutes).most_common(4)} -> "
              f"**{tf_label(statistics.median(minutes)) if minutes else 'single bar'}**")
        print(f"* on the {name} grid: {on_grid}/{len(times)}; closed: {closed}/{len(candles)}; "
              f"ascending: {times == sorted(set(times))}")
        print(f"* sources: {sources}; price range: "
              f"{min(bar['low'] for bar in candles):.2f} .. {max(bar['high'] for bar in candles):.2f}")
        print(f"* newest bar is **{age_min:.0f} minutes behind now**")
        print()

    # ---- /tick-volume ------------------------------------------------
    print("## /tick-volume")
    print()
    bars = load(os.path.join(out, "tick-volume.json"))
    if isinstance(bars, list):
        if bars:
            last = bars[-1]
            print(f"* {len(bars)} bar(s); newest {ts(last['time'])} closed={last['closed']} "
                  f"ticks={last['ticks']} ({last['up_ticks']}↑ {last['down_ticks']}↓ "
                  f"{last['flat_ticks']}=) at {last['close']}")
        else:
            print("* no live bars (the probe saw a quiet or closed market)")
    else:
        print(f"unavailable (http {code(out, 'tick-volume')})")
    print()

    # ---- /calendar ---------------------------------------------------
    print("## /calendar")
    print()
    calendar = load(os.path.join(out, "calendar.json"))
    if isinstance(calendar, dict):
        print(f"* source `{calendar.get('source')}` count **{calendar.get('count')}** "
              f"updated {ts(calendar.get('updated'))}")
        for event in (calendar.get("events") or [])[:10]:
            print(f"  * {str(event.get('time')):<28} | {event.get('currency'):>4} | "
                  f"{event.get('impact'):<8} | {event.get('event')}")
    else:
        print(f"unavailable (http {code(out, 'calendar')})")
    print()

    # ---- retired surfaces --------------------------------------------
    print("## Retired surfaces")
    print()
    print("C1 v2 has no fields to draw, no strategy and no simulator. A deployed build")
    print("still answering these predates the cutover (404 is the expected answer).")
    print()
    for surface in RETIRED:
        print(f"* `/{surface}` -> http {code(out, f'retired-{surface}')}")
    print()

    # ---- websocket ---------------------------------------------------
    print("## WebSocket sample")
    print()
    ws_path = os.path.join(out, "ws-sample.jsonl")
    if not os.path.exists(ws_path):
        print("* no sample recorded")
    else:
        kinds = Counter()
        live_seqs, replay = [], Counter()
        candle_times = {}
        control_with_seq = 0
        last_candle = None
        with open(ws_path) as fh:
            for line in fh:
                try:
                    frame = json.loads(line)
                except ValueError:
                    continue
                if not isinstance(frame, dict) or "type" not in frame:
                    continue
                kind = frame["type"]
                kinds[kind] += 1
                if frame.get("replay") is True:
                    replay[kind] += 1
                elif isinstance(frame.get("seq"), int):
                    live_seqs.append(frame["seq"])
                elif kind not in {"heartbeat", "gap"}:
                    control_with_seq += 1
                data = frame.get("data")
                if kind == "candle" and isinstance(data, dict) and "time" in data:
                    candle_times.setdefault(data.get("interval"), []).append(data["time"])
                    last_candle = data
        print(f"* frames: {dict(kinds)}")
        print(f"* replay: {dict(replay)}; live (seq'd): {len(live_seqs)}; "
              f"feed frames with neither: {control_with_seq}")
        if live_seqs:
            print(f"* live seq range: {min(live_seqs)} .. {max(live_seqs)}, "
                  f"strictly increasing: {live_seqs == sorted(set(live_seqs))}")
        for name, times in sorted(candle_times.items()):
            minutes = deltas_min(times)
            print(f"* `candle` {name}: {len(times)} frames, deltas (min) "
                  f"{Counter(minutes).most_common(3) or 'n/a'}")
        if last_candle is not None:
            print(f"* last candle frame: {json.dumps(last_candle)[:220]}")
        if not candle_times:
            print("* no `candle` frame arrived in the sample window (closed market?)")
    print()

    # ---- contract matrix ---------------------------------------------
    matrix_path = os.path.join(out, "contract.txt")
    print("## C1 contract matrix")
    print()
    if os.path.exists(matrix_path):
        with open(matrix_path) as fh:
            lines = [line.rstrip() for line in fh if line.strip()]
        for line in lines[-8:]:
            print(f"    {line}")
    else:
        print("* not run in this probe")
    print()

    # ---- verdict -----------------------------------------------------
    print("## Verdict")
    print()
    candles = load(os.path.join(out, "candles.json"))
    if status and contract == "2.0":
        print("* the deployed engine serves **C1 v2**")
    elif status:
        print(f"* the deployed engine serves contract `{contract}` — the branch head is 2.0, "
              f"so the deployment is behind")
    if isinstance(candles, list) and candles:
        times = [bar["time"] for bar in candles]
        minutes = deltas_min(times)
        print(f"* chart candles: **{tf_label(statistics.median(minutes)) if minutes else 'single'}** "
              f"({len(candles)} bars, {ts(times[0])} -> {ts(times[-1])})")
    intervals = ((status or {}).get("candles") or {}).get("intervals") or {}
    lags = {name: block.get("edge_lag_minutes") for name, block in intervals.items()}
    if lags:
        print(f"* edge lag now: {lags}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
