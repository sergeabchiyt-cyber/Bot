#!/usr/bin/env python3
"""Probe Deriv's Options API for the contract shapes Node 4 can actually trade.

This exists because of a production failure:

    ERROR xauusd_node4_execution: Trade execution failed: Deriv proposal error:
    {"code":"ContractBuyValidationError","message":"Invalid barrier.","subcode":"InvalidBarrier"}

Node 4 used to derive a ``barrier`` from ``|tp - sl| / 2`` and send it with every
proposal. The probe records, straight from Deriv, everything needed to send a
shape Deriv accepts:

* ``contracts_for frxXAUUSD`` - contract types, expiry type, number of barriers
  and the duration window of each entry (what Deriv is selling right now), plus
  the raw JSON of the intraday ``CALL``/``PUT`` entries;
* a barrier sweep of ``proposal`` requests at the contract's minimum stake, so
  the accepted barrier range (and the exact rejection for the production
  ``+2.500``) is visible instead of guessed.

It talks to two endpoints:

* the public WebSocket of the *current* API
  (``wss://api.derivws.com/trading/v1/options/ws/public``) - the same wire
  schema as the OTP sockets Node 4 trades on, no token required;
* the legacy ``/websockets/v3`` public endpoint with Deriv's official test
  ``app_id`` (1089), which is the failover flow Node 4 keeps for ``a1-...``
  tokens (Cloudflare answers 520 for it from many cloud networks - the probe
  records that rather than hiding it).

Runs on a GitHub runner (see ``.github/workflows/deriv-probe.yml``) because the
development sandbox cannot reach Deriv; the report lands in
``ci/deriv/DERIV.md``. ``--dry-run`` prints the exact payloads without opening a
socket, and ``DERIV_PROBE_LEGACY_URL`` / ``DERIV_PROBE_PUBLIC_URL`` /
``DERIV_PROBE_STAKE`` override the endpoints and stake (the local mock test uses
that).
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import time

try:
    import websockets
except ImportError:  # pragma: no cover - the workflow installs it
    print("This probe needs the websockets package: python3 -m pip install websockets")
    raise SystemExit(2)

# Mirrors node4-execution/src/execution_deriv.rs.
SYMBOL = "frxXAUUSD"
CURRENCY = "USD"
DURATION = 5
DURATION_UNIT = "m"
# Deriv's own minimum-stake message for this contract is
# `Please enter a stake amount that's at least 0.50.` (subcode InvalidMinStake).
STAKE = float(os.environ.get("DERIV_PROBE_STAKE", "0.50"))

LEGACY_DEFAULT = "wss://ws.derivws.com/websockets/v3?app_id=1089"
PUBLIC_DEFAULT = "wss://api.derivws.com/trading/v1/options/ws/public"
LEGACY_FAILOVER = [
    "wss://ws.derivws.com/websockets/v3?app_id=1089",
    "wss://ws.binaryws.com/websockets/v3?app_id=1089",
]

TIMEOUT = 25.0

# (contract_type, barrier, label)
SWEEP = [
    ("CALL", None, "CALL, no barrier"),
    ("CALL", "+0.01", "CALL barrier +0.01"),
    ("CALL", "+0.05", "CALL barrier +0.05"),
    ("CALL", "+0.10", "CALL barrier +0.10"),
    ("CALL", "+0.50", "CALL barrier +0.50"),
    ("CALL", "+1.00", "CALL barrier +1.00"),
    ("CALL", "+2.50", "CALL barrier +2.50 (production shape)"),
    ("CALL", "+6.00", "CALL barrier +6.00 (TP distance on gold)"),
    ("CALL", "+15.00", "CALL barrier +15.00"),
    ("CALL", "-2.50", "CALL barrier -2.50 (wrong sign)"),
    ("PUT", None, "PUT, no barrier"),
    ("PUT", "-0.01", "PUT barrier -0.01"),
    ("PUT", "-0.10", "PUT barrier -0.10"),
    ("PUT", "-0.50", "PUT barrier -0.50"),
    ("PUT", "-2.50", "PUT barrier -2.50 (production shape)"),
    ("PUT", "-6.00", "PUT barrier -6.00 (TP distance on gold)"),
]


def contracts_for_payload(new_api: bool) -> dict:
    """``contracts_for`` request for the endpoint flavour Node 4 would use."""
    payload: dict = {"contracts_for": SYMBOL}
    if not new_api:
        # The current API dropped currency / product_type (pricing follows the
        # authenticated account); the legacy API still accepts them.
        payload["currency"] = CURRENCY
        payload["product_type"] = "basic"
    return payload


def proposal_payload(new_api: bool, contract_type: str, barrier: str | None) -> dict:
    """``proposal`` request exactly as Node 4 builds it."""
    payload = {
        "proposal": 1,
        "amount": STAKE,
        "basis": "stake",
        "contract_type": contract_type,
        "currency": CURRENCY,
        "duration": DURATION,
        "duration_unit": DURATION_UNIT,
    }
    payload["underlying_symbol" if new_api else "symbol"] = SYMBOL
    if barrier is not None:
        payload["barrier"] = barrier
    return payload


async def request(ws, payload: dict, msg_type: str, timeout: float = TIMEOUT) -> dict:
    """Send one request and return the first response that answers it."""
    await ws.send(json.dumps(payload))
    deadline = time.monotonic() + timeout
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(f"no {msg_type} response within {timeout:.0f}s")
        raw = await asyncio.wait_for(ws.recv(), timeout=remaining)
        try:
            data = json.loads(raw)
        except (TypeError, ValueError):
            continue
        # Accept the matching msg_type, or an error frame for anything (Deriv
        # answers validation failures with the request's own msg_type).
        if data.get("msg_type") == msg_type or "error" in data:
            return data


def print_contracts_for(data: dict) -> None:
    if "error" in data:
        err = data["error"]
        print(f"    contracts_for ERROR {err.get('code')}: {err.get('message')}")
        return
    contracts_for = data.get("contracts_for") or {}
    available = contracts_for.get("available") or []
    print(f"    contracts_for: {len(available)} entries")
    for item in available:
        print(
            "      - {ct:<11} expiry={ex:<9} barriers={b:<2} duration={mn}..{mx} market={mkt}".format(
                ct=item.get("contract_type", "?"),
                ex=item.get("expiry_type", "?"),
                b=item.get("barriers", "?"),
                mn=item.get("min_contract_duration", "?"),
                mx=item.get("max_contract_duration", "?"),
                mkt=item.get("market", "?"),
            )
        )

    intraday = [
        item
        for item in available
        if item.get("contract_type") in ("CALL", "PUT", "HIGHER", "LOWER")
        and str(item.get("expiry_type", "")).startswith("intraday")
    ]
    if intraday:
        print("    raw entries Node 4 can trade intraday (all fields Deriv returns):")
        for item in intraday:
            print(f"      {json.dumps(item, sort_keys=True)}")
    if not available:
        print(f"    (raw response: {json.dumps(data)[:400]})")


def print_proposal(label: str, data: dict) -> bool:
    """Print one proposal result; True when Deriv priced the contract."""
    err = data.get("error")
    if err:
        subcode = err.get("subcode")
        suffix = f" (subcode {subcode})" if subcode else ""
        print(f"    [{label:<42}] REJECTED {err.get('code')}: {err.get('message')}{suffix}")
        return False
    proposal = data.get("proposal") or {}
    print(
        "    [{label:<42}] OK id={id} ask_price={ask} payout={payout} spot={spot} barrier={barrier}".format(
            label=label,
            id=proposal.get("id"),
            ask=proposal.get("ask_price"),
            payout=proposal.get("payout"),
            spot=proposal.get("spot"),
            barrier=proposal.get("barrier", "-"),
        )
    )
    longcode = proposal.get("longcode")
    if longcode:
        print(f"        {longcode}")
    return True


def print_sweep_summary(results: list[tuple[str, str, bool, str]]) -> None:
    """results: (contract_type, barrier, priced, outcome)."""
    for contract_type in ("CALL", "PUT"):
        rows = [r for r in results if r[0] == contract_type]
        if not rows:
            continue
        accepted = [r[1] for r in rows if r[2]]
        print(
            "    sweep {ct}: accepted barriers = {ok}; rejected = {bad}".format(
                ct=contract_type,
                ok=", ".join(accepted) if accepted else "none",
                bad=", ".join(r[1] for r in rows if not r[2]) or "none",
            )
        )


async def probe(label: str, url: str, new_api: bool) -> str:
    """Probe one endpoint. Returns 'ok' or 'unreachable'."""
    print(f"\n=== {label}\n    url: {url}")
    try:
        async with websockets.connect(url, open_timeout=TIMEOUT) as ws:
            print_contracts_for(await request(ws, contracts_for_payload(new_api), "contracts_for"))
            print(f"    proposal sweep at stake {STAKE} {CURRENCY}, {DURATION}{DURATION_UNIT}:")
            results: list[tuple[str, str, bool, str]] = []
            for contract_type, barrier, note in SWEEP:
                response = await request(ws, proposal_payload(new_api, contract_type, barrier), "proposal")
                priced = print_proposal(note, response)
                outcome = "" if priced else (response.get("error") or {}).get("message", "")
                results.append((contract_type, barrier or "none", priced, outcome))
            print_sweep_summary(results)
            return "ok"
    except Exception as exc:  # noqa: BLE001 - the probe reports, it does not raise
        print(f"    UNREACHABLE: {type(exc).__name__}: {exc}")
        return "unreachable"


async def main() -> int:
    if "--dry-run" in sys.argv:
        print("contracts_for (legacy):", json.dumps(contracts_for_payload(False)))
        print("contracts_for (new):   ", json.dumps(contracts_for_payload(True)))
        for contract_type, barrier, note in SWEEP:
            print(f"proposal ({note}):")
            print("  legacy:", json.dumps(proposal_payload(False, contract_type, barrier)))
            print("  new:   ", json.dumps(proposal_payload(True, contract_type, barrier)))
        return 0

    legacy = os.environ.get("DERIV_PROBE_LEGACY_URL", LEGACY_DEFAULT)
    public = os.environ.get("DERIV_PROBE_PUBLIC_URL", PUBLIC_DEFAULT)

    results = [
        await probe("current API - public WebSocket (same schema as the OTP sockets)", public, True),
        await probe("legacy /websockets/v3 (a1-... token failover flow)", legacy, False),
    ]

    # The legacy endpoint is Cloudflare-fronted and answers 520 from some
    # networks; retry the alternates before calling it unreachable.
    if results[1] == "unreachable" and not os.environ.get("DERIV_PROBE_LEGACY_URL"):
        for url in LEGACY_FAILOVER[1:]:
            results[1] = await probe("legacy /websockets/v3 (failover host)", url, False)
            if results[1] != "unreachable":
                break

    print("\n=== result")
    print(f"    current API public socket: {results[0]}")
    print(f"    legacy socket:             {results[1]}")
    if all(result == "unreachable" for result in results):
        print("    both endpoints unreachable - check network access, not the payloads")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
