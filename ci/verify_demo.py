#!/usr/bin/env python3
"""Runtime proof for /strategy and /demo.

Order flow is gone from Node1, so the CI proof has to show that the built-in
strategy still produces a decision from price + volume profile + tick volume,
and that the demo engine books it on the contract the desk asked for:

  * XAUUSD, 1:1000 leverage, 0.10 spread,
  * 0.01 .. 1.00 lots in 0.01 steps,
  * margin = lots * 100 oz * entry / 1000,
  * equity = balance + floating PnL, free margin = equity - margin.

usage: verify_demo.py <strategy.json> <demo.json>
"""

import json
import sys

STRATEGY_PATH, DEMO_PATH = sys.argv[1], sys.argv[2]
ACTIONS = {"buy", "sell", "flat"}
EXIT_REASONS = {"signal_flip", "stop_loss", "take_profit", "stop_out"}


def fail(msg):
    print(f"DEMO CHECK FAILED: {msg}")
    sys.exit(1)


def load(path):
    try:
        with open(path) as fh:
            return json.load(fh)
    except Exception as exc:
        fail(f"{path}: {exc}")


strategy = load(STRATEGY_PATH)
demo = load(DEMO_PATH)

# --------------------------------------------------------------------------
# /strategy — the plan
# --------------------------------------------------------------------------
for key in ("symbol", "enabled", "params", "last", "history"):
    if key not in strategy:
        fail(f"/strategy is missing `{key}`")

if strategy["symbol"] != "XAUUSD":
    fail(f"/strategy symbol = {strategy['symbol']!r}, expected XAUUSD")

params = strategy["params"]
if params.get("min_lots") != 0.01 or params.get("max_lots") != 1.0:
    fail(f"strategy lot range is not 0.01-1.00: {params}")
if params.get("leverage") != 1000:
    fail(f"strategy sizes for leverage {params.get('leverage')}, expected 1000")
if not (0.0 < params.get("risk_pct", 0) <= 5.0):
    fail(f"risk_per_trade out of range: {params.get('risk_pct')}")

decisions = list(strategy.get("history") or [])
if strategy.get("last") and (not decisions or decisions[-1] != strategy["last"]):
    decisions.append(strategy["last"])
if not decisions:
    fail("no strategy evaluation has been recorded (the engine should evaluate at boot)")

timestamps = [d["ts"] for d in decisions]
if timestamps != sorted(timestamps):
    fail(f"strategy history is not in ascending time order: {timestamps[:5]}")

last = decisions[-1]
if last["action"] not in ACTIONS:
    fail(f"unknown action {last['action']!r}")
if not (-1.0 <= last["score"] <= 1.0):
    fail(f"score out of range: {last['score']}")
if not (0.0 <= last["confidence"] <= 1.0):
    fail(f"confidence out of range: {last['confidence']}")
if last["atr"] <= 0.0:
    fail(f"ATR should be positive once history is seeded: {last['atr']}")
if last["regime"] not in {"trend_up", "trend_down", "balance"}:
    fail(f"unexpected regime {last['regime']!r}")
if last["volatility"] not in {"expanding", "normal", "contracting"}:
    fail(f"unexpected volatility {last['volatility']!r}")

if last["action"] == "flat":
    if last["lots"] != 0.0:
        fail(f"a flat decision carries lots={last['lots']}")
else:
    lots = last["lots"]
    if not (0.01 <= lots <= 1.0):
        fail(f"lots {lots} outside 0.01-1.00")
    if abs(round(lots / 0.01) * 0.01 - lots) > 1e-6:
        fail(f"lots {lots} is not on the 0.01 grid")
    stop, target, price = last["stop_loss"], last["take_profit"], last["price"]
    if stop is None or target is None:
        fail("an active decision must carry a stop and a target")
    if last["action"] == "buy" and not (stop < price < target):
        fail(f"buy brackets are inverted: price={price} sl={stop} tp={target}")
    if last["action"] == "sell" and not (target < price < stop):
        fail(f"sell brackets are inverted: price={price} sl={stop} tp={target}")
    if last["risk_usd"] <= 0.0:
        fail(f"risk_usd should be positive: {last['risk_usd']}")
    if not last["reasons"]:
        fail("an active decision must explain itself in `reasons`")
    for factor in last["reasons"]:
        if not (-1.0 <= factor["value"] <= 1.0):
            fail(f"factor {factor['name']} out of range: {factor['value']}")
        if abs(factor["value"] * factor["weight"] - factor["contribution"]) > 1e-3:
            fail(f"factor {factor['name']} contribution is inconsistent")

print(
    f"/strategy: {len(decisions)} decision(s), last={last['action']} "
    f"score={last['score']:+.3f} conf={last['confidence']:.2f} "
    f"atr={last['atr']:.2f} lots={last['lots']:.2f} regime={last['regime']}"
)

# --------------------------------------------------------------------------
# /demo — the print
# --------------------------------------------------------------------------
for key in ("mode", "symbol", "leverage", "quote", "account", "params", "position",
            "stats", "recent_trades", "enabled"):
    if key not in demo:
        fail(f"/demo is missing `{key}`")

if demo["mode"] != "demo":
    fail(f"/demo mode = {demo['mode']!r}, expected 'demo'")
if demo["symbol"] != "XAUUSD":
    fail(f"/demo symbol = {demo['symbol']!r}")
if demo["leverage"] != 1000:
    fail(f"leverage = {demo['leverage']}, expected 1000")
if abs(demo["quote"]["spread"] - 0.10) > 1e-9:
    fail(f"spread = {demo['quote']['spread']}, expected 0.10")
if demo["params"]["min_lots"] != 0.01 or demo["params"]["max_lots"] != 1.0:
    fail(f"lot range is not 0.01-1.00: {demo['params']}")

quote = demo["quote"]
if not (quote["bid"] < quote["mid"] < quote["ask"]):
    fail(f"quote is not bid < mid < ask: {quote}")
if abs((quote["ask"] - quote["bid"]) - demo["quote"]["spread"]) > 0.02:
    fail(f"bid/ask do not reflect the configured spread: {quote}")

account = demo["account"]
if abs(account["free_margin"] - (account["equity"] - account["margin"])) > 0.02:
    fail(f"free margin != equity - margin: {account}")
if abs(account["equity"] - (account["balance"] + account["unrealized_pnl"])) > 0.02:
    fail(f"equity != balance + floating PnL: {account}")
if account["margin"] > 0:
    expected_level = account["equity"] / account["margin"] * 100.0
    # Margin is rounded to cents, and the level is equity/margin: at 0.11 lots
    # a 0.005 rounding step moves the level by a few points, so compare with a
    # relative tolerance rather than a hard 0.5.
    tolerance = max(1.0, expected_level * 0.01)
    if abs(account["margin_level"] - expected_level) > tolerance:
        fail(
            f"margin level is inconsistent: {account} "
            f"(expected {expected_level:.2f} +/- {tolerance:.2f})"
        )

position = demo["position"]
if position is not None:
    if position["side"] not in {"buy", "sell"}:
        fail(f"position side = {position['side']!r}")
    if not (0.01 <= position["lots"] <= 1.0):
        fail(f"position lots out of range: {position['lots']}")
    expected_margin = position["lots"] * 100.0 * position["entry"] / 1000.0
    if abs(position["margin"] - expected_margin) > 0.5:
        fail(f"position margin {position['margin']} != {expected_margin:.2f} at 1:1000")
    print(
        f"/demo position: {position['side']} {position['lots']:.2f} lots @ "
        f"{position['entry']:.2f} sl={position['stop_loss']} tp={position['take_profit']} "
        f"pnl={position['unrealized_pnl']:+.2f}"
    )
else:
    print("/demo position: flat")

trades = demo["recent_trades"]
stats = demo["stats"]
if stats["closed_trades"] < len(trades):
    fail(f"stats say {stats['closed_trades']} trades but {len(trades)} are listed")
if stats["wins"] + stats["losses"] != stats["closed_trades"]:
    fail(f"wins + losses != closed_trades: {stats}")
if len(trades) > 50:
    fail(f"recent_trades is capped at 50, got {len(trades)}")

for trade in trades:
    if trade["exit_reason"] not in EXIT_REASONS:
        fail(f"unknown exit reason {trade['exit_reason']!r}")
    sign = 1.0 if trade["side"] == "buy" else -1.0
    expected = (trade["exit"] - trade["entry"]) * sign * trade["lots"] * 100.0
    if abs(expected - trade["pnl"]) > 0.02:
        fail(f"trade {trade['id']} PnL {trade['pnl']} != {expected:.2f}")

# This service is not a broker: no credential may ride along in the payload.
blob = json.dumps(demo).lower()
for needle in ("api_key", "apikey", "secret", "password", "token"):
    if needle in blob:
        fail(f"a credential-shaped field appeared in /demo: {needle}")

print(
    f"/demo: balance={account['balance']:.2f} equity={account['equity']:.2f} "
    f"margin={account['margin']:.2f} free={account['free_margin']:.2f} "
    f"1:{demo['leverage']:.0f} spread={quote['spread']:.2f} "
    f"trades={stats['closed_trades']} net={stats['net_pnl']:+.2f}"
)
print("DEMO CHECK PASSED")
