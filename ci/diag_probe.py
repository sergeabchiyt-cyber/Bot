#!/usr/bin/env python3
"""Fetch SiftingIO commodities historical + pagination + websocket docs."""

import html
import re
import sys
import urllib.error
import urllib.request


def http_get(url, accept="text/markdown, text/plain, text/html", timeout=20):
    req = urllib.request.Request(
        url,
        headers={
            "User-Agent": "Mozilla/5.0",
            "Accept": accept,
            "Accept-Encoding": "identity",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, resp.headers.get("Content-Type", ""), resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as err:
        body = err.read().decode("utf-8", "replace") if err.fp else ""
        return err.code, "", body
    except Exception as exc:
        return -1, "", str(exc)


def strip_html(raw):
    raw = re.sub(r"<script\b[^>]*>.*?</script>", " ", raw, flags=re.S | re.I)
    raw = re.sub(r"<style\b[^>]*>.*?</style>", " ", raw, flags=re.S | re.I)
    raw = re.sub(r"<[^>]+>", "\n", raw)
    raw = html.unescape(raw)
    return "\n".join(line.strip() for line in raw.splitlines() if line.strip())


def main():
    urls = [
        "https://sifting.io/docs/historical/commodities.md",
        "https://sifting.io/docs/historical/commodities",
        "https://sifting.io/blog/commodities-market-data-api-backfill-history-stream-live-prices.md",
        "https://sifting.io/blog/commodities-market-data-api-backfill-history-stream-live-prices",
        "https://sifting.io/blog/cursor-pagination-market-data-api-years-of-1-minute-ohlcv-bars.md",
        "https://sifting.io/blog/cursor-pagination-market-data-api-years-of-1-minute-ohlcv-bars",
        "https://sifting.io/blog/commodities-data-api-coverage-symbols-gold-silver-oil",
        "https://sifting.io/blog/streaming-gold-silver-oil-prices-websocket-commodities-api",
    ]
    lines = ["# SiftingIO Commodities & Pagination Docs", ""]
    for u in urls:
        code, ct, body = http_get(u)
        lines.append(f"## {u} -> {code} ({len(body)} chars, ct={ct})")
        if code == 200:
            if "html" in ct.lower():
                cleaned = strip_html(body)
                lines.append(cleaned[:18000])
            else:
                lines.append(body[:18000])
        else:
            lines.append(body[:400])
        lines.append("")

    out_path = sys.argv[1] if len(sys.argv) > 1 else "ci/live/diag.md"
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote {out_path} ({len(lines)} lines)")


if __name__ == "__main__":
    main()
