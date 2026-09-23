#!/usr/bin/env bash
# Boots the release binary and proves, against the live process:
#   1. /levels exposes a bounded PS window ending on a 17:00 NY session close
#   2. /calendar is actually populated with real ForexFactory events
set -euo pipefail

BIN=${BIN:-./target/release/xauusd-engine}
BASE=${BASE:-http://127.0.0.1:3000}

"$BIN" &
PID=$!
cleanup() { kill "$PID" 2>/dev/null || true; }
trap cleanup EXIT

echo "--- waiting for /health ---"
for _ in $(seq 1 30); do
  if curl -sf "$BASE/health" >/dev/null; then break; fi
  sleep 1
done
curl -sf "$BASE/health" && echo " <- /health ok"
echo
echo "--- /status ---"
curl -sf "$BASE/status" | head -c 2000
echo

echo
echo "--- /levels ---"
curl -sf "$BASE/levels" -o /tmp/levels.json
cat /tmp/levels.json
echo
python3 ci/verify_levels.py /tmp/levels.json

echo
echo "--- /calendar (waiting for first fetch) ---"
for _ in $(seq 1 30); do
  COUNT=$(curl -sf "$BASE/calendar" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin).get("count",0))' 2>/dev/null || echo 0)
  if [ "${COUNT:-0}" -gt 0 ]; then break; fi
  sleep 2
done
curl -sf "$BASE/calendar" -o /tmp/cal.json
python3 ci/verify_calendar.py /tmp/cal.json

echo
echo "SMOKE TEST PASSED"
