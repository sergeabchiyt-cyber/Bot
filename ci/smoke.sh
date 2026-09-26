#!/usr/bin/env bash
# Boots the release binary and proves, against the live process:
#   1. /levels exposes a bounded PS window ending on a 17:00 NY session close
#   2. /calendar is actually populated with real ForexFactory events
#   3. CORS lets the Node2 static origin (and only that origin) read the REST API
#   4. /ws still upgrades and replays levels + candles through the CORS layer
#   5. /tick-volume serves well-formed live tick-volume bars
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
echo "--- /candles payload shape (must be untouched by the CORS layer) ---"
curl -sf "$BASE/candles" -o /tmp/candles.json || fail "/candles request failed"
python3 ci/verify_candles.py /tmp/candles.json || fail "/candles verification failed"

echo
echo "--- /tick-volume payload shape ---"
curl -sf "$BASE/tick-volume" -o /tmp/tick_volume.json || fail "/tick-volume request failed"
python3 ci/verify_tick_volume.py /tmp/tick_volume.json || fail "/tick-volume verification failed"

echo
echo "--- CORS: only the Node2 origin may read the REST API ---"
CORS_ALLOWED=${CORS_ALLOWED_ORIGIN:-https://static-dash-frontend.onrender.com}
CORS_FOREIGN=https://evil.example

# Response headers of a request, CR stripped:
#   cors_headers <method> <path> [extra curl args...]
cors_headers() {
  local method=$1 path=$2
  shift 2
  curl -sS -o /dev/null -D - -X "$method" "$@" "$BASE$path" | tr -d '\r'
}

one_line() { printf '%s' "$1" | tr '\n' '|'; }

echo "1) allowed origin gets access-control-allow-origin on every route"
for r in /health /levels /candles /tick-volume /calendar /status; do
  h=$(cors_headers GET "$r" -H "Origin: $CORS_ALLOWED")
  printf '%s\n' "$h" | head -n1 | grep -q " 200" \
    || fail "$r: expected 200, got '$(printf '%s\n' "$h" | head -n1)'"
  printf '%s\n' "$h" | grep -qi "^access-control-allow-origin: $CORS_ALLOWED\$" \
    || fail "$r: missing/wrong allow-origin [$(one_line "$h")]"
  printf '%s\n' "$h" | grep -qi "^vary:.*origin" \
    || fail "$r: missing 'vary: origin' [$(one_line "$h")]"
  echo "  $r -> $(printf '%s\n' "$h" | grep -i '^access-control-allow-origin' | head -n1)"
done

echo "2) preflight (OPTIONS) is authorised for that origin"
h=$(cors_headers OPTIONS /candles -H "Origin: $CORS_ALLOWED" -H "Access-Control-Request-Method: GET")
printf '%s\n' "$h" | grep -qi "^access-control-allow-methods:.*GET" \
  || fail "preflight: GET not in allow-methods [$(one_line "$h")]"
printf '%s\n' "$h" | grep -qi "^access-control-allow-origin: $CORS_ALLOWED\$" \
  || fail "preflight: missing allow-origin [$(one_line "$h")]"
printf '%s\n' "$h" | grep -qi "^access-control-max-age: 600\$" \
  || fail "preflight: missing max-age 600 [$(one_line "$h")]"
printf '  %s\n' "$(one_line "$(printf '%s\n' "$h" | grep -Ei '^(HTTP/|access-control|vary)')")"

echo "3) every other origin gets NO allow-header"
for r in /health /levels /candles /tick-volume /calendar /status; do
  h=$(cors_headers GET "$r" -H "Origin: $CORS_FOREIGN")
  printf '%s\n' "$h" | grep -qi "^access-control-allow-origin" \
    && fail "$r: leaked an allow-header to $CORS_FOREIGN [$(one_line "$h")]"
  echo "  $r (foreign) -> $(printf '%s\n' "$h" | head -n1), no CORS grant"
done
h=$(cors_headers OPTIONS /candles -H "Origin: $CORS_FOREIGN" -H "Access-Control-Request-Method: GET")
printf '%s\n' "$h" | grep -qi "^access-control-allow-origin" \
  && fail "preflight from $CORS_FOREIGN was authorised [$(one_line "$h")]"
echo "  OPTIONS /candles (foreign) -> $(printf '%s\n' "$h" | head -n1), no CORS grant"

echo "4) no wildcard, no credentials, no secrets in the response"
h=$(cors_headers GET /levels -H "Origin: $CORS_ALLOWED")
printf '%s\n' "$h" | grep -qi "^access-control-allow-origin: \*" && fail "wildcard origin in use"
printf '%s\n' "$h" | grep -qi "^access-control-allow-credentials" && fail "allow-credentials is enabled"
printf '%s\n' "$h" | grep -qiE "^(set-cookie|authorization|x-api-key|.*api-key)" && fail "a credential header was exposed"
grep -qiE "sifting_api_key|api_?key|token" /tmp/levels.json /tmp/candles.json \
  && fail "a key-like field appeared in a response body"
echo "  headers are CORS-only"

echo "5) /ws still upgrades and replays frames through the layer"
python3 ci/verify_ws.py "${BASE#*://}" "$CORS_ALLOWED" || fail "/ws no longer streams through the CORS layer"

echo
echo "--- engine log: session/level/calendar/CORS lines ---"
grep -Ei "session|poc=|Calendar|Seeded|CORS" "$ENGINE_LOG" | head -40 || true

echo
echo "SMOKE TEST PASSED"
