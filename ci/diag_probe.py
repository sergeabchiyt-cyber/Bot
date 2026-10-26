#!/usr/bin/env python3
"""Temporary read-only live diagnostic probe."""

import json
import sys
import time
import urllib.error
import urllib.request

BASE = "https://engine-southeastasia-sng-main.onrender.com"


def http_get(url, headers=None, timeout=30):
    req = urllib.request.Request(url, headers=headers or {})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read()
            return resp.status, dict(resp.headers), body
    except urllib.error.HTTPError as err:
        body = err.read() if err.fp else b""
        return err.code, dict(err.headers) if err.headers else {}, body
    except Exception as exc:
        return -1, {}, str(exc).encode()


def main():
    lines = ["# Live Diagnostic Report", ""]

    # 1. Initial /status
    code, hdrs, body = http_get(f"{BASE}/status")
    lines.append(f"## 1. GET /status (t=0) -> {code} ({len(body)} bytes)")
    status_t0 = None
    try:
        status_t0 = json.loads(body)
        lines.append("```json")
        lines.append(json.dumps(status_t0, indent=2, sort_keys=True))
        lines.append("```")
    except Exception as exc:
        lines.append(f"Parse error: {exc} | raw: {body[:500]!r}")
    lines.append("")

    # 2. /calendar on Render
    code, _, body = http_get(f"{BASE}/calendar")
    lines.append(f"## 2. GET /calendar -> {code} ({len(body)} bytes)")
    try:
        cal = json.loads(body)
        lines.append(
            f"source={cal.get('source')!r} count={cal.get('count')} updated={cal.get('updated')}"
        )
        for ev in (cal.get("events") or [])[:3]:
            lines.append(f"  - {ev}")
    except Exception as exc:
        lines.append(f"Parse error: {exc} | raw: {body[:300]!r}")
    lines.append("")

    # 3. /candles for 1m, 15m, 1h, 60m + /tick-volume
    candles_by_iv = {}
    for iv in ("1m", "15m", "1h", "60m"):
        code, _, body = http_get(f"{BASE}/candles?interval={iv}", timeout=60)
        lines.append(f"## 3.{iv} GET /candles?interval={iv} -> {code} ({len(body)} bytes)")
        try:
            arr = json.loads(body)
            if isinstance(arr, list):
                candles_by_iv[iv] = arr
                sources = sorted({c.get("source") for c in arr if isinstance(c, dict)})
                lines.append(f"count={len(arr)} sources={sources}")
                if arr:
                    lines.append(f"first={arr[0]}")
                    if len(arr) > 1:
                        lines.append(f"second={arr[1]}")
                    if len(arr) > 2:
                        lines.append(f"penultimate={arr[-2]}")
                    lines.append(f"last={arr[-1]}")
            else:
                lines.append(f"non-list response: {arr}")
        except Exception as exc:
            lines.append(f"Parse error: {exc} | raw: {body[:300]!r}")
        lines.append("")

    code, _, body = http_get(f"{BASE}/tick-volume")
    lines.append(f"## 4. GET /tick-volume -> {code} ({len(body)} bytes)")
    tv_bars = []
    try:
        tv_bars = json.loads(body)
        lines.append(f"count={len(tv_bars)}")
        for b in tv_bars[-15:]:
            lines.append(f"  {b}")
    except Exception as exc:
        lines.append(f"Parse error: {exc} | raw: {body[:300]!r}")
    lines.append("")

    # Compare tick_volume.ticks vs 1m sum and 15m candle volume
    lines.append("## 5. tick_volume.ticks vs 1m sum vs 15m volume")
    m1 = candles_by_iv.get("1m", [])
    m15 = {c["time"]: c for c in candles_by_iv.get("15m", [])}
    for tv in tv_bars[-20:]:
        t = tv["time"]
        sub_1m = [c for c in m1 if t <= c["time"] < t + 900_000]
        sum_1m = sum(c["volume"] for c in sub_1m)
        c15 = m15.get(t)
        v15 = c15["volume"] if c15 else None
        lines.append(
            f"bucket={t} closed={tv['closed']} tv.ticks={tv['ticks']} "
            f"15m.vol={v15} 1m_bars={len(sub_1m)} 1m_sum={sum_1m}"
        )
    lines.append("")

    # 6. WebSocket 60s sample
    lines.append("## 6. WebSocket 60s live sample")
    try:
        import websocket

        ws = websocket.create_connection(
            BASE.replace("https://", "wss://") + "/ws", timeout=20
        )
        ws.send(
            json.dumps(
                {
                    "type": "subscribe",
                    "topics": ["candle", "tick_volume", "calendar", "status"],
                    "intervals": ["1m", "15m", "1h"],
                }
            )
        )
        t_end = time.time() + 60.0
        replay_counts = {}
        live_counts = {}
        tv_live = []
        same_ms_updates = 0
        prev_last_tick = None
        while time.time() < t_end:
            ws.settimeout(max(1.0, t_end - time.time()))
            try:
                raw = ws.recv()
            except Exception:
                break
            if not raw:
                continue
            fr = json.loads(raw)
            tp = fr.get("type", "?")
            if fr.get("replay") is True:
                key = tp
                if tp == "candle" and isinstance(fr.get("data"), dict):
                    key = f"candle:{fr['data'].get('interval')}"
                replay_counts[key] = replay_counts.get(key, 0) + 1
            else:
                key = tp
                if tp == "candle" and isinstance(fr.get("data"), dict):
                    key = f"candle:{fr['data'].get('interval')}"
                live_counts[key] = live_counts.get(key, 0) + 1
                if tp == "tick_volume" and isinstance(fr.get("data"), dict):
                    d = fr["data"]
                    lt = d.get("last_tick")
                    if prev_last_tick is not None and lt == prev_last_tick:
                        same_ms_updates += 1
                    prev_last_tick = lt
                    tv_live.append(
                        (
                            fr.get("seq"),
                            d.get("time"),
                            d.get("ticks"),
                            d.get("up_ticks"),
                            d.get("down_ticks"),
                            d.get("flat_ticks"),
                            d.get("close"),
                            lt,
                            d.get("closed"),
                        )
                    )
        ws.close()
        lines.append(f"replay_counts={replay_counts}")
        lines.append(f"live_counts={live_counts}")
        lines.append(f"tv_live_frames={len(tv_live)} same_ms_updates={same_ms_updates}")
        for row in tv_live[:15]:
            lines.append(f"  head: {row}")
        for row in tv_live[-15:]:
            lines.append(f"  tail: {row}")
    except Exception as exc:
        lines.append(f"WS sample error: {exc}")
    lines.append("")

    # 7. Second /status after 60s
    code, _, body = http_get(f"{BASE}/status")
    lines.append(f"## 7. GET /status (t=60) -> {code}")
    try:
        status_t60 = json.loads(body)
        c0 = ((status_t0 or {}).get("candles") or {}).get("counters") or {}
        c1 = (status_t60.get("candles") or {}).get("counters") or {}
        deltas = {k: c1.get(k, 0) - c0.get(k, 0) for k in sorted(set(c0) | set(c1))}
        lines.append(f"counters_t0={c0}")
        lines.append(f"counters_t60={c1}")
        lines.append(f"counters_delta_60s={deltas}")
        lines.append(f"feeds_t60={status_t60.get('feeds')}")
        lines.append(f"intervals_t60={(status_t60.get('candles') or {}).get('intervals')}")
    except Exception as exc:
        lines.append(f"Parse error: {exc}")
    lines.append("")

    # 8. Public SiftingIO docs & endpoints
    lines.append("## 8. SiftingIO public docs / endpoints")
    doc_urls = [
        "https://sifting.io/llms.txt",
        "https://sifting.io/llms-full.txt",
        "https://sifting.io/docs",
        "https://docs.sifting.io",
        "https://api.sifting.io/openapi.json",
        "https://api.sifting.io/docs",
        "https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?interval=1h&limit=1",
        "https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?interval=60m&limit=1",
    ]
    for u in doc_urls:
        c, h, b = http_get(u, headers={"User-Agent": "Mozilla/5.0", "Accept-Encoding": "identity"}, timeout=15)
        text = b.decode("utf-8", "replace")
        lines.append(f"### {u} -> {c} ({len(b)} bytes) ct={h.get('Content-Type') or h.get('content-type')}")
        lines.append("```")
        lines.append(text[:4000])
        lines.append("```")
    lines.append("")

    # 9. ForexFactory direct endpoints
    lines.append("## 9. ForexFactory direct endpoints")
    for u in (
        "https://nfs.faireconomy.media/ff_calendar_thisweek.json",
        "https://cdn-nfs.faireconomy.media/ff_calendar_thisweek.json",
    ):
        c, h, b = http_get(
            u,
            headers={"User-Agent": "Mozilla/5.0 (compatible; xauusd-engine/0.3)"},
            timeout=15,
        )
        lines.append(f"### {u} -> {c} ({len(b)} bytes)")
        lines.append(f"headers={h}")
        lines.append(f"body_head={b[:300]!r}")
    lines.append("")

    out_path = sys.argv[1] if len(sys.argv) > 1 else "ci/live/diag.md"
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote {out_path} ({len(lines)} lines)")


if __name__ == "__main__":
    main()
