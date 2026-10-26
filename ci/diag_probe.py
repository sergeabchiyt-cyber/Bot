#!/usr/bin/env python3
"""Fetch SiftingIO public OpenAPI / AsyncAPI / docs sections for historical bars and WS ticks."""

import re
import sys
import urllib.error
import urllib.request


def http_get(url, timeout=20):
    req = urllib.request.Request(
        url, headers={"User-Agent": "Mozilla/5.0", "Accept-Encoding": "identity"}
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as err:
        body = err.read().decode("utf-8", "replace") if err.fp else ""
        return err.code, body
    except Exception as exc:
        return -1, str(exc)


def main():
    lines = ["# SiftingIO Spec Extract", ""]

    # 1. Full llms-full.txt (58 KB)
    code, text = http_get("https://sifting.io/llms-full.txt")
    lines.append(f"## 1. llms-full.txt ({code}, {len(text)} chars)")
    lines.append(text)
    lines.append("")

    # 2. pricing.md
    code, text = http_get("https://sifting.io/pricing.md")
    lines.append(f"## 2. pricing.md ({code}, {len(text)} chars)")
    lines.append(text[:15000])
    lines.append("")

    # 3. openapi.yaml — extract commodities/bars + components/parameters/schemas + pagination/errors
    code, openapi = http_get("https://sifting.io/openapi.yaml")
    lines.append(f"## 3. openapi.yaml ({code}, {len(openapi)} chars)")
    if len(openapi) <= 120000:
        lines.append(openapi)
    else:
        for m in re.finditer(r"(?im)^.*(?:commodit|bars|cursor|interval|hist).*$", openapi):
            start = max(0, m.start() - 400)
            end = min(len(openapi), m.end() + 1200)
            lines.append("--- match ---")
            lines.append(openapi[start:end])
    lines.append("")

    # 4. asyncapi.yaml
    code, asyncapi = http_get("https://sifting.io/asyncapi.yaml")
    lines.append(f"## 4. asyncapi.yaml ({code}, {len(asyncapi)} chars)")
    lines.append(asyncapi[:50000])
    lines.append("")

    out_path = sys.argv[1] if len(sys.argv) > 1 else "ci/live/diag.md"
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote {out_path} ({len(lines)} lines)")


if __name__ == "__main__":
    main()
