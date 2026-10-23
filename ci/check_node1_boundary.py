#!/usr/bin/env python3
"""Static guard for the Node1 service boundary.

Node1 is the market-data / volume-profile service plus the built-in strategy
evaluator and the **demo (paper) simulator**. It is still not an execution
venue: no broker credential, no MT5 bridge, no node3-execution, no write route
and no way to reach a broker from the strategy or demo modules.

This script fails when a broker/execution shape reappears, when the removed
order-flow path comes back (exchange aggTrade adapters + the bubbles analyzer),
or when one of the demo contract's fixed invariants (1:1000 leverage, 0.10
spread, 0.01-1.00 lots) drifts.

Note: this file is the *denylist*, so it is the one place in the branch where
the forbidden identifiers legitimately appear as text. It is skipped when
scanning itself. Everywhere else they are banned.

usage: check_node1_boundary.py [repo-root]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()

# Patterns that must never appear in Node1 *executable* surface.
FORBIDDEN: list[tuple[str, re.Pattern[str], str]] = [
    (
        "execution module declaration",
        re.compile(r"^\s*mod\s+execution", re.M),
        "src/main.rs still declares an execution module",
    ),
    (
        "ExecutionManager",
        re.compile(r"\bExecutionManager\b"),
        "execution manager still referenced",
    ),
    (
        "execution venue type",
        re.compile(r"\bExecutionVenue\b"),
        "execution venue type still referenced",
    ),
    (
        "execution files",
        re.compile(r"src/execution(_deriv|_chelsea)?\.rs"),
        "execution source file still referenced",
    ),
    (
        "node3-execution service",
        re.compile(r"node3-execution"),
        "execution service still referenced",
    ),
    (
        "mt5 bridge",
        re.compile(r"mt5-bridge"),
        "MT5 bridge still referenced",
    ),
    (
        "broker credential env var",
        re.compile(r"\b(DERIV_DEMO_API|DERIV_APP_ID|DERIV_API_URL|MCP_CHELSEA_URL|EXECUTION_VENUE)\b"),
        "broker/execution env var still read",
    ),
    (
        "account stake / lot config",
        re.compile(r"\b(ORDER_SIZE|DERIV_MIN_STAKE|MT5_VOLUME_LOTS|STAKE_|LOT_?SIZE)\b"),
        "stake/lot-size setting still present (the demo uses DEMO_MIN_LOTS/DEMO_MAX_LOTS)",
    ),
    (
        "execution SL/TP/RR config",
        re.compile(r"\b(SL_MIN_PIPS|SL_MAX_PIPS|TP_MIN_PIPS|TP_MAX_PIPS|RR_MIN|RR_MAX)\b"),
        "execution-only SL/TP/RR field still present",
    ),
    (
        "execution venue in a payload",
        re.compile(r'"venue"\s*:'),
        "a recorded payload still carries an execution venue",
    ),
    (
        "trades WS topic",
        re.compile(r'"trades"'),
        "the bidirectional `trades` topic is still in use",
    ),
    (
        "broker write route",
        re.compile(r"\.route\([^)]*,\s*(post|put|delete|patch)\("),
        "a write route is registered on the router",
    ),
    # ---- Order flow was removed from Node1; it must not come back ----
    (
        "order-flow analyzer",
        re.compile(r"\bOrderFlowAnalyzer\b"),
        "the removed order-flow analyzer is still referenced",
    ),
    (
        "exchange aggTrade adapter",
        re.compile(r"\bAggTrade\b"),
        "the removed exchange aggTrade adapter is still referenced",
    ),
    (
        "order-flow delta history",
        re.compile(r"\bdelta_history\b"),
        "order-flow delta state is still referenced",
    ),
    (
        "bubbles WS frame",
        re.compile(r'#\[serde\(rename = "bubbles"\)\]'),
        "the removed order-flow `bubbles` frame is back in the wire contract",
    ),
    (
        "order-flow feed switch",
        re.compile(r"\bFEED_(BINANCE|BYBIT|OKX|BITGET|GATE|KRAKEN|ALLTICK|ITICK)\b"),
        "an exchange order-flow feed switch is still read",
    ),
]

# Paths whose *content* is checked (source, CI, deployment, docs).
CHECK_SUFFIXES = {
    ".rs", ".toml", ".yml", ".yaml", ".py", ".sh", ".md", ".json",
    ".jsonl", ".code", ".err", "",  # "" = extensionless (Dockerfile)
}

# Generated CI artefacts: not hand-maintained, so they are not part of the
# contract this guard enforces.
GENERATED = {
    Path("ci") / "PROOF.md",
    Path("ci") / "LIVE.md",
    Path("ci") / "check_node1_boundary.py",  # this file quotes every pattern
}

# Directories of *recorded* output from whatever build is currently deployed
# (`ci/live-probe.sh` snapshots the live service). They describe the
# deployment, not this branch, so a stale field from a build that predates
# this cleanup must not fail CI — redeploying refreshes them.
RECORDED_DIRS = {"live"}

# Sites where a forbidden pattern is intentional, mapped to the finding names
# allowed there. Every entry needs a reason: this is the audit trail for
# "the remaining matches are deliberate".
ALLOWED_SITES: dict[str, set[str]] = {
    # Negative tests: each one asserts that Node1 *rejects* an execution frame
    # or that the removed order-flow topic no longer parses.
    "src/types.rs": {"trades WS topic"},
    "src/ws_server.rs": {"trades WS topic"},
    "ci/verify_ws.py": {"trades WS topic"},
}

# Directories never walked.
SKIP_DIRS = {".git", "target", "__pycache__", "node_modules", ".venv"}

MARKET_TOPICS_REQUIRED = [
    "levels", "candle", "tick_volume", "calendar", "status", "strategy", "demo",
]
MARKET_ROUTES_REQUIRED = [
    "/health", "/status", "/levels", "/vp", "/candles", "/tick-volume",
    "/calendar", "/ai", "/strategy", "/demo", "/ws",
]

# The demo contract the desk asked for, pinned so a refactor cannot silently
# turn the simulator into something else.
DEMO_CONTRACT = [
    (r'env_f64\("DEMO_LEVERAGE", 1000\.0\)', "1:1000 leverage default"),
    (r'env_f64\("DEMO_SPREAD", 0\.10\)', "0.10 spread default"),
    (r'env_f64\("DEMO_MIN_LOTS", 0\.01\)', "0.01 minimum lot"),
    (r'env_f64\("DEMO_MAX_LOTS", 1\.0\)', "1.00 maximum lot"),
    (r'env_f64\("DEMO_START_BALANCE", 10_000\.0\)', "10,000 starting balance"),
]

# Modules that must stay offline: a simulator that could open a socket is one
# refactor away from being an execution path.
OFFLINE_MODULES = ["src/strategy.rs", "src/demo_engine.rs"]
NETWORK_PATTERN = re.compile(r"\breqwest\b|connect_async|\bTcpStream\b|https?://|wss?://")


def iter_files():
    for path in sorted(ROOT.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(ROOT)
        if any(part in SKIP_DIRS for part in rel.parts):
            continue
        if rel in GENERATED:
            continue
        # ci/live/** is recorded output from the deployed build.
        if len(rel.parts) > 2 and rel.parts[0] == "ci" and rel.parts[1] in RECORDED_DIRS:
            continue
        if rel.suffix not in CHECK_SUFFIXES:
            continue
        yield rel


def main() -> int:
    failures: list[str] = []
    files = list(iter_files())

    for rel in files:
        try:
            text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        except OSError as exc:  # pragma: no cover - unreadable file
            failures.append(f"{rel}: unreadable ({exc})")
            continue

        allowed = ALLOWED_SITES.get(rel.as_posix(), set())
        for name, pattern, message in FORBIDDEN:
            if name in allowed:
                continue
            for match in pattern.finditer(text):
                line = text[: match.start()].count("\n") + 1
                snippet = text.splitlines()[line - 1].strip()[:120]
                failures.append(f"{rel}:{line}: {message}\n      {snippet}")

    # Positive checks: the market + strategy surface must survive the cleanup.
    def has(path: str, pattern: str) -> bool:
        p = ROOT / path
        return p.is_file() and re.search(pattern, p.read_text(encoding="utf-8", errors="replace"))

    for route in MARKET_ROUTES_REQUIRED:
        if not has("src/ws_server.rs", re.escape(f'"{route}"')):
            failures.append(f"src/ws_server.rs: market route {route} is missing")

    for topic in MARKET_TOPICS_REQUIRED:
        if not has("src/ws_server.rs", re.escape(f'"{topic}"')):
            failures.append(f"src/ws_server.rs: market WS topic {topic} is missing")

    if not has("src/strategy.rs", r"pub struct StrategyEngine"):
        failures.append("src/strategy.rs: the strategy evaluator is missing")

    if not has("src/demo_engine.rs", r"pub struct DemoEngine"):
        failures.append("src/demo_engine.rs: the demo engine is missing")

    # Nothing may sit between the strategy/demo modules and a network socket.
    for module in OFFLINE_MODULES:
        p = ROOT / module
        if not p.is_file():
            failures.append(f"{module}: missing")
            continue
        text = p.read_text(encoding="utf-8", errors="replace")
        if NETWORK_PATTERN.search(text):
            failures.append(f"{module}: the strategy/demo path must stay offline")

    # The 1:1000 / 0.10 / 0.01-1.00 demo contract.
    config_text = ""
    if (ROOT / "src/config.rs").is_file():
        config_text = (ROOT / "src/config.rs").read_text(encoding="utf-8", errors="replace")
    for pattern, label in DEMO_CONTRACT:
        if not re.search(pattern, config_text):
            failures.append(f"src/config.rs: demo contract drifted — missing {label}")

    # Deployment builds only the engine crate.
    dockerfile = ROOT / "Dockerfile"
    if dockerfile.is_file():
        text = dockerfile.read_text(encoding="utf-8")
        if "xauusd-engine" not in text:
            failures.append("Dockerfile: does not build xauusd-engine")
        if re.search(r"COPY\s+(node3-execution|mt5-bridge)", text):
            failures.append("Dockerfile: copies an execution/MT5 crate")

    # No execution subtree survived on the branch, and the order-flow files
    # that were removed must not come back.
    for gone in ["node3-execution", "mt5-bridge", "docs/mt5", "ci/mt5", "ci/deriv",
                 "ci/deriv_probe.py", "src/execution.rs", "src/execution_deriv.rs",
                 "src/execution_chelsea.rs", ".github/workflows/deriv-probe.yml",
                 "src/order_flow.rs", "src/binance_ws.rs", "src/multi_exchange.rs"]:
        if (ROOT / gone).exists():
            failures.append(f"{gone}: removed Node1 asset is still present")

    if failures:
        print(f"NODE1 BOUNDARY CHECK FAILED ({len(failures)} finding(s))")
        print()
        for f in failures:
            print(" -", f)
        return 1

    print(f"NODE1 BOUNDARY CHECK PASSED ({len(files)} files scanned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
