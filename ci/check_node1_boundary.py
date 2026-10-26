#!/usr/bin/env python3
"""Static guard for the C1 v2 Node 1 boundary.

Node 1 is a **read-only market-data and calendar feed**. It must not compute a
level, choose a side, size a trade, simulate one, or register a single write
route. Everything that used to do those things has moved:

    src/strategy.rs, src/demo_engine.rs        deleted (Node 1 has no strategy)
    src/volume_profile*.rs                     handoff/node3-vp/   (Node 3)
    src/econ_monitor.rs, src/ai_cache.rs       handoff/node3-ai/   (Node 3, optional)
    /levels /vp /ai /strategy /demo            no longer routed

This script fails when any of that comes back, when the published surface loses
a route or a topic the contract promises, or when the handoff packs drift from
their manifest.

Scanned surfaces: `src/`, `ci/`, `.github/`, `Cargo.toml`, `Dockerfile`
(executable and configuration files). Deliberately **not** scanned:

  * this file — it is the denylist, so it quotes every pattern;
  * `handoff/` — that is Node 3 source stored here until Agent 3 ports it;
  * `contracts/` — the signed contract names the forbidden vocabulary inside
    its `not` clause on purpose, and `ci/contract_sync.py` pins its bytes;
  * `ci/live/` — recorded output of whatever build is deployed, including the
    v0.4.0 fixtures Node 3 needs for parity;
  * `*.md` — prose has to be able to explain the removal. The acceptance
    criterion is `src/`, and `src/` is scanned in full.

usage: check_node1_boundary.py [repo-root]
"""

from __future__ import annotations

import hashlib
import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()

# ---------------------------------------------------------------------------
# Denylist: (token, pattern, message)
# ---------------------------------------------------------------------------

FORBIDDEN: list[tuple[str, re.Pattern[str], str]] = [
    (
        "retired module",
        re.compile(r"^\s*(?:pub\s+)?mod\s+(?:strategy|demo_engine|volume_profile|econ_monitor|ai_cache)\b", re.M),
        "a retired module is declared again",
    ),
    (
        "retired type",
        re.compile(r"\b(?:StrategyEngine|DemoEngine|VolumeProfile|ProfileMeta|VpCandle|VpWindow|vp_audit|nearest_high_impact_event)\b"),
        "a retired type or helper is referenced",
    ),
    (
        "sl-tp-rr setting",
        re.compile(r"\b(?:SL|TP|RR)_[A-Z][A-Z0-9_]*\b"),
        "a stop/target/risk-reward setting is back (Node 3 owns those)",
    ),
    (
        "intent type",
        re.compile(r"\b(?:TradeIntent|IntentSide|intent_id|trade_intent)\b"),
        "a trade-intent identifier is back",
    ),
    (
        "broker identifier",
        re.compile(r"\b(?:broker|venue|Deriv|MT5|MetaTrader|AggTrade|OrderFlowAnalyzer|ExecutionManager)\b"),
        "a broker/execution identifier is back",
    ),
    (
        "execution field",
        re.compile(r'"(?:side|stop_loss|take_profit|lots|risk_usd|action|leverage|position|equity)"\s*:'),
        "a published payload carries a trade field",
    ),
    (
        "execution crate",
        re.compile(r"node3-execution|mt5-bridge"),
        "an execution service is referenced",
    ),
    (
        "write route",
        re.compile(r"\.route\(\s*[^)]*,\s*(?:post|put|patch|delete)\("),
        "a write route is registered on the router",
    ),
    (
        "write method",
        re.compile(r"Method::(?:POST|PUT|PATCH|DELETE)"),
        "a write method is allowed through CORS",
    ),
    (
        "retired surface name",
        re.compile(r'"(?:levels|vp|strategy|demo|ai|volume_profile)"'),
        "a retired surface is named in the code",
    ),
    (
        "retired env var",
        re.compile(r"\b(?:STRATEGY|DEMO|ECON|VP)_[A-Z][A-Z0-9_]*\b"),
        "a retired environment variable is read",
    ),
]

# Sites where a finding is deliberate, keyed by the exact *token* allowed there.
# Every entry is an audit trail: negative tests that assert Node 1 refuses an
# execution shape have to spell that shape out.
ALLOWED: dict[str, set[str]] = {
    # REJECTED_KINDS: the list of inbound frame kinds that are dropped, and the
    # test that proves a status document carries none of those fields.
    "src/types.rs": {"intent type", "broker identifier"},
    # `no_route_accepts_a_broker_write` and `removed_routes_are_not_served`.
    "src/ws_server.rs": {"broker identifier", "retired surface name"},
    # The verifiers' own denylists and the "retired surfaces" sweep.
    "ci/c1_schema.py": {"intent type", "broker identifier"},
    "ci/verify_contract.py": {"retired surface name"},
    "ci/verify_ws.py": {
        "intent type",
        "broker identifier",
        "execution field",
        "retired surface name",
    },
    "ci/live_report.py": {"retired surface name", "retired type"},
    "ci/live-probe.sh": {"retired surface name"},
    "ci/smoke.sh": {"retired surface name", "retired type"},
}

CHECK_SUFFIXES = {".rs", ".toml", ".py", ".sh", ".yml", ".yaml"}
SKIP_DIRS = {".git", "target", "__pycache__", "node_modules", ".venv", "handoff", "contracts"}
SKIP_PREFIXES = {("ci", "live")}

# ---------------------------------------------------------------------------
# What the contract promises, pinned so a refactor cannot quietly drop it
# ---------------------------------------------------------------------------

EXPECTED_ROUTES = {"/health", "/status", "/candles", "/tick-volume", "/calendar", "/ws"}
EXPECTED_TOPICS = {"candle", "tick_volume", "calendar", "status"}
REJECTED_KINDS = [
    "trades", "trade_intent", "execution_hello", "execution_report", "order", "bubbles",
]
EXPECTED_MODULES = [
    "calendar", "candles", "config", "mcp_client", "publish", "session",
    "sifting_rest", "sifting_ws", "status", "synthetic", "tick_volume", "types",
    "ws_server",
]
RETIRED_MODULES = ["strategy", "demo_engine", "volume_profile", "econ_monitor", "ai_cache"]
RETIRED_FILES = [
    "src/strategy.rs",
    "src/demo_engine.rs",
    "src/volume_profile.rs",
    "src/volume_profile",
    "src/econ_monitor.rs",
    "src/ai_cache.rs",
    "FRONTEND_NEXT.md",
    "ci/verify_demo.py",
    "ci/verify_levels.py",
    "ci/verify_vp.py",
    "ci/verify_econ_audio.py",
    "ci/live/strategy.json",
    "ci/live/strategy.code",
    "ci/live/strategy.err",
    "ci/live/demo.json",
    "ci/live/demo.code",
    "ci/live/demo.err",
]
REQUIRED_FILES = [
    "contracts/MANIFEST.sha256",
    "contracts/NODE1_FEED_CONTRACT_v2.md",
    "contracts/c1-market-feed.schema.json",
    "ci/contract_sync.py",
    "ci/smoke.sh",
    "ci/verify_ws.py",
    "ci/verify_contract.py",
    "ci/verify_candles.py",
    "ci/verify_tick_volume.py",
    "ci/verify_calendar.py",
    "handoff/README.md",
    "handoff/MANIFEST.sha256",
    "handoff/node3-vp/volume_profile.rs",
    "handoff/node3-ai/econ_monitor.rs",
    "src/calendar.rs",
]
# The build must actually run the checks this guard belongs to.
REQUIRED_BUILD_STEPS = [
    "cargo fmt --all -- --check",
    "cargo check --all-targets",
    "cargo clippy --all-targets -- -D warnings",
    "cargo test --all-targets",
    "cargo build --release",
    "python3 ci/contract_sync.py",
    "python3 ci/check_node1_boundary.py",
    "bash ci/smoke.sh",
]


def read(rel: str) -> str:
    path = ROOT / rel
    if not path.is_file():
        return ""
    return path.read_text(encoding="utf-8", errors="replace")


def iter_files():
    for path in sorted(ROOT.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(ROOT)
        if any(part in SKIP_DIRS for part in rel.parts):
            continue
        if len(rel.parts) >= 2 and (rel.parts[0], rel.parts[1]) in SKIP_PREFIXES:
            continue
        if rel.as_posix() == "ci/check_node1_boundary.py":
            continue
        if rel.name != "Dockerfile" and rel.suffix not in CHECK_SUFFIXES:
            continue
        yield rel


def main() -> int:
    failures: list[str] = []
    files = list(iter_files())

    # ---- 1. denylist ------------------------------------------------------
    for rel in files:
        key = rel.as_posix()
        text = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        allowed = ALLOWED.get(key, set())
        for token, pattern, message in FORBIDDEN:
            if token in allowed:
                continue
            for match in pattern.finditer(text):
                line = text[: match.start()].count("\n") + 1
                snippet = text.splitlines()[line - 1].strip()[:120]
                failures.append(f"{key}:{line}: {message}\n      {snippet}")

    # ---- 2. the published surface is complete -----------------------------
    ws_server = read("src/ws_server.rs")
    routes = set(re.findall(r'\.route\(\s*"([^"]+)"', ws_server))
    if routes != EXPECTED_ROUTES:
        failures.append(
            f"src/ws_server.rs: routes are {sorted(routes)}, expected {sorted(EXPECTED_ROUTES)}"
        )
    for match in re.finditer(r'\.route\(\s*"([^"]+)"\s*,\s*([a-z_]+)\(', ws_server):
        if match.group(2) != "get":
            failures.append(
                f"src/ws_server.rs: {match.group(1)} is registered with "
                f"`{match.group(2)}`, not `get` — Node 1 has no write route"
            )
    if "fn no_route_accepts_a_broker_write" not in ws_server:
        failures.append(
            "src/ws_server.rs: the `no_route_accepts_a_broker_write` test is missing"
        )

    types = read("src/types.rs")
    if set(re.findall(r'Topic::\w+ => "([a-z_]+)"', types)) != EXPECTED_TOPICS:
        failures.append(f"src/types.rs: Topic::as_str does not map to {sorted(EXPECTED_TOPICS)}")
    for kind in REJECTED_KINDS:
        if f'"{kind}"' not in types:
            failures.append(f"src/types.rs: REJECTED_KINDS no longer covers {kind!r}")
    if 'CONTRACT_VERSION: &str = "2.0"' not in types:
        failures.append('src/types.rs: CONTRACT_VERSION is not "2.0"')

    # ---- 3. module wiring -------------------------------------------------
    main_rs = read("src/main.rs")
    declared = set(re.findall(r"^mod\s+(\w+)\s*;", main_rs, re.M))
    if declared != set(EXPECTED_MODULES):
        missing = sorted(set(EXPECTED_MODULES) - declared)
        extra = sorted(declared - set(EXPECTED_MODULES))
        failures.append(f"src/main.rs: modules differ from the feed (missing {missing}, extra {extra})")
    for module in RETIRED_MODULES:
        if (ROOT / "src" / f"{module}.rs").exists() or (ROOT / "src" / module).is_dir():
            failures.append(f"src/{module}: the retired module is back on Node 1")
    # Only `subscribe` and `heartbeat` are inbound frames (`type` names the
    # discriminant field of the outbound envelope and is not a frame kind).
    inbound = set(re.findall(r'#\[serde\(rename = "(\w+)"\)\]', types))
    if inbound - {"type", "subscribe", "heartbeat"}:
        failures.append(f"src/types.rs: unexpected inbound frame kind(s) {sorted(inbound)}")
    if not {"subscribe", "heartbeat"} <= inbound:
        failures.append("src/types.rs: the `subscribe`/`heartbeat` inbound frames are missing")

    # ---- 4. files that must and must not exist ----------------------------
    for rel in REQUIRED_FILES:
        if not (ROOT / rel).exists():
            failures.append(f"{rel}: required file is missing")
    for rel in RETIRED_FILES:
        if (ROOT / rel).exists():
            failures.append(f"{rel}: removed Node 1 asset is still present")
    if not read("src/calendar.rs").count("pub struct CalendarSource"):
        failures.append("src/calendar.rs: the ForexFactory/MCP calendar source is missing")

    # ---- 5. the handoff packs are pinned ----------------------------------
    manifest = ROOT / "handoff" / "MANIFEST.sha256"
    if manifest.is_file():
        for line in manifest.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            digest, _, name = line.partition("  ")
            path = ROOT / "handoff" / name.strip()
            if not path.is_file():
                failures.append(f"handoff/{name.strip()}: listed in the manifest but missing")
                continue
            actual = hashlib.sha256(path.read_bytes()).hexdigest()
            if actual != digest:
                failures.append(
                    f"handoff/{name.strip()}: manifest hash drifted "
                    f"({actual[:12]}… != {digest[:12]}…) — Node 3 ports these byte-for-byte"
                )

    # ---- 6. deployment ----------------------------------------------------
    dockerfile = read("Dockerfile")
    if "xauusd-engine" not in dockerfile:
        failures.append("Dockerfile: does not build xauusd-engine")
    for tool in ("ffmpeg", "yt-dlp"):
        if tool in dockerfile:
            failures.append(f"Dockerfile: still installs {tool} (the audio relay left Node 1)")

    build = read(".github/workflows/build.yml")
    for step in REQUIRED_BUILD_STEPS:
        if step not in build:
            failures.append(f".github/workflows/build.yml: missing step `{step}`")

    # ---- verdict ----------------------------------------------------------
    if failures:
        print(f"NODE1 BOUNDARY CHECK FAILED ({len(failures)} finding(s))")
        print()
        for finding in failures:
            print(" -", finding)
        return 1

    print(f"NODE1 BOUNDARY CHECK PASSED ({len(files)} files scanned)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
