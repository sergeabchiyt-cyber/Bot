#!/usr/bin/env python3
"""Probe the deployed Node 4 service (/diagnostics + /ws) and record its shape.

Runs the exact contract checks requested for ExecutionSnapshot:
1. GET /diagnostics -> keys, build (.work.service + " " + .work.version),
   role (.work.role), has("market_feed"), has("strategy").
2. WS /ws with {"type":"subscribe","topics":["diagnostics"]} -> verify the
   {"type":"diagnostics","data":{...}} frame carries the same object shape.

Also supports probing a local URL (e.g. http://127.0.0.1:10000) so CI can
verify the freshly built binary alongside the deployed Render instance.
"""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
from typing import Any

try:
    import websockets
except ImportError:
    websockets = None  # type: ignore[assignment]

DEFAULT_HOST = os.environ.get(
    "NODE4_PROBE_HOST", "https://execution-southeastasia-sng-main.onrender.com"
)
EXPECTED_KEYS = [
    "deriv_account",
    "market_feed",
    "mt5",
    "node3",
    "open_trades",
    "recent_events",
    "strategy",
    "timestamp",
    "work",
]


def summarize_snapshot(data: dict[str, Any]) -> dict[str, Any]:
    work = data.get("work") if isinstance(data.get("work"), dict) else {}
    service = work.get("service") or ""
    version = work.get("version") or ""
    role = work.get("role")
    return {
        "build": f"{service} {version}".strip(),
        "role": role,
        "has_market_feed": "market_feed" in data,
        "has_strategy": "strategy" in data,
        "keys": sorted(data.keys()),
    }


def fetch_http(url: str, timeout: float = 45.0) -> tuple[int, dict[str, str], str]:
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            headers = {k.lower(): v for k, v in resp.headers.items()}
            body = resp.read().decode("utf-8", errors="replace")
            return resp.status, headers, body
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        headers = {k.lower(): v for k, v in exc.headers.items()} if exc.headers else {}
        return exc.code, headers, body
    except Exception as exc:  # noqa: BLE001
        return 0, {}, f"ERROR: {exc}"


async def probe_ws(ws_url: str, timeout: float = 30.0) -> dict[str, Any]:
    if websockets is None:
        return {"ok": False, "error": "websockets package not installed"}
    try:
        async with websockets.connect(ws_url, open_timeout=timeout, close_timeout=5) as ws:
            await ws.send(json.dumps({"type": "subscribe", "topics": ["diagnostics"]}))
            deadline = asyncio.get_event_loop().time() + timeout
            frames_seen: list[str] = []
            while asyncio.get_event_loop().time() < deadline:
                remaining = max(0.5, deadline - asyncio.get_event_loop().time())
                raw = await asyncio.wait_for(ws.recv(), timeout=remaining)
                msg = json.loads(raw)
                msg_type = msg.get("type") or msg.get("topic") or "unknown"
                frames_seen.append(str(msg_type))
                if msg_type == "diagnostics":
                    payload = msg.get("data") if isinstance(msg.get("data"), dict) else msg
                    return {
                        "ok": True,
                        "frame_type": msg_type,
                        "frames_seen": frames_seen,
                        "summary": summarize_snapshot(payload),
                        "payload": payload,
                    }
            return {"ok": False, "frames_seen": frames_seen, "error": "no diagnostics frame received"}
    except Exception as exc:  # noqa: BLE001
        return {"ok": False, "error": f"{type(exc).__name__}: {exc}"}


def run_jq_check(raw_json: str) -> str:
    expr = (
        '{build: (.work.service + " " + .work.version), '
        'role: .work.role, '
        'has_market_feed: has("market_feed"), '
        'has_strategy: has("strategy"), '
        'keys: (keys|sort)}'
    )
    try:
        proc = subprocess.run(
            ["jq", expr],
            input=raw_json,
            text=True,
            capture_output=True,
            timeout=10,
            check=False,
        )
        if proc.returncode == 0:
            return proc.stdout.strip()
        return f"jq error ({proc.returncode}): {proc.stderr.strip()}"
    except FileNotFoundError:
        try:
            return json.dumps(summarize_snapshot(json.loads(raw_json)), indent=2)
        except Exception as exc:  # noqa: BLE001
            return f"parse error: {exc}"


def probe_target(base_url: str) -> dict[str, Any]:
    base = base_url.rstrip("/")
    diag_url = f"{base}/diagnostics"
    ws_url = base.replace("https://", "wss://").replace("http://", "ws://") + "/ws"

    print(f"=== Target: {base}")
    status, headers, body = fetch_http(diag_url)
    print(f"GET {diag_url} -> HTTP {status} ({len(body)} bytes)")
    for h in ("server", "via", "cf-ray", "x-render-origin-server", "content-type"):
        if h in headers:
            print(f"  header {h}: {headers[h]}")

    parsed: dict[str, Any] = {}
    summary: dict[str, Any] = {}
    if status == 200:
        print("\n--- jq check on GET /diagnostics ---")
        print(run_jq_check(body))
        try:
            parsed = json.loads(body)
            if isinstance(parsed, dict):
                summary = summarize_snapshot(parsed)
                print("\n--- work & node3 excerpt ---")
                print(
                    json.dumps(
                        {
                            "work": parsed.get("work"),
                            "node3": parsed.get("node3"),
                            "has_market_feed": "market_feed" in parsed,
                            "has_strategy": "strategy" in parsed,
                            "market_feed_state": (parsed.get("market_feed") or {}).get("state")
                            if isinstance(parsed.get("market_feed"), dict)
                            else None,
                            "strategy_mode": (parsed.get("strategy") or {}).get("mode")
                            if isinstance(parsed.get("strategy"), dict)
                            else None,
                            "recent_events": (parsed.get("recent_events") or [])[:5],
                        },
                        indent=2,
                    )
                )
        except Exception as exc:  # noqa: BLE001
            print(f"JSON decode error: {exc}")
    else:
        print(f"Body: {body[:500]}")

    print(f"\n--- WS {ws_url} subscribe diagnostics ---")
    ws_res = asyncio.run(probe_ws(ws_url))
    ws_print = {k: v for k, v in ws_res.items() if k != "payload"}
    print(json.dumps(ws_print, indent=2))
    print()
    return {
        "base": base,
        "http_status": status,
        "summary": summary,
        "ws": ws_print,
        "raw_parsed": parsed,
    }


def main() -> int:
    targets = sys.argv[1:] or [DEFAULT_HOST]
    require_both = os.environ.get("NODE4_PROBE_REQUIRE_BOTH") == "1"
    exit_code = 0
    for target in targets:
        res = probe_target(target)
        summary = res.get("summary") or {}
        if require_both:
            if not (
                summary.get("has_market_feed") is True
                and summary.get("has_strategy") is True
                and summary.get("keys") == EXPECTED_KEYS
                and res.get("ws", {}).get("summary", {}).get("has_market_feed") is True
                and res.get("ws", {}).get("summary", {}).get("has_strategy") is True
                and res.get("ws", {}).get("summary", {}).get("keys") == EXPECTED_KEYS
            ):
                exit_code = 1
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
