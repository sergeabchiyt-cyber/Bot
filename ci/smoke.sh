#!/usr/bin/env bash
# Boots the release binary and proves, against the live process:
#   1. /levels exposes a bounded PS window ending on a 17:00 NY session close
#   2. /calendar is actually populated with real ForexFactory events
set -uo pipefail

BIN=${BIN:-./target/release/xauusd-engine}
BASE=${BASE:-http://127.0.0.1:3000}
ENGINE_LOG=${ENGINE_LOG:-/tmp/engine.log}

SEED_SYNTHETIC_CANDLES=${SEED_SYNTHETIC_CANDLES:-1} \
RUST_LOG=${RUST_LOG:-info} "$BIN" >"$ENGINE_LOG" 2>&1 &
PID=$!
cleanup() { kill "$PID" 2>/dev/null || true; }
trap cleanup EXIT

fail() {
  echo
  echo "!!! SMOKE TEST FAILED: $*"
  echo "--- engine log (tail 120) ---"
  tail -120 "$ENGINE_LOG" || true
  exit 1
}

echo "--- waiting for /health ---"
for _ in $(seq 1 30); do
  if curl -sf "$BASE/health" >/dev/null; then break; fi
  sleep 1
done
curl -sf "$BASE/health" || fail "engine never became healthy"
echo " <- /health ok"
echo
echo "--- /status ---"
curl -sf "$BASE/status" | head -c 2000
echo

echo
echo "--- /levels ---"
curl -sf "$BASE/levels" -o /tmp/levels.json || fail "/levels request failed"
cat /tmp/levels.json
echo
python3 ci/verify_levels.py /tmp/levels.json || fail "/levels verification failed"

echo
echo "--- /calendar (waiting for first fetch) ---"
for _ in $(seq 1 30); do
  COUNT=$(curl -sf "$BASE/calendar" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin).get("count",0))' 2>/dev/null || echo 0)
  if [ "${COUNT:-0}" -gt 0 ]; then break; fi
  sleep 2
done
curl -sf "$BASE/calendar" -o /tmp/cal.json || fail "/calendar request failed"
python3 ci/verify_calendar.py /tmp/cal.json || fail "/calendar verification failed"

echo
echo "--- engine log: session/level/calendar lines ---"
grep -Ei "session|poc=|Calendar|Seeded" "$ENGINE_LOG" | head -40 || true

echo
echo "SMOKE TEST PASSED"
