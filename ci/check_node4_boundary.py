#!/usr/bin/env python3
"""Static guard for the Node 4 service boundary.

Node 4 is the **execution + risk management** service. It consumes exactly one
intent source (Node 3's private `/execution` socket) and owns venue policy,
sizing, the durable ledger, the kill switch and reconciliation. It must never
contain:

* market-data ingestion (no Node 1 feed consumer, no candle/levels subscriber),
* volume-profile maths or FRVP levels,
* a strategy / break-retest state machine,
* a second dashboard (Node 2 is a separate branch/service),
* any second intent source (the old `node1_strategy` mode).

This file is the denylist, so it is the one place where the forbidden
identifiers legitimately appear as text. It skips itself when scanning.

usage: check_node4_boundary.py [repo-root]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
SRC = ROOT / "node4-execution" / "src"

# (label, pattern, explanation). Patterns are matched against Rust source only.
FORBIDDEN: list[tuple[str, re.Pattern[str], str]] = [
    ("local strategy module", re.compile(r"^\s*mod\s+strategy\s*;", re.M),
     "src/main.rs still declares a strategy module"),
    ("strategy runner module", re.compile(r"^\s*mod\s+strategy_runner\s*;", re.M),
     "src/main.rs still declares a strategy runner"),
    ("Node 1 feed module", re.compile(r"^\s*mod\s+node1\s*;", re.M),
     "Node 4 must not ingest Node 1 market data directly"),
    ("intent source switch", re.compile(r"\bNODE4_INTENT_SOURCE\b"),
     "there must be exactly one intent source (Node 3)"),
    ("local strategy mode", re.compile(r"\bnode1_strategy\b"),
     "the local-strategy mode must not return"),
    ("Node 1 feed env var", re.compile(r"\bNODE1_(FEED_ENABLED|WS_URL|CONTRACT_VERSION|SYMBOL|INTERVAL)\b"),
     "Node 4 must not read a Node 1 feed configuration"),
    ("strategy approval gate", re.compile(r"\bNODE4_STRATEGY_APPROVED\b"),
     "strategy sign-off is a Node 3 concern"),
    ("strategy engine", re.compile(r"\bStrategyEngine\b"),
     "a strategy state machine has returned"),
    ("strategy decision", re.compile(r"\bStrategyDecision\b"),
     "strategy output must not be computed here"),
    ("strategy diagnostics block", re.compile(r"\bStrategySnapshot\b"),
     "/diagnostics must not carry strategy state"),
    ("feed diagnostics block", re.compile(r"\bFeedSnapshot\b"),
     "/diagnostics must not carry market-feed state"),
    ("feed handle", re.compile(r"\bMarketFeed\b"),
     "the Node 1 feed consumer has returned"),
    ("local intent handler", re.compile(r"\bhandle_local_intent\b"),
     "there must be no second intent path"),
    ("VP maths", re.compile(r"\bvolume_profile\b"),
     "volume-profile computation belongs to Node 3"),
    ("break state machine", re.compile(r"\bBreakState\b|\bPROXIMITY_DOLLARS\b|\bINVALIDATION_DOLLARS\b"),
     "break/retest detection belongs to Node 3"),
    ("attached feed", re.compile(r"\battach_market_feed\b|\battach_strategy\b"),
     "diagnostics must not attach market/strategy state"),
]

# Files that must not exist anywhere in this branch.
FORBIDDEN_PATHS = [
    "node4-execution/src/strategy.rs",
    "node4-execution/src/strategy_runner.rs",
    "node4-execution/src/node1.rs",
    "node2/serve.mjs",
    "node2/public/index.html",
    "docs/node4/LOCAL_STRATEGY.md",
    "Node-4.txt",
]

REQUIRED = [
    ("risk module declaration", "node4-execution/src/main.rs", re.compile(r"^\s*mod\s+risk\s*;", re.M)),
    ("risk pre-write gate", "node4-execution/src/node3.rs", re.compile(r"\.risk\s*\.\s*check\s*\(")),
    ("single Node 3 link", "node4-execution/src/main.rs", re.compile(r"node3::run\s*\(")),
]


def main() -> int:
    errors: list[str] = []

    if not SRC.is_dir():
        print(f"FAIL: {SRC} does not exist", file=sys.stderr)
        return 1

    rust_files = sorted(SRC.glob("*.rs"))
    for path in rust_files:
        text = path.read_text(encoding="utf-8")
        for label, pattern, explanation in FORBIDDEN:
            if pattern.search(text):
                errors.append(f"{path.relative_to(ROOT)}: {label} — {explanation}")

    for rel in FORBIDDEN_PATHS:
        if (ROOT / rel).exists():
            errors.append(f"{rel}: forbidden path still exists in the Node 4 branch")

    for label, rel, pattern in REQUIRED:
        path = ROOT / rel
        if not path.exists() or not pattern.search(path.read_text(encoding="utf-8")):
            errors.append(f"{rel}: missing {label}")

    if errors:
        print("Node 4 boundary check FAILED:")
        for error in errors:
            print(f"  - {error}")
        return 1

    print(f"Node 4 boundary check: OK ({len(rust_files)} Rust source files scanned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
