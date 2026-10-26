#!/usr/bin/env bash
# Boots the release binary and proves, against the live process, that Node 1 is
# a read-only C1 v2 market feed:
#
#   1. /health and /status: contract_version, boot_id, freshness, session, the
#      per-interval candle book and the WS block — and no retired section
#      (strategy / demo / vp / levels / ai)
#   2. /candles?interval=1m|15m|1h: ascending, unique, bucket-aligned, closed,
#      with a 400 for an unknown interval and `60m` accepted as `1h`
#   3. /tick-volume and /calendar (calendar from a local fixture, so the run is
#      hermetic and the first fetch lands *after* the WS clients connect)
#   4. CORS: only the Node 2 origin, GET/OPTIONS only, no credentials
#   5. no write route: POST/PUT/PATCH/DELETE are 405 on every registered path
#   6. /ws: replay burst, live `seq`, heartbeats, and refusal of every
#      execution-shaped inbound frame (ci/verify_ws.py)
#   7. the C1 contract matrix over every payload recorded above
#      (ci/verify_contract.py against contracts/c1-market-feed.schema.json)
set -uo pipefail

BIN=${BIN:-./target/release/xauusd-engine}
PORT=${PORT:-3000}
BASE=${BASE:-http://127.0.0.1:$PORT}
ENGINE_LOG=${ENGINE_LOG:-/tmp/engine.log}
STUB_LOG=${STUB_LOG:-/tmp/calendar-stub.log}
STUB_PORT=${STUB_PORT:-9099}
# The fixture answers late on purpose: the WS clients have to be connected
# before the first successful fetch publishes a live frame.
STUB_DELAY=${STUB_DELAY:-10}
PAYLOADS=${PAYLOADS:-/tmp/c1}
CORS_ALLOWED=${CORS_ALLOWED_ORIGIN:-https://static-dash-frontend.onrender.com}
CORS_FOREIGN=https://evil.example

CLEANUP_PIDS=()
cleanup() {
  for pid in "${CLEANUP_PIDS[@]:-}"; do
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
  done
}
trap cleanup EXIT

fail() {
  echo
  echo "!!! SMOKE TEST FAILED: $*"
  echo "--- engine log (tail 120) ---"
  tail -120 "$ENGINE_LOG" || true
  echo "--- calendar stub log (tail 20) ---"
  tail -20 "$STUB_LOG" || true
  exit 1
}

one_line() { printf '%s' "$1" | tr '\n' '|'; }

# Response headers, CR stripped: cors_headers <method> <path> [extra curl args]
cors_headers() {
  local method=$1 path=$2
  shift 2
  curl -sS -o /dev/null -D - -X "$method" "$@" "$BASE$path" | tr -d '\r'
}

mkdir -p "$PAYLOADS"
rm -f "$PAYLOADS"/*.json "$PAYLOADS"/*.jsonl

echo "--- starting the calendar fixture on 127.0.0.1:$STUB_PORT ---"
python3 ci/calendar_stub.py --port "$STUB_PORT" --delay "$STUB_DELAY" >"$STUB_LOG" 2>&1 &
CLEANUP_PIDS+=("$!")
for _ in $(seq 1 60); do
  if (exec 3<>"/dev/tcp/127.0.0.1/$STUB_PORT") 2>/dev/null; then
    exec 3>&-
    break
  fi
  sleep 0.1
done
(exec 3<>"/dev/tcp/127.0.0.1/$STUB_PORT") 2>/dev/null || fail "calendar fixture never listened"
head -1 "$STUB_LOG" 2>/dev/null || true

echo
echo "--- booting the engine (synthetic seed, no market key) ---"
PORT="$PORT" \
  SEED_SYNTHETIC_CANDLES=1 \
  FEED_SIFTING=0 \
  CALENDAR_USE_MCP=0 \
  CALENDAR_URL="http://127.0.0.1:$STUB_PORT/ff_calendar_thisweek.json" \
  MCP_SCRAPE_SECS=60 \
  CORS_ALLOWED_ORIGIN="$CORS_ALLOWED" \
  RUST_LOG=${RUST_LOG:-info} \
  "$BIN" >"$ENGINE_LOG" 2>&1 &
CLEANUP_PIDS+=("$!")

echo "--- waiting for /health ---"
for _ in $(seq 1 60); do
  if curl -sf "$BASE/health" >/dev/null; then break; fi
  sleep 0.5
done
curl -sf "$BASE/health" || fail "engine never became healthy"
echo " <- /health ok"

# The WS check runs first: the calendar fixture answers ten seconds after the
# first request, so both subscribers are connected when the first live frame
# (calendar + the status transition it causes) is published.
echo
echo "--- /ws: replay burst, live seq, heartbeats, execution-frame refusal ---"
python3 ci/verify_ws.py "${BASE#*://}" "$CORS_ALLOWED" \
  --record "$PAYLOADS/frames.jsonl" --require-live \
  || fail "/ws is not a C1 v2 read-only feed"

echo
echo "--- /status ---"
curl -sf "$BASE/status" -o "$PAYLOADS/status.json" || fail "/status request failed"
python3 - "$PAYLOADS/status.json" <<'PY' || fail "/status does not describe the C1 v2 feed"
import json, sys
status = json.load(open(sys.argv[1]))
print("  service          :", status.get("service"), "v" + str(status.get("version")))
print("  contract_version :", status.get("contract_version"))
print("  boot_id          :", status.get("boot_id"))
print("  market_open      :", status.get("market_open"))
print("  freshness        :", {k: status["freshness"][k] for k in ("feed", "state", "age_ms")})
print("  session          :", status.get("session"))
print("  intervals        :", {k: v["bars"] for k, v in status["candles"]["intervals"].items()})
print("  ws topics        :", status["ws"]["topics"])
assert status["contract_version"] == "2.0", status["contract_version"]
assert status["boot_id"].startswith("boot-"), status["boot_id"]
assert status["freshness"]["state"] in {"live", "stale", "offline", "closed_market"}
for retired in ("strategy", "demo", "vp", "levels", "volume_profile", "ai"):
    assert retired not in status, f"/status still has a {retired} section"
for interval in ("1m", "15m", "1h"):
    block = status["candles"]["intervals"][interval]
    assert block["bars"] > 0, f"no {interval} bars were seeded"
    assert block["last_bar"] % (60_000 * {"1m": 1, "15m": 15, "1h": 60}[interval]) == 0, block
PY

echo
echo "--- /candles: three resolutions, one contract ---"
for INTERVAL in 1m 15m 1h; do
  curl -sf "$BASE/candles?interval=$INTERVAL" -o "$PAYLOADS/candles-$INTERVAL.json" \
    || fail "/candles?interval=$INTERVAL request failed"
  python3 ci/verify_candles.py "$PAYLOADS/candles-$INTERVAL.json" "$INTERVAL" \
    || fail "/candles?interval=$INTERVAL verification failed"
  echo
done

echo "--- /candles interval handling ---"
CODE=$(curl -s -o /tmp/bad-interval.json -w '%{http_code}' "$BASE/candles?interval=5m")
[ "$CODE" = "400" ] || fail "/candles?interval=5m returned $CODE, expected 400"
grep -q '"contract_version":"2.0"' /tmp/bad-interval.json \
  || fail "the 400 body does not name the contract version: $(cat /tmp/bad-interval.json)"
CODE=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/candles?interval=60m")
[ "$CODE" = "200" ] || fail "/candles?interval=60m returned $CODE, expected 200 (1h synonym)"
echo "  ?interval=5m -> 400 (names contract_version), ?interval=60m -> 200"

echo
echo "--- /tick-volume ---"
curl -sf "$BASE/tick-volume" -o "$PAYLOADS/tick-volume.json" || fail "/tick-volume request failed"
python3 ci/verify_tick_volume.py "$PAYLOADS/tick-volume.json" || fail "/tick-volume verification failed"

echo
echo "--- /calendar (hermetic fixture, waiting for the first fetch) ---"
for _ in $(seq 1 40); do
  COUNT=$(curl -sf "$BASE/calendar" \
    | python3 -c 'import sys,json; print(json.load(sys.stdin).get("count",0))' 2>/dev/null || echo 0)
  [ "${COUNT:-0}" -gt 0 ] && break
  sleep 1
done
curl -sf "$BASE/calendar" -o "$PAYLOADS/calendar.json" || fail "/calendar request failed"
python3 ci/verify_calendar.py "$PAYLOADS/calendar.json" || fail "/calendar verification failed"

echo
echo "--- CORS: only the Node 2 origin may read the REST API ---"
echo "1) allowed origin gets access-control-allow-origin on every route"
for r in /health /status /candles /tick-volume /calendar; do
  h=$(cors_headers GET "$r" -H "Origin: $CORS_ALLOWED")
  printf '%s\n' "$h" | head -n1 | grep -q " 200" \
    || fail "$r: expected 200, got '$(printf '%s\n' "$h" | head -n1)'"
  printf '%s\n' "$h" | grep -qi "^access-control-allow-origin: $CORS_ALLOWED\$" \
    || fail "$r: missing/wrong allow-origin [$(one_line "$h")]"
  printf '%s\n' "$h" | grep -qi "^vary:.*origin" \
    || fail "$r: missing 'vary: origin' [$(one_line "$h")]"
  echo "  $r -> $(printf '%s\n' "$h" | grep -i '^access-control-allow-origin' | head -n1)"
done

echo "2) preflight (OPTIONS) is authorised for that origin, GET only"
h=$(cors_headers OPTIONS /candles -H "Origin: $CORS_ALLOWED" -H "Access-Control-Request-Method: GET")
printf '%s\n' "$h" | grep -qi "^access-control-allow-methods:.*GET" \
  || fail "preflight: GET not in allow-methods [$(one_line "$h")]"
printf '%s\n' "$h" | grep -qi "^access-control-allow-origin: $CORS_ALLOWED\$" \
  || fail "preflight: missing allow-origin [$(one_line "$h")]"
printf '%s\n' "$h" | grep -qi "^access-control-max-age: 600\$" \
  || fail "preflight: missing max-age 600 [$(one_line "$h")]"
printf '%s\n' "$h" | grep -qi "POST\|PUT\|PATCH\|DELETE" \
  && fail "preflight advertises a write method [$(one_line "$h")]"
printf '  %s\n' "$(one_line "$(printf '%s\n' "$h" | grep -Ei '^(HTTP/|access-control|vary)')")"

echo "3) every other origin gets NO allow-header"
for r in /health /status /candles /tick-volume /calendar; do
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
h=$(cors_headers GET /status -H "Origin: $CORS_ALLOWED")
printf '%s\n' "$h" | grep -qi "^access-control-allow-origin: \*" && fail "wildcard origin in use"
printf '%s\n' "$h" | grep -qi "^access-control-allow-credentials" && fail "allow-credentials is enabled"
printf '%s\n' "$h" | grep -qiE "^(set-cookie|authorization|x-api-key)" && fail "a credential header was exposed"
grep -qhiE "sifting_api_key|api_?key|token" "$PAYLOADS"/*.json \
  && fail "a key-like field appeared in a response body"
echo "  headers are CORS-only, bodies carry no credentials"

echo
echo "5) no write route: every non-GET method is 405 on every registered path"
for r in /health /status /candles /tick-volume /calendar; do
  for m in POST PUT PATCH DELETE; do
    CODE=$(curl -s -o /dev/null -w '%{http_code}' -X "$m" "$BASE$r")
    [ "$CODE" = "405" ] || fail "$m $r returned $CODE, expected 405 (Node 1 has no write route)"
  done
  echo "  $r: POST/PUT/PATCH/DELETE -> 405"
done

echo
echo "--- C1 contract matrix (contracts/c1-market-feed.schema.json) ---"
python3 ci/verify_contract.py "$PAYLOADS" || fail "a payload violates the C1 v2 contract"

echo
echo "--- engine log: seed / calendar / CORS / rejection lines ---"
grep -Ei "seeded|calendar|cors|ws session|ignored|listening" "$ENGINE_LOG" | head -30 || true

echo
echo "SMOKE TEST PASSED"
