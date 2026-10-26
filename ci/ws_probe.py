#!/usr/bin/env python3
"""Sample a *deployed* engine's WebSocket for a few seconds.

The live stream is the only place to see the real envelope (`seq` ordering,
`replay` bursts, which resolutions actually arrive), so the probe records it
verbatim into `ci/live/ws-sample.jsonl` and lets `live_report.py` summarize.

C1 v2 topics are `candle`, `tick_volume`, `calendar`, `status`; the client may
only send `subscribe` and `heartbeat`. Anything execution-shaped that a probe
might send on purpose (to prove it is refused) is not recorded here — that is
`ci/verify_ws.py`'s job, against a process this repository controls.

usage: ws_probe.py wss://host/ws out.jsonl [seconds]
"""

import json
import sys
import time

import websocket  # websocket-client

TOPICS = ["candle", "tick_volume", "calendar", "status"]
INTERVALS = ["1m", "15m", "1h"]
# A calendar frame carries the whole week; keep the file readable.
MAX_FRAME = 8_000


def compact(frame):
    """Keep the envelope always, trim only the payload that would swamp the file."""
    kind = frame.get("type")
    data = frame.get("data")
    if kind == "calendar" and isinstance(data, dict):
        slim = {key: data.get(key) for key in ("source", "count", "updated")}
        slim["events"] = len(data.get("events") or [])
        return {**{k: v for k, v in frame.items() if k != "data"}, "data": slim}
    encoded = json.dumps(frame)
    if len(encoded) > MAX_FRAME:
        return {
            "type": kind,
            "seq": frame.get("seq"),
            "replay": frame.get("replay"),
            "truncated": True,
            "bytes": len(encoded),
        }
    return frame


def main() -> int:
    url = sys.argv[1]
    out_path = sys.argv[2]
    seconds = float(sys.argv[3]) if len(sys.argv) > 3 else 45.0

    deadline = time.time() + seconds
    ws = websocket.create_connection(url, timeout=20)
    ws.send(json.dumps({"type": "subscribe", "topics": TOPICS, "intervals": INTERVALS}))

    kinds = {}
    with open(out_path, "w") as out:
        out.write(json.dumps({"kind": "probe", "topics": TOPICS,
                              "intervals": INTERVALS, "seconds": seconds}) + "\n")
        while time.time() < deadline:
            try:
                ws.settimeout(max(1.0, deadline - time.time()))
                raw = ws.recv()
            except Exception as e:  # timeout / closed
                out.write(json.dumps({"kind": "probe-end", "error": str(e)}) + "\n")
                break
            if not raw:
                continue
            try:
                frame = json.loads(raw)
            except ValueError:
                out.write(json.dumps({"kind": "unparsed", "raw": raw[:200]}) + "\n")
                continue
            kind = frame.get("type", "?")
            kinds[kind] = kinds.get(kind, 0) + 1
            out.write(json.dumps(compact(frame)) + "\n")
    ws.close()
    print("frames:", ", ".join(f"{k}={v}" for k, v in sorted(kinds.items())) or "(none)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
