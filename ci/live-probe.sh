#!/usr/bin/env bash
# Live probe of a *deployed* engine.
#
# The sandbox that developed the VP changes cannot reach the Render host, and
# the deployed build is not necessarily the branch head — this script is run by
# a GitHub runner (which can reach it) and commits the public payloads back to
# the branch, so "what is the live engine serving right now?" is answerable
# from the repository instead of from memory.
#
# Read-only: GETs plus one short WebSocket subscription. No keys are used or
# printed (the WS probe sends no credentials).
#
# usage: bash ci/live-probe.sh [base-url] [outdir]
set -uo pipefail

BASE="${1:-${ENGINE_BASE:-https://engine-southeastasia-sng-main.onrender.com}}"
OUT="${2:-ci/live}"
mkdir -p "$OUT"

echo "engine base: $BASE"
echo

fetch() { # fetch <name> <path>
  local name="$1" path="$2" code
  code=$(curl -sS -m 60 --retry 2 --retry-delay 2 -o "$OUT/$name.json" \
    -w '%{http_code}' "$BASE$path" 2>"$OUT/$name.err" || echo 000)
  echo "$code" >"$OUT/$name.code"
  printf '%-18s %s  %s bytes\n' "$name" "$code" "$(wc -c <"$OUT/$name.json" 2>/dev/null || echo 0)"
}

fetch status      /status
fetch levels      /levels
fetch candles     /candles
fetch tick-volume /tick-volume
fetch calendar    /calendar

# Volume-profile audit (added on the current branch; 404 on older builds).
fetch vp-PW       '/vp?window=PW'
fetch vp-PS       '/vp?window=PS'
fetch vp-CW       '/vp?window=CW'
fetch vp-PW-64    '/vp?window=PW&rows=64'
fetch vp-bad      '/vp?window=NOPE'

# Hand-selected ranges, including the two that answer "is SiftingIO's *history*
# current?" — the live stream can be current while the REST history lags.
NOW_MS=$(python3 -c 'import time; print(int(time.time()*1000))')
fetch vp-6h       "/vp?start=$((NOW_MS - 6 * 3600 * 1000))&end=$NOW_MS"
fetch vp-24h      "/vp?start=$((NOW_MS - 24 * 3600 * 1000))&end=$NOW_MS"
fetch vp-bad-range '/vp?start=abc&end=def'

echo
if command -v python3 >/dev/null; then
  python3 -m pip install --quiet websocket-client >/dev/null 2>&1 || true
  python3 ci/ws_probe.py "${BASE/https:/wss:}/ws" "$OUT/ws-sample.jsonl" 45 \
    || echo "ws probe failed"
fi

echo
python3 ci/live_report.py "$OUT" >"$OUT/../LIVE.md" 2>&1 || true
sed -n '1,60p' "$OUT/../LIVE.md"
