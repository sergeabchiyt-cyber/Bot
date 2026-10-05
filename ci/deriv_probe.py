#!/usr/bin/env python3
"""Probe Deriv's Options API for the contract shapes Node 3 can actually trade.

This exists because of a production failure:

    ERROR xauusd_node3_execution: Trade execution failed: Deriv proposal error:
    {"code":"ContractBuyValidationError","message":"Invalid barrier.","subcode":"InvalidBarrier"}

Node 3 used to derive a barrier from ``|tp - sl| / 2`` and send it with every
``CALL``/``PUT`` proposal. On ``frxXAUUSD`` an intraday ``CALL``/``PUT`` is an
at-the-money *Rise/Fall* contract, so Derev rejects the proposal as soon as a
non-zero barrier is attached.

The probe records what Deriv really offers, straight from Deriv:

* ``contracts_for frxXAUUSD`` - contract types, expiry type, number of barriers
  and the duration window of each entry (this is what a server can sell now);
* the raw ``proposal`` response for the at-the-money shape Node 3 sends after
  the fix (no ``barrier`` field at all);
* the raw ``proposal`` response for the shape that failed in production
  (``barrier: "+2.500"``), so the ``InvalidBarrier`` regression stays visible.

It talks to two endpoints:

* the public WebSocket of the *current* API
  (``wss://api.derivws.com/trading/v1/options/ws/public``) - the same wire
  schema as the OTP sockets Node 3 trades on, no token required;
* the legacy ``/websockets/v3`` public endpoint with Deriv's official test
  ``app_id`` (1089), which is the failover flow Node 3 keeps for ``a1-...``
  tokens.

Runs on a GitHub runner (see ``.github/workflows/deriv-probe.yml``) because the
development sandbox cannot reach Deriv. ``--dry-run`` prints the exact payloads
without opening a socket, and ``DERIV_PROBE_LEGACY_URL`` /
``DERIV_PROBE_PUBLIC_URL`` override the endpoints (the local mock test uses
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

# Mirrors node3-execution/src/execution_deriv.rs.
SYMBOL = "frxXAUUSD"
CURRENCY = "USD"
STAKE = 0.35  # Deriv's minimum stake for this account type.
DURATION = 5
DURATION_UNIT = "m"

LEGACY_DEFAULT = "wss://ws.derivws.com/websockets/v3?app_id=1089"
PUBLIC_DEFAULT = "wss://api.derivws.com/trading/v1/options/ws/public"
LEGACY_FAILOVER = [
    "wss://ws.derivws.com/websockets/v3?app_id=1089",
    "wss://ws.binaryws.com/websockets/v3?app_id=1089",
]

TIMEOUT = 25.0

# (contract_type, barrier, label) - the first entry is the shape Node 3 sends
# after the fix, the rest are the shapes that were rejected in production.
SHAPES = [
    ("CALL", None, "ATM rise, no barrier (Node 3 fix shape)"),
    ("PUT", None, "ATM fall, no barrier (Node 3 fix shape)"),
    ("CALL", "+2.500", "CALL + relative barrier (production failure shape)"),
    ("PUT", "-2.500", "PUT + relative barrier (production failure shape)"),
]


def contracts_for_payload(new_api: bool) -> dict:
    """``contracts_for`` request for the endpoint flavour Node 3 would use."""
    payload: dict = {"contracts_for": SYMBOL}
    if not new_api:
        # The current API dropped currency / product_type (contract pricing is
        # bound to the authenticated account); the legacy API still accepts them.
        payload["currency"] = CURRENCY
        payload["product_type"] = "basic"
    return payload


def proposal_payload(new_api: bool, contract_type: str, barrier: str | None) -> dict:
    """``proposal`` request exactly as Node 3 builds it."""
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
            "      - {ct:<7} expiry={ex:<9} barriers={b:<2} duration={mn}..{mx} market={mkt}".format(
                ct=item.get("contract_type", "?"),
                ex=item.get("expiry_type", "?"),
                b=item.get("barriers", "?"),
                mn=item.get("min_contract_duration", "?"),
                mx=item.get("max_contract_duration", "?"),
                mkt=item.get("market", "?"),
            )
        )
    if not available:
        print(f"      (raw response: {json.dumps(data)[:400]})")


def print_proposal(label: str, data: dict) -> bool:
    """Print one proposal result; True when Deriv priced the contract."""
    err = data.get("error")
    if err:
        subcode = err.get("subcode")
        suffix = f" (subcode {subcode})" if subcode else ""
        print(f"    [{label}] REJECTED {err.get('code')}: {err.get('message')}{suffix}")
        return False
    proposal = data.get("proposal") or {}
    print(
        "    [{label}] OK id={id} ask_price={ask} payout={payout} spot={spot}".format(
            label=label,
            id=proposal.get("id"),
            ask=proposal.get("ask_price"),
            payout=proposal.get("payout"),
            spot=proposal.get("spot"),
        )
    )
    longcode = proposal.get("longcode")
    if longcode:
        print(f"        {longcode}")
    return True


async def probe(label: str, url: str, new_api: bool) -> str:
    """Probe one endpoint. Returns 'ok', 'unreachable' or 'partial'."""
    print(f"\n=== {label}\n    url: {url}")
    try:
        async with websockets.connect(url, open_timeout=TIMEOUT) as ws:
            print_contracts_for(await request(ws, contracts_for_payload(new_api), "contracts_for"))
            priced = rejected = 0
            for contract_type, barrier, note in SHAPES:
                payload = proposal_payload(new_api, contract_type, barrier)
                response = await request(ws, payload, "proposal")
                if print_proposal(note, response):
                    priced += 1
                else:
                    rejected += 1
            print(f"    summary: {priced} priced, {rejected} rejected")
            return "ok"
    except Exception as exc:  # noqa: BLE001 - the probe reports, it does not raise
        print(f"    UNREACHABLE: {type(exc).__name__}: {exc}")
        return "unreachable"


async def main() -> int:
    if "--dry-run" in sys.argv:
        print("contracts_for (legacy):", json.dumps(contracts_for_payload(False)))
        print("contracts_for (new):   ", json.dumps(contracts_for_payload(True)))
        for contract_type, barrier, note in SHAPES:
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
