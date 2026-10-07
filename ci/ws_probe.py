#!/usr/bin/env python3
"""Sample the deployed engine's WebSocket for a few seconds.

The chart's live candles arrive as `{"type":"candle","data":{...}}` frames, so
this is the only way to record the resolution the *running* engine actually
streams (the REST snapshot can be stale or compacted by the client).

usage: ws_probe.py wss://host/ws out.jsonl [seconds]
"""

import json
import sys
import time

import websocket  # websocket-client

# Node1 publishes market topics only — there is no `trades` topic any more:
# execution lives on Node4.
TOPICS = ["levels", "candle", "tick_volume", "bubbles", "calendar"]


def main() -> int:
    url = sys.argv[1]
    out_path = sys.argv[2]
    seconds = float(sys.argv[3]) if len(sys.argv) > 3 else 45.0

    deadline = time.time() + seconds
    ws = websocket.create_connection(url, timeout=20)
    ws.send(json.dumps({"type": "subscribe", "topics": TOPICS}))

    kinds = {}
    with open(out_path, "w") as out:
        out.write(json.dumps({"kind": "probe", "topics": TOPICS, "seconds": seconds}) + "\n")
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
            if len(json.dumps(frame)) > 20000:
                # Keep the file readable: record the shape, not the whole payload.
                frame = {"type": kind, "truncated": True,
                         "data": {k: v for k, v in list((frame.get("data") or {}).items())[:12]}}
            out.write(json.dumps(frame) + "\n")
    ws.close()
    print("frames:", ", ".join(f"{k}={v}" for k, v in sorted(kinds.items())) or "(none)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
