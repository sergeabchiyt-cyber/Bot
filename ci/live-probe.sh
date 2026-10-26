#!/usr/bin/env bash
# Live probe of a *deployed* engine.
#
# The development sandbox cannot reach the Render host, and the deployed build
# is not necessarily the branch head — this script is run by a GitHub runner
# (which can reach it) and commits the public payloads back to the branch, so
# "what is the live engine serving right now?" is answerable from the repository
# instead of from memory.
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
  printf '%-20s %s  %s bytes\n' "$name" "$code" "$(wc -c <"$OUT/$name.json" 2>/dev/null || echo 0)"
}

fetch status       /status
fetch candles      '/candles?interval=15m'
fetch candles-1m   '/candles?interval=1m'
fetch candles-1h   '/candles?interval=1h'
fetch tick-volume  /tick-volume
fetch calendar     /calendar
fetch candles-bad  '/candles?interval=5m'

# `verify_contract.py` looks for a fixed set of names; the 15m snapshot is the
# chart contract, so it keeps its golden file name and gains the alias.
cp "$OUT/candles.json" "$OUT/candles-15m.json" 2>/dev/null || true

echo
echo "--- retired surfaces (C1 v2 has no levels, vp, strategy, demo or ai) ---"
for surface in levels vp strategy demo ai; do
  code=$(curl -sS -m 30 -o /dev/null -w '%{http_code}' "$BASE/$surface" || echo 000)
  echo "$code" >"$OUT/retired-$surface.code"
  printf '%-20s %s\n' "/$surface" "$code"
done

echo
echo "--- WebSocket sample ---"
if command -v python3 >/dev/null; then
  python3 -m pip install --quiet websocket-client >/dev/null 2>&1 || true
  python3 ci/ws_probe.py "${BASE/https:/wss:}/ws" "$OUT/ws-sample.jsonl" 45 \
    || echo "ws probe failed"
  # Only frame lines carry a `type`; the probe header/footer do not.
  python3 - "$OUT/ws-sample.jsonl" "$OUT/frames.jsonl" <<'PY'
import json, sys
kept = 0
with open(sys.argv[1]) as src, open(sys.argv[2], "w") as dst:
    for line in src:
        line = line.strip()
        if not line:
            continue
        try:
            frame = json.loads(line)
        except ValueError:
            continue
        if isinstance(frame, dict) and "type" in frame:
            dst.write(json.dumps(frame) + "\n")
            kept += 1
print(f"recorded frames: {kept}")
PY
fi

echo
echo "--- C1 contract matrix over the recorded payloads ---"
python3 ci/verify_contract.py "$OUT" >"$OUT/contract.txt" 2>&1 || true
tail -25 "$OUT/contract.txt"

echo
python3 ci/live_report.py "$OUT" >"$OUT/../LIVE.md" 2>&1 || true
sed -n '1,70p' "$OUT/../LIVE.md"
