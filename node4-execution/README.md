# Node 4 — Execution Service (XAUUSD)

A Rust service that consumes **authenticated trade intents from Node 3**,
applies risk management and venue policy, places **at most one broker order per
`intent_id`**, and reports the broker-authoritative outcome.

It owns every execution concern:

1. the outbound intent link to Node 3 (`wss://<node3-host>/execution`,
   protocol v1 / intent schema v1.1 additive),
2. **risk management** (`src/risk.rs`): ATR price bands, RR band, per-trade
   risk cap, lot cap, max open positions, daily loss limit, max drawdown and a
   cooldown after consecutive losses,
3. venue policy (exactly one of `deriv_mt5_demo`, `deriv_demo`,
   `chelsea_live`, `none` — no fallback, ever),
4. stake/lot sizing, symbol mapping and demo safeguards,
5. the durable append-only idempotency ledger (`intent_id` is the system-wide
   key, fsynced before any broker write),
6. the Deriv options adapter, the Chelsea MCP adapter, the Deriv MT5 **demo**
   execution adapter and its `mt5-bridge`,
7. broker/account/position/history state, reconciliation of unclear writes,
8. the operator kill switch and the public read-only diagnostics surface.

Node 4 owns **no strategy, no market data and no volume-profile maths**. Those
live in Nodes 3 and 1. There is exactly one intent source (Node 3); the old
opt-in `node1_strategy` mode, the Node 1 feed consumer and the embedded
dashboard have been deleted. See [`../README.md`](../README.md),
[`../PLAN.md`](../PLAN.md) and [`../contracts/README.md`](../contracts/README.md).

```
+-----------------------------+         execution_hello / trade_intent
|  Node 3  (strategy engine)  | <--------------------------------------+
|  intent: side, SL, TP, RR   |                                      |
+-----------------------------+         execution_report (+ ack)      |
                                                                     |
+-----------------------------------------------------------------+  |
|  Node 4  (node4-execution)                                      |  |
|                                                                 |--+
|  1. validate intent (schema, id, symbol, side, prices, expiry)  |
|  2. RISK CHECK (bands, RR, caps, exposure, cooldown)            |
|  3. duplicate? -> replay the recorded result, never a 2nd order  |
|  4. persist intent_id durably, report `accepted`                 |
|  5. exactly one venue, pre-write health checks                   |
|  6. one order -> filled / partial / rejected / unknown           |
|  7. unknown -> reconcile by intent_id, never resend              |
+-----------------------------------------------------------------+
        |                    |                       |
        | Deriv options      | Chelsea MCP           | WS /mt5/bridge (outbound bridge)
        v                    v                       v
   Deriv demo API      Chelsea live MCP        mt5-bridge -> EA -> MT5 demo terminal
```

## Intent contract (protocol version 1, C3 v1.1)

Node 4 sends the first frame and accepts no intent before Node 3 acknowledges:

```json
{"type":"execution_hello","token":"<NODE4_SHARED_TOKEN>","service":"xauusd-node4-execution","protocol_version":1}
{"type":"execution_hello_ack","accepted":true,"protocol_version":1}
```

`trade_intent.data` carries `schema_version`, `intent_id`, `strategy`,
`symbol`, `side`, `order_type`, `reference_price`, `stop_loss`,
`take_profit`, `risk_reward`, `level_name`, `source_candle_time`,
`created_at`, `expires_at` — and **no** venue, account, stake, lots or
credential. C3 v1.1 may add optional flat fields (`atr`, `atr_pips`,
`level_id`, `drawing_id`, `strategy_version`, `volume_confirmed`); serde
ignores unknown fields, so a v1 consumer accepts them unchanged (pinned by
`c3_v1_1_additive_fields_are_accepted_and_ignored`).

Validation rejects an intent when the schema version is unsupported, the
`intent_id` is missing/duplicated/non-conforming, the symbol is unsupported or
unmapped, `side` is not exactly `buy`/`sell`, the order type is unsupported, a
price/RR is non-finite, stop and target are on the wrong side of the entry, or
`now >= expires_at`.

## Risk management

`src/risk.rs` runs **immediately before the broker write**, after the Node 3
link gate. Node 3 projects the ATR-scaled prices; Node 4 only refuses (never
rewrites) them.

| Guard | Default | Rejection code |
|---|---|---|
| SL band | 200–300 pips ($2–3) | `risk_sl_out_of_band` |
| TP band | 600–800 pips ($6–8) | `risk_tp_out_of_band` |
| RR band | 2.0–3.0 | `risk_rr_out_of_band` |
| Declared RR vs prices | ±5 % | `risk_rr_mismatch` |
| Lots | ≤ 1.00 | `risk_lot_limit` |
| Per-trade risk USD | unset (disabled) | `risk_per_trade_cap` |
| Open positions | ≤ 3 | `risk_max_open_positions` |
| Daily realised loss | unset (disabled) | `risk_daily_loss_cap` |
| Max drawdown | unset (disabled) | `risk_max_drawdown` |
| Cooldown | 900 s after 3 consecutive losses | `risk_cooldown` |

`RISK_PIP_VALUE=0.01` and `RISK_CONTRACT_SIZE=100` convert a stop distance and
a lot size into USD risk. Realised PnL is fed from the MT5 bridge positions
snapshot: when a ticket disappears from `mt5_positions`, its last reported
`profit + swap` is recorded as a close. Every check, refusal reason and counter
is exposed in `GET /diagnostics` under `risk` (see
[`../contracts/c4-diagnostics.schema.json`](../contracts/c4-diagnostics.schema.json)).

## Reports back to Node 3

Same `intent_id`, with
`accepted | filled | partial | rejected | unknown | cancelled | closed`:

* `accepted` means "Node 4 durably owns this ID" (sent only after the ledger
  write);
* `filled`/`partial` come from a broker-confirmed fill;
* `rejected` means nothing reached the broker (including a risk refusal);
* `unknown` means a write may have reached the broker — Node 4 reconciles it
  by `intent_id` and **never** re-sends the order;
* a duplicate `intent_id` replays the recorded result without touching a
  broker.

## Public resources (C4)

`GET /health`, `/diagnostics` (`/status`), `/open-trades` (`/trades`),
`/deriv` (`/account`), `/mt5/account`, `/mt5/positions`, `/mt5/history`,
`/mt5/status`, `WS /ws`. Field reference:
[`../docs/mt5/FRONTEND_RESOURCES.md`](../docs/mt5/FRONTEND_RESOURCES.md).
`/diagnostics` has a `risk` object; it no longer has `market_feed` or
`strategy`.

`POST /mt5/control` is the only write route and is token-gated
(`X-Control-Token: $MT5_CONTROL_TOKEN`). It is never referenced by browser
code.

## Configuration

See [`.env.example`](.env.example). Highlights:

* `NODE3_WS_URL`, `NODE4_SHARED_TOKEN` — the single intent source.
* `RISK_*` — the risk policy above.
* `NODE4_DEMO_INTEGRATION_PASSED=1` — required before ANY real venue.
* `NODE4_LIVE_TRADING_APPROVED=1` — additionally required for `chelsea_live`.
* `EXECUTION_VENUE=none` — explicit dry-run/demo mode (default).
* `MT5_BRIDGE_TOKEN`, `MT5_CONTROL_TOKEN` — two separate secrets; the MT5
  password never leaves the bridge host.

## Test

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test  --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
cd .. && python3 ci/check_node4_boundary.py && python3 ci/contract_sync.py
```

`src/risk.rs` has a pure, clock-injectable unit-test suite covering every
guard; `src/node3.rs` covers the link, ledger, reconciliation and report path;
`src/intent.rs` covers validation and additive-frame compatibility.

## Demo execution

`EXECUTION_VENUE=none` is the explicit dry-run/demo mode: intents are
validated, risk-checked, durably recorded and reported, but no broker order is
placed. For broker-confirmed demo fills, use `EXECUTION_VENUE=deriv_mt5_demo`
with the MT5 **demo** bridge after passing the integration gate in
[`../docs/mt5/EXECUTION_ARCHITECTURE.md`](../docs/mt5/EXECUTION_ARCHITECTURE.md).
Node 4 deliberately has no price feed of its own (D1 puts all market data in
Node 1), so a local paper-fill simulator would require a second market-data
path and is intentionally not part of this service.
