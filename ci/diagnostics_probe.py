#!/usr/bin/env python3
"""Live + local HTTP and WebSocket diagnostics probe for Node 4."""

import asyncio
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone

try:
    import websockets
except ImportError:
    websockets = None


TARGETS = [
    ("Node 4 (Render default)", "https://execution-southeastasia-sng-main.onrender.com"),
    ("Node 3 (Render default)", "https://strategy-southeastasia-sng-main.onrender.com"),
]

DASHBOARD_CONFIG_URLS = [
    "https://static-dash-frontend.onrender.com/js/config.js",
]


def jq_summary(doc: dict) -> dict:
    work = doc.get("work") or {}
    service = work.get("service") or ""
    version = work.get("version") or ""
    return {
        "build": f"{service} {version}".strip(),
        "role": work.get("role"),
        "has_market_feed": "market_feed" in doc,
        "has_strategy": "strategy" in doc,
        "keys": sorted(doc.keys()),
    }


def probe_http(base_url: str) -> dict:
    url = f"{base_url.rstrip('/')}/diagnostics"
    req = urllib.request.Request(url, headers={"Accept": "application/json", "User-Agent": "node4-diag-probe/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=25) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
            headers = dict(resp.headers.items())
            doc = json.loads(raw)
            return {
                "ok": True,
                "status": resp.status,
                "url": url,
                "headers": {
                    k: v
                    for k, v in headers.items()
                    if k.lower() in ("content-type", "server", "x-render-origin-server", "cf-ray", "date")
                },
                "raw_len": len(raw),
                "summary": jq_summary(doc),
                "work": doc.get("work"),
                "node3": doc.get("node3"),
                "market_feed": doc.get("market_feed"),
                "strategy": doc.get("strategy"),
            }
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")[:500]
        return {"ok": False, "url": url, "status": e.code, "error": f"HTTPError {e.code}", "body": body}
    except Exception as e:
        return {"ok": False, "url": url, "error": f"{type(e).__name__}: {e}"}


async def probe_ws(base_url: str) -> dict:
    if websockets is None:
        return {"ok": False, "error": "websockets library not installed"}
    ws_url = base_url.rstrip("/").replace("https://", "wss://").replace("http://", "ws://") + "/ws"
    frames = []
    try:
        async with websockets.connect(ws_url, open_timeout=20, close_timeout=5, max_size=4 * 1024 * 1024) as ws:
            sub = json.dumps({"type": "subscribe", "topics": ["diagnostics"]})
            await ws.send(sub)
            deadline = asyncio.get_event_loop().time() + 10.0
            while asyncio.get_event_loop().time() < deadline and len(frames) < 10:
                timeout = max(0.5, deadline - asyncio.get_event_loop().time())
                try:
                    msg = await asyncio.wait_for(ws.recv(), timeout=timeout)
                except asyncio.TimeoutError:
                    break
                try:
                    parsed = json.loads(msg)
                except Exception:
                    parsed = {"raw": str(msg)[:200]}
                ftype = parsed.get("type") or parsed.get("topic")
                entry = {"type": ftype, "top_keys": sorted(parsed.keys())}
                data = parsed.get("data") if isinstance(parsed.get("data"), dict) else parsed.get("diagnostics")
                if ftype == "diagnostics" and isinstance(data, dict):
                    entry["summary"] = jq_summary(data)
                    entry["work"] = data.get("work")
                    entry["has_market_feed"] = "market_feed" in data
                    entry["has_strategy"] = "strategy" in data
                    frames.append(entry)
                    break
                frames.append(entry)
        return {"ok": True, "ws_url": ws_url, "frames": frames}
    except Exception as e:
        return {"ok": False, "ws_url": ws_url, "error": f"{type(e).__name__}: {e}", "frames": frames}


async def probe_local_binary(bin_path: str) -> dict:
    results = {}

    # 1. Verify fail-closed startup when legacy env (no NODE3_WS_URL / NODE4_SHARED_TOKEN) is used
    with tempfile.TemporaryDirectory() as tmpdir:
        env_unconfigured = {
            "PATH": os.environ.get("PATH", ""),
            "PORT": "19090",
            "EXECUTION_VENUE": "none",
            "EXECUTION_LEDGER_FILE": os.path.join(tmpdir, "ledger.jsonl"),
        }
        proc = subprocess.Popen(
            [bin_path],
            env=env_unconfigured,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        try:
            http_res = {"ok": False}
            for _ in range(30):
                await asyncio.sleep(0.1)
                http_res = probe_http("http://127.0.0.1:19090")
                if http_res.get("ok"):
                    break
            ws_res = await probe_ws("http://127.0.0.1:19090")
            results["unconfigured_fail_closed_mode"] = {"http": http_res, "ws": ws_res}
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except Exception:
                proc.kill()

    # 2. Verify local binary in node1_strategy mode (dry-run)
    with tempfile.TemporaryDirectory() as tmpdir:
        env_local = {
            "PATH": os.environ.get("PATH", ""),
            "PORT": "19091",
            "EXECUTION_VENUE": "none",
            "NODE4_INTENT_SOURCE": "node1_strategy",
            "NODE1_FEED_ENABLED": "1",
            "NODE4_STRATEGY_APPROVED": "1",
            "NODE1_WS_URL": "ws://127.0.0.1:19999/ws",
            "EXECUTION_LEDGER_FILE": os.path.join(tmpdir, "ledger.jsonl"),
        }
        proc = subprocess.Popen([bin_path], env=env_local, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            http_res = {"ok": False}
            for _ in range(30):
                await asyncio.sleep(0.1)
                http_res = probe_http("http://127.0.0.1:19091")
                if http_res.get("ok"):
                    break
            ws_res = await probe_ws("http://127.0.0.1:19091")
            results["node1_strategy_mode"] = {"http": http_res, "ws": ws_res}
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except Exception:
                proc.kill()

    # 3. Verify local binary in node3 mode
    with tempfile.TemporaryDirectory() as tmpdir:
        env_node3 = {
            "PATH": os.environ.get("PATH", ""),
            "PORT": "19092",
            "EXECUTION_VENUE": "none",
            "NODE4_INTENT_SOURCE": "node3",
            "NODE3_WS_URL": "ws://127.0.0.1:19998/execution",
            "NODE4_SHARED_TOKEN": "probe-token",
            "EXECUTION_LEDGER_FILE": os.path.join(tmpdir, "ledger.jsonl"),
        }
        proc = subprocess.Popen([bin_path], env=env_node3, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        try:
            http_res = {"ok": False}
            for _ in range(30):
                await asyncio.sleep(0.1)
                http_res = probe_http("http://127.0.0.1:19092")
                if http_res.get("ok"):
                    break
            ws_res = await probe_ws("http://127.0.0.1:19092")
            results["node3_mode"] = {"http": http_res, "ws": ws_res}
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except Exception:
                proc.kill()

    return results


async def main() -> int:
    report = {
        "probed_at_utc": datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC"),
        "targets": {},
        "dashboard_configs": {},
        "local_binary": {},
    }

    for label, base_url in TARGETS:
        http_res = probe_http(base_url)
        ws_res = await probe_ws(base_url)
        report["targets"][label] = {
            "base_url": base_url,
            "http_diagnostics": http_res,
            "ws_diagnostics": ws_res,
        }

    for url in DASHBOARD_CONFIG_URLS:
        req = urllib.request.Request(url, headers={"User-Agent": "node4-diag-probe/1.0"})
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                report["dashboard_configs"][url] = {
                    "status": resp.status,
                    "body": resp.read().decode("utf-8", errors="replace"),
                }
        except Exception as e:
            report["dashboard_configs"][url] = {"error": f"{type(e).__name__}: {e}"}

    bin_path = os.environ.get("NODE4_BIN_PATH", "node4-execution/target/debug/xauusd-node4-execution")
    if os.path.exists(bin_path):
        report["local_binary"] = await probe_local_binary(bin_path)

    out_json = json.dumps(report, indent=2)
    print(out_json)

    md_lines = [
        "# Node 4 `/diagnostics` Live & Local Probe Report",
        "",
        f"Recorded at: `{report['probed_at_utc']}`",
        "",
        "## 1. Deployed Render Services",
        "",
    ]
    for label, data in report["targets"].items():
        md_lines.append(f"### {label} (`{data['base_url']}`)")
        md_lines.append("")
        md_lines.append("#### HTTP `GET /diagnostics`")
        md_lines.append("```json")
        md_lines.append(json.dumps(data["http_diagnostics"], indent=2))
        md_lines.append("```")
        md_lines.append("")
        md_lines.append("#### WebSocket `/ws` (`{\"type\":\"subscribe\",\"topics\":[\"diagnostics\"]}`)")
        md_lines.append("```json")
        md_lines.append(json.dumps(data["ws_diagnostics"], indent=2))
        md_lines.append("```")
        md_lines.append("")

    if report["local_binary"]:
        md_lines.append("## 2. Current Branch `node4-execution` Binary Verification")
        md_lines.append("")
        md_lines.append("```json")
        md_lines.append(json.dumps(report["local_binary"], indent=2))
        md_lines.append("```")
        md_lines.append("")

    md_lines.append("## 3. Deployed Dashboard `config.js`")
    md_lines.append("```json")
    md_lines.append(json.dumps(report["dashboard_configs"], indent=2))
    md_lines.append("```")
    md_lines.append("")

    with open("ci/DIAGNOSTICS-PROBE.md", "w", encoding="utf-8") as f:
        f.write("\n".join(md_lines))

    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
