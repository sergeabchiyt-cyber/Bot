#!/usr/bin/env python3
"""Cross-check the MT5 demo bridge protocol against its three implementations.

The bridge protocol exists in three places that drift independently:

  1. ``mt5-bridge/src/proto.rs``      — EA ⇄ bridge line protocol (Rust side)
  2. ``mt5-bridge/mql5/Mt5BridgeEA.mq5`` — EA ⇄ bridge line protocol (MQL5 side)
  3. ``mt5-bridge/src/snapshot.rs``   — bridge ⇄ Node 4 JSON contract
     ``node4-execution/src/types.rs`` — the execution-side mirror of that contract

A mismatch in any of them is a silent failure at runtime: a parameter the EA
never reads, a response field nobody sets, a frame Node 4 parses as
``Unknown``, or a snapshot field that is always ``null``. None of that is
visible to a compiler, so CI runs this script instead.

Exit code 0 = consistent, 1 = at least one error. Warnings never fail.

Usage:  python3 ci/mt5/protocol_lint.py [repo_root]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ERRORS: list[str] = []
WARNINGS: list[str] = []


def error(message: str) -> None:
    ERRORS.append(message)


def warn(message: str) -> None:
    WARNINGS.append(message)


def read(path: Path) -> str:
    if not path.exists():
        error(f"missing file: {path}")
        return ""
    return path.read_text(encoding="utf-8")


def strip_rust_tests(source: str) -> str:
    """Drop everything from the first ``#[cfg(test)]`` module onwards.

    Test fixtures describe fake payloads, not the wire contract, and they would
    otherwise contribute phantom field names.
    """
    marker = source.find("#[cfg(test)]")
    return source if marker < 0 else source[:marker]


# ---------------------------------------------------------------------------
# 1. EA method parity
# ---------------------------------------------------------------------------
def rust_ea_methods(proto_src: str) -> set[str]:
    module = re.search(r"pub mod method \{(?P<body>.*?)\n\}", proto_src, re.S)
    if not module:
        error("proto.rs: could not find `pub mod method`")
        return set()
    return set(re.findall(r'pub const [A-Z_]+: &str = "([A-Z_]+)";', module.group("body")))


def ea_methods(ea_src: str) -> set[str]:
    return set(re.findall(r'method\s*==\s*"([A-Z_]+)"', ea_src))


# ---------------------------------------------------------------------------
# 2. EA request parameters (bridge → EA)
# ---------------------------------------------------------------------------
def rust_request_params(terminal_src: str) -> set[str]:
    """Keys the Rust side sends in ``REQ`` lines.

    Only ``vec![...]`` blocks and ``params.push((...))`` calls carry request
    parameters; response field reads live in other call shapes on purpose.
    """
    keys: set[str] = set()
    for start in (m.end() - 1 for m in re.finditer(r"vec!\[", terminal_src)):
        depth = 0
        index = start
        while index < len(terminal_src):
            char = terminal_src[index]
            if char == "[":
                depth += 1
            elif char == "]":
                depth -= 1
                if depth == 0:
                    break
            index += 1
        block = terminal_src[start:index]
        keys.update(re.findall(r'\(\s*"([a-z_0-9]+)"\s*,', block))
    keys.update(re.findall(r'params\.push\(\(\s*"([a-z_0-9]+)"', terminal_src))
    return keys


def ea_read_params(ea_src: str) -> set[str]:
    return set(re.findall(r'Param\(keys, values, count,\s*"([a-z_0-9]+)"', ea_src))


# ---------------------------------------------------------------------------
# 3. EA response fields (EA → bridge)
# ---------------------------------------------------------------------------
def rust_response_reads(rust_srcs: dict[str, str]) -> set[str]:
    keys: set[str] = set()
    patterns = [
        r'(?:require_f64|require_i64|require_u64|opt_f64|opt_i64|opt_u64|get_str|get_f64|get_i64|get_u64|get_bool|get)\(\s*"([a-z_0-9]+)"',
        r'item_(?:i64|f64|str|bool)\(\s*\w+\s*,\s*"([a-z_0-9]+)"',
        r'\.get\(\s*"([a-z_0-9]+)"\s*\)',
    ]
    for name, src in rust_srcs.items():
        for pattern in patterns:
            for key in re.findall(pattern, src):
                keys.add(key)
    return keys


def ea_response_fields(ea_src: str) -> set[str]:
    # `key=value` inside the EA's reply builders, either as a literal or as a
    # StringFormat placeholder followed by `=`.
    literal = set(re.findall(r'([a-z_][a-z_0-9]*)=', ea_src))
    return literal


# ---------------------------------------------------------------------------
# 4. Bridge ⇄ Node 4 struct field parity
# ---------------------------------------------------------------------------
def struct_fields(source: str, struct_name: str) -> list[str] | None:
    match = re.search(
        rf"pub struct {struct_name} \{{(?P<body>.*?)\n\}}", source, re.S
    )
    if not match:
        return None
    fields: list[str] = []
    for line in match.group("body").splitlines():
        line = line.strip()
        if not line or line.startswith("//") or line.startswith("#["):
            continue
        field = re.match(r"pub ([a-z_0-9]+)\s*:", line)
        if field:
            fields.append(field.group(1))
    return fields


def enum_frame_names(source: str, enum_name: str) -> set[str]:
    """Frame names of a serde-tagged enum, honouring explicit renames.

    ``#[serde(other)]`` variants (the catch-all) never appear on the wire, so
    they are skipped along with any other attribute-only lines.
    """
    match = re.search(rf"pub enum {enum_name} \{{(?P<body>.*?)\n\}}", source, re.S)
    if not match:
        error(f"could not find `pub enum {enum_name}`")
        return set()
    names: set[str] = set()
    pending_rename: str | None = None
    pending_other = False
    for line in match.group("body").splitlines():
        stripped = line.strip()
        rename = re.search(r'#\[serde\(rename = "([a-z_0-9]+)"\)\]', stripped)
        if rename:
            pending_rename = rename.group(1)
            continue
        if stripped.startswith("#["):
            if "serde(other)" in stripped:
                pending_other = True
            continue
        variant = re.match(r"([A-Z][A-Za-z0-9]*)\s*(?:\{|\(|,)", stripped)
        if variant:
            if pending_rename:
                names.add(pending_rename)
            elif not pending_other:
                names.add(camel_to_snake(variant.group(1)))
            pending_rename = None
            pending_other = False
    return names


def camel_to_snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def enum_variant_names(source: str, enum_name: str) -> set[str]:
    """CamelCase variant names of an enum, excluding the catch-all."""
    match = re.search(rf"pub enum {enum_name} \{{(?P<body>.*?)\n\}}", source, re.S)
    if not match:
        error(f"could not find `pub enum {enum_name}`")
        return set()
    names: set[str] = set()
    pending_other = False
    for line in match.group("body").splitlines():
        stripped = line.strip()
        if stripped.startswith("#["):
            if "serde(other)" in stripped:
                pending_other = True
            continue
        variant = re.match(r"([A-Z][A-Za-z0-9]*)\s*(?:\{|\()", stripped)
        if variant:
            if not pending_other:
                names.add(variant.group(1))
            pending_other = False
    return names


# ---------------------------------------------------------------------------
# 5. Node 4 REST/WS key expectations
# ---------------------------------------------------------------------------
def ws_frame_renames(source: str) -> set[str]:
    return set(re.findall(r'#\[serde\(rename = "([a-z_0-9]+)"\)\]', source))


def documented_bridge_events(bridge_dir, docs_dir) -> int:
    """Every `bridge_event` name the frontend doc promises must actually exist.

    The bridge writes these names as string literals in node4.rs; if one is
    renamed, the dashboard would silently never see that event again. The doc is
    the only place they are written down outside the Rust source.
    """
    node4_src = read(Path(bridge_dir) / "src/node4.rs")
    emitted = set(re.findall(r'"(\w+)"\.into\(\)', node4_src))
    emitted |= set(re.findall(r'\n\s*"([a-z_]+)",\n', node4_src))

    doc = read(Path(docs_dir) / "mt5" / "FRONTEND_RESOURCES.md")
    paragraph = ""
    for chunk in doc.split("\n\n"):
        if "bridge_event" in chunk and "names" in chunk:
            paragraph = chunk
            break
    if not paragraph:
        error("FRONTEND_RESOURCES.md no longer describes the bridge_event names")
        return 0

    documented = set(re.findall(r"`([a-z_]+)`", paragraph))
    skip = {"bridge_event", "mt5_positions", "mt5_history", "reason", "auto"}
    checked = 0
    for name in sorted(documented - skip):
        checked += 1
        if name not in emitted:
            error(
                f"FRONTEND_RESOURCES.md documents bridge_event '{name}' but "
                "mt5-bridge/src/node4.rs never emits it"
            )
    return checked


def main() -> int:
    root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()
    bridge = root / "mt5-bridge"
    node4 = root / "node4-execution"
    if not bridge.exists() or not node4.exists():
        print(f"error: expected {bridge} and {node4} to exist", file=sys.stderr)
        return 1

    proto_src = read(bridge / "src/proto.rs")
    snapshot_src = read(bridge / "src/snapshot.rs")
    terminal_src = strip_rust_tests(read(bridge / "src/terminal.rs"))
    bridge_src = strip_rust_tests(read(bridge / "src/bridge.rs"))
    ea_src = read(bridge / "mql5/Mt5BridgeEA.mq5")
    types_src = read(node4 / "src/types.rs")

    # --- 1. methods -------------------------------------------------------
    rust_methods = rust_ea_methods(proto_src)
    mql_methods = ea_methods(ea_src)
    for missing in sorted(rust_methods - mql_methods):
        error(
            f"method {missing} is sent by the bridge (proto.rs) but never handled "
            f"in Mt5BridgeEA.mq5 — the EA would answer 'unknown method'"
        )
    for extra in sorted(mql_methods - rust_methods):
        warn(f"Mt5BridgeEA.mq5 handles {extra}, which proto.rs never sends")

    # --- 2. request parameters -------------------------------------------
    sent = rust_request_params(terminal_src)
    if not sent:
        error("could not extract any request parameter from mt5-bridge/src/terminal.rs")
    read_params = ea_read_params(ea_src)
    for key in sorted(sent - read_params):
        error(
            f"request parameter '{key}' is sent in terminal.rs but the EA never reads it "
            f"(Param(keys, values, count, \"{key}\"))"
        )
    for key in sorted(read_params - sent):
        error(
            f"the EA reads request parameter '{key}' that terminal.rs never sends"
        )

    # --- 3. response fields ----------------------------------------------
    read_fields = rust_response_reads(
        {"terminal.rs": terminal_src, "bridge.rs": bridge_src}
    )
    produced = ea_response_fields(ea_src)
    for key in sorted(read_fields - produced):
        error(
            f"response field '{key}' is read by the bridge but the EA never emits it "
            f"({key}=...) — the value would always be None"
        )

    # --- 4. shared snapshot structs --------------------------------------
    snapshot_structs = [
        "Mt5AccountSnapshot",
        "Mt5Position",
        "Mt5PositionsSnapshot",
        "Mt5Deal",
        "Mt5HistorySnapshot",
        "Mt5BridgeStatus",
        "BridgeErrorPayload",
    ]
    for name in snapshot_structs:
        bridge_fields = struct_fields(snapshot_src, name)
        node_fields = struct_fields(types_src, name)
        if bridge_fields is None:
            error(f"snapshot.rs is missing struct {name}")
            continue
        if node_fields is None:
            error(f"node4-execution/src/types.rs is missing struct {name}")
            continue
        for field in sorted(set(bridge_fields) - set(node_fields)):
            error(f"{name}.{field} exists in snapshot.rs but not in node4 types.rs")
        for field in sorted(set(node_fields) - set(bridge_fields)):
            error(f"{name}.{field} exists in node4 types.rs but not in snapshot.rs")

    # Node 4's MT5 order outcome is the bridge's OrderOutcome under a different
    # name (the bridge does not know about TradeEvent).
    bridge_outcome = struct_fields(snapshot_src, "OrderOutcome")
    node_outcome = struct_fields(types_src, "Mt5OrderOutcome")
    if bridge_outcome is None or node_outcome is None:
        error("could not compare OrderOutcome with Mt5OrderOutcome")
    else:
        for field in sorted(set(bridge_outcome) - set(node_outcome)):
            error(f"OrderOutcome.{field} is not mirrored by Mt5OrderOutcome")
        for field in sorted(set(node_outcome) - set(bridge_outcome)):
            error(f"Mt5OrderOutcome.{field} has no counterpart in OrderOutcome")

    # --- 5. frames --------------------------------------------------------
    bridge_frames = enum_frame_names(snapshot_src, "BridgeToNode4") | enum_frame_names(
        snapshot_src, "Node4ToBridge"
    )
    node_frames = ws_frame_renames(types_src)
    for frame in sorted(bridge_frames - node_frames):
        error(f"frame '{frame}' is in the bridge contract but not a WsFrame variant in Node 4")
    for frame in sorted(node_frames - bridge_frames):
        # Frontend-only resources (levels, candle, trades, ...) are Node 4's own
        # vocabulary and are not expected on the bridge link.
        if frame.startswith(("bridge_", "mt5_")):
            warn(
                f"WsFrame variant '{frame}' looks bridge-related but has no counterpart "
                f"in the bridge contract"
            )

    # --- 6. every Node 4 command is actually dispatched -------------------
    node4_src = strip_rust_tests(read(bridge / "src/node4.rs"))
    commands = enum_variant_names(snapshot_src, "Node4ToBridge")
    for command in sorted(commands):
        if f"Node4ToBridge::{command}" not in node4_src:
            error(
                f"Node 4 can send {camel_to_snake(command)} but mt5-bridge/src/node4.rs "
                f"never matches Node4ToBridge::{command}"
            )

    # --- 7. documented bridge_event names --------------------------------
    events_checked = documented_bridge_events(bridge, root / "docs")

    # --- report -----------------------------------------------------------
    for message in WARNINGS:
        print(f"warning: {message}")
    for message in ERRORS:
        print(f"error: {message}")
    if ERRORS:
        print(f"\nprotocol lint FAILED with {len(ERRORS)} error(s)")
        return 1
    print(
        "protocol lint ok: "
        f"{len(rust_methods)} EA methods, {len(sent)} request params, "
        f"{len(read_fields)} response fields, {len(bridge_frames)} frames, "
        f"{events_checked} bridge events checked"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
