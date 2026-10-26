#!/usr/bin/env python3
"""Serve the ForexFactory weekly JSON shape from localhost.

The runtime smoke test needs a calendar that is (a) hermetic — hosted runners
must not depend on forexfactory's CDN being up — and (b) *late*, so the first
successful fetch lands while the WS client is already connected. A successful
fetch flips the feed state and publishes a live `calendar` frame, which is what
proves the process-wide `seq` counter end to end.

usage: calendar_stub.py [--port 9099] [--delay 6] [--fixture PATH]

Any path answers with the fixture, so `CALENDAR_URL` can point at
`http://127.0.0.1:9099/ff_calendar_thisweek.json`.
"""

import argparse
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# A miniature of the real export: USD high/medium, a non-USD low, and an
# all-day holiday (no timestamp). `parse_ff_json` sorts by time and normalizes
# the impact strings, so the recorded payload exercises both.
FIXTURE = [
    {
        "title": "Core CPI m/m",
        "country": "USD",
        "date": "2026-10-27T08:30:00-04:00",
        "impact": "High",
        "forecast": "0.3%",
        "previous": "0.2%",
    },
    {
        "title": "FOMC Member Barr Speaks",
        "country": "USD",
        "date": "2026-10-27T12:15:00-04:00",
        "impact": "Medium",
        "forecast": "",
        "previous": "",
    },
    {
        "title": "German ifo Business Climate",
        "country": "EUR",
        "date": "2026-10-28T05:00:00-04:00",
        "impact": "Low",
        "forecast": "88.0",
        "previous": "87.6",
    },
    {
        "title": "Bank Holiday",
        "country": "ALL",
        "date": "2026-10-29T00:00:00-04:00",
        "impact": "Holiday",
        "forecast": "",
        "previous": "",
    },
]


class Handler(BaseHTTPRequestHandler):
    delay = 0.0
    body = b"[]"

    def do_GET(self):  # noqa: N802 - http.server naming
        # Hold the response long enough for the WS clients to subscribe first.
        time.sleep(self.delay)
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(self.body)))
        self.end_headers()
        self.wfile.write(self.body)

    def log_message(self, fmt, *args):  # keep the smoke log readable
        print(f"[calendar-stub] {fmt % args}", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=9099)
    parser.add_argument("--delay", type=float, default=6.0)
    parser.add_argument("--fixture", default=None)
    args = parser.parse_args()

    if args.fixture:
        with open(args.fixture, encoding="utf-8") as handle:
            payload = json.load(handle)
    else:
        payload = FIXTURE

    Handler.delay = args.delay
    Handler.body = json.dumps(payload).encode()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(
        f"calendar stub on 127.0.0.1:{args.port} "
        f"({len(payload)} events, {args.delay:g}s delay)",
        flush=True,
    )
    server.serve_forever()


if __name__ == "__main__":
    main()
