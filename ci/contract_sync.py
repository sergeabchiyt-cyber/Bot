#!/usr/bin/env python3
"""Keep the shared `contracts/` folder byte-identical on all four branches.

D2 keeps `Node1`, `Node2`, `Node3` and `Node-4` as disjoint-history branches.
A contract change must therefore be applied to all four branches in the same
wave. This script verifies a checked-in sha256 manifest so CI on *each* branch
fails when its copy drifts.

usage:
    python3 ci/contract_sync.py            # verify (CI mode)
    python3 ci/contract_sync.py --write    # regenerate the manifest
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONTRACTS = ROOT / "contracts"
MANIFEST = CONTRACTS / "MANIFEST.sha256"
SKIP = {"MANIFEST.sha256"}


def digest_map() -> dict[str, str]:
    out: dict[str, str] = {}
    for path in sorted(CONTRACTS.iterdir()):
        if not path.is_file() or path.name in SKIP:
            continue
        out[path.name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


def render(items: dict[str, str]) -> str:
    return "".join(f"{digest}  {name}\n" for name, digest in sorted(items.items()))


def main() -> int:
    if not CONTRACTS.is_dir():
        print("FAIL: contracts/ is missing", file=sys.stderr)
        return 1

    current = digest_map()
    if not current:
        print("FAIL: contracts/ contains no contract files", file=sys.stderr)
        return 1

    if "--write" in sys.argv:
        MANIFEST.write_text(render(current), encoding="utf-8")
        print(f"wrote {MANIFEST.relative_to(ROOT)} ({len(current)} files)")
        return 0

    if not MANIFEST.exists():
        print("FAIL: contracts/MANIFEST.sha256 is missing", file=sys.stderr)
        return 1

    recorded: dict[str, str] = {}
    for line in MANIFEST.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        digest, name = line.split(None, 1)
        recorded[name.strip()] = digest.strip()

    expected, actual = render(current), render(recorded)
    if expected != actual:
        print("FAIL: contracts/ does not match MANIFEST.sha256", file=sys.stderr)
        print("  contract files changed without regenerating the manifest,", file=sys.stderr)
        print("  or the manifest was copied from another branch.", file=sys.stderr)
        print("  Run: python3 ci/contract_sync.py --write", file=sys.stderr)
        return 1

    print(f"contract sync: OK ({len(current)} files, identical manifest)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
