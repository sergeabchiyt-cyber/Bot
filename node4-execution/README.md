# Node 4 — Execution Service (XAUUSD)

The single execution service of the three-node split. Node 4 is a Rust service
that consumes **authenticated trade intents from Node 3** (the strategy engine),
applies venue policy, places **at most one broker order per `intent_id`**, and
reports the broker-authoritative result back to Node 3.

It owns every execution concern:

1. the outbound intent link to Node 3 (`wss://<node3-host>/execution`, protocol v1),
2. venue policy (exactly one of `deriv_mt5_demo`, `deriv_demo`, `chelsea_live`,
   `none` — no fallback, ever),
3. stake/lot sizing, symbol mapping, account risk caps and demo safeguards,
4. the durable idempotency ledger (`intent_id` is the system-wide key),
5. the Deriv options adapter, the Chelsea MCP adapter, the Deriv MT5 **demo**
   execution adapter and its `mt5-bridge`,
6. broker/account/position/history state, reconciliation of unclear writes,
7. the operator kill switch and the audit trail.

It owns **no** strategy and **no** market data. There is no candle, level,
volume-profile, indicator or trigger state machine in Node 4, and it never
recomputes the stop-loss or take-profit Node 3 sends: it may refuse an intent,
but it never rewrites its meaning.

```
+-----------------------------+         execution_hello / trade_intent
|  Node 3  (strategy engine)  | <--------------------------------------+
|  intent: side, SL, TP, RR   |                                      |
+-----------------------------+         execution_report (+ ack)     |
                                                                     |
+-----------------------------------------------------------------+  |
|  Node 4  (node4-execution)                                      |  |
|                                                                 |--+
|  1. validate intent (schema, ids, symbol, side, prices, expiry)  |
|  2. duplicate? -> replay the recorded result, never a 2nd order  |
|  3. persist intent_id durably, report `accepted`                 |
|  4. exactly one venue, pre-write health checks, risk cap         |
|  5. one order -> filled / partial / rejected / unknown           |
|  6. unknown -> reconcile by intent_id, never resend              |
+-----------------------------------------------------------------+
        |                    |                       |
        | Deriv options      | Chelsea MCP           | WS /mt5/bridge (outbound bridge)
        v                    v                       v
   Deriv demo API      Chelsea live MCP        mt5-bridge -> EA -> MT5 demo terminal
```

The MT5 demo venue executes through `../mt5-bridge`, a separate service that runs
next to the MT5 terminal and **dials out** to this one. Node 4 never holds the MT5
password, never opens an inbound connection to the terminal host, and never marks
a trade open without a broker-confirmed fill. See
[`../docs/mt5/EXECUTION_ARCHITECTURE.md`](../docs/mt5/EXECUTION_ARCHITECTURE.md)
and [`../mt5-bridge/README.md`](../mt5-bridge/README.md).

## Intent contract (protocol version 1)

Node 4 sends the first frame and will not accept an intent before Node 3
acknowledges it:

```json
{"type":"execution_hello","token":"<NODE4_SHARED_TOKEN>","service":"xauusd-node4-execution","protocol_version":1}
{"type":"execution_hello_ack","accepted":true,"protocol_version":1}
```

An intent (`trade_intent.data`) carries `schema_version`, `intent_id`,
`strategy`, `symbol`, `side`, `order_type`, `reference_price`, `stop_loss`,
`take_profit`, `risk_reward`, `level_name`, `source_candle_time`, `created_at`,
`expires_at` — and **no** venue, account, stake, lot size or credential: those are
Node 4's. Validation rejects an intent when the schema version is unsupported,
`intent_id` is missing/duplicated/non-conforming, the symbol is unsupported or
unmapped, `side` is not exactly `buy`/`sell`, the order type is unsupported, a
price/RR is non-finite, stop and target are on the wrong side of the entry, or
`now >= expires_at`.

Reports back to Node 3 use the same `intent_id`, with
`accepted | filled | partial | rejected | unknown | cancelled | closed`:

* `accepted` means "Node 4 durably owns this ID" and is only sent **after** the
  intent is in the ledger;
* `filled`/`partial` come from a broker-confirmed fill;
* `rejected` means nothing reached the broker;
* `unknown` means a write may have reached the broker — Node 4 reconciles it by
  `intent_id`/comment/order/deal/position, publishes the reconciled result, and
  **never** re-sends the order;
* a duplicate `intent_id` replays the recorded result (`duplicate: true`) without
  touching a broker.

## Idempotency ledger

`EXECUTION_LEDGER_FILE` (default `data/execution_ledger.jsonl`) is an append-only
JSONL file, `write_all` + `flush` + `sync_data` on every record, replayed at
startup. It records intents, the accepted report, the broker command, every
lifecycle report, reconciliation results and duplicate replays. No secret
(token, password, header) is ever written. When a **real** venue is selected and
the ledger cannot be written, Node 4 refuses to trade — an order it cannot
deduplicate after a crash must not exist.

## HTTP & WebSocket resources (`PORT=10000`)

Public, read-only:

| Endpoint | Description |
|---|---|
| `WS /ws` (or `/`) | Live stream of `diagnostics`, `open_trades`, `deriv_account`, `mt5_account`, `mt5_positions`, `mt5_history`, `bridge_status`, `trades`, `diagnostic_event`. |
| `GET /health` | Plaintext `ok` for platform probes. |
| `GET /diagnostics` (or `/status`) | JSON `ExecutionSnapshot`: the Node 3 link health, the ledger state, execution counters, open trades, Deriv account, the whole MT5 bridge view, and the event log. |
| `GET /open-trades` (or `/trades`) | JSON `OpenTradesSnapshot` (`node4_open_trades`, `deriv_open_trades`, `mt5_open_positions`, `recent_trades`). |
| `GET /deriv` (or `/account`) | JSON `DerivAccountSnapshot` (`balance`, `currency`, `account_id`, `open_trades`, `total_unrealized_pnl`). |
| `GET /mt5/account` | JSON `Mt5AccountSnapshot`: configured/connected/authorized, account type, login/server, balance/equity/margin, symbol contract data, halted/halt_reason, error, setup_hint. |
| `GET /mt5/positions` | JSON `Mt5PositionsSnapshot`: broker positions (volume, SL/TP, current price, unrealized PnL). |
| `GET /mt5/history` | JSON `Mt5HistorySnapshot`: closed deals with profit/swap/commission and realized totals. |
| `GET /mt5/status` | JSON `Mt5BridgeStatus`: bridge/EA link state, protocol, counters, uptime, last error. |

Private, token-gated (no permissive CORS):

| Endpoint | Description |
|---|---|
| `WS /mt5/bridge` | The MT5 bridge dials out here and authenticates with `bridge_hello { token: $MT5_BRIDGE_TOKEN }`. Never a frontend endpoint. |
| `POST /mt5/control` | Kill switch: `{"action":"halt"\|"resume"\|"close_all"\|"close_position"}` with header `X-Control-Token: $MT5_CONTROL_TOKEN`; 403 when that token is unset or wrong. `halt` accepts `"flatten": true` to close everything first. |

WS clients may send `{"type":"subscribe","topics":[...]}`,
`{"type":"snapshot"}` and `{"type":"heartbeat"}`. Bridge-only frames
(`bridge_hello`, `bridge_hello_ack`, `bridge_ack`) are never forwarded to
frontends, and Node 3's intent/report frames never appear on `/ws` at all.

**Node 3 link health is reported separately from broker/bridge health**: the
`node3` section of `/diagnostics` covers the intent socket (connected,
authenticated, last frame, reports sent/acked, reconnect count), while the `mt5`
section covers the bridge and the terminal.

## Configuration

Every variable is documented with its default in
[`.env.example`](.env.example). The important ones:

| Variable | Meaning |
|---|---|
| `NODE3_WS_URL` | `wss://<node3-host>/execution` — required; Node 4 refuses to start without it. |
| `NODE4_SHARED_TOKEN` | Required; sent in `execution_hello`. Must equal Node 3's. |
| `NODE3_HELLO_TIMEOUT_MS`, `NODE3_RECONNECT_MIN_MS`, `NODE3_RECONNECT_MAX_MS`, `NODE3_STALE_MS` | Handshake deadline, bounded reconnect backoff, and the link-staleness window. |
| `EXECUTION_LEDGER_FILE` | Durable idempotency ledger (default `data/execution_ledger.jsonl`). |
| `EXECUTION_VENUE` | `deriv_mt5_demo` \| `deriv_demo` \| `chelsea_live` \| `none`. Empty = auto-detect the single configured venue. |
| `EXECUTION_STAKE` (alias `ORDER_SIZE`), `DERIV_MIN_STAKE` | USD stake for the Deriv options venue. |
| `MT5_BRIDGE_TOKEN`, `MT5_CONTROL_TOKEN` | Two **separate** secrets: bridge handshake, and the operator kill switch. |
| `MT5_SYMBOL`, `MT5_SYMBOL_MAP`, `MT5_ALLOW_OTHER_SYMBOL` | Explicit, fail-closed symbol mapping (never guessed). |
| `MT5_VOLUME_LOTS`, `MT5_MAX_RISK_PER_TRADE`, `MT5_ORDER_TIMEOUT_MS`, `MT5_HISTORY_PAGE_SIZE` | MT5 sizing, optional risk cap, fill timeout, history paging. |

Startup fails hard on: a missing/invalid `NODE3_WS_URL`, a missing
`NODE4_SHARED_TOKEN`, a contradictory venue configuration (two credential sets,
or an explicit venue whose credentials are absent), an unmapped symbol with a
real venue selected, or a non-positive stake.

## Running

```bash
cp .env.example .env          # fill in the values; never commit .env
cargo run --release           # binds 0.0.0.0:$PORT
```

With no venue credential and `EXECUTION_VENUE=none` the service is an explicit
dry-run: it validates, records and reports intents, and places nothing.

Docker (execution service only; the bridge is separate):

```bash
docker build -f ../Dockerfile -t xauusd-node4-execution ..
docker run --env-file .env -p 10000:10000 xauusd-node4-execution
```

## Validation

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo test  --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
cd .. && python3 ci/mt5/protocol_lint.py     # EA ⇄ bridge ⇄ Node 4 contract lint
```

`ci/mt5/protocol_lint.py` fails the build if this crate's `src/types.rs` drifts
from the bridge's `src/snapshot.rs`, or if a bridge frame/command loses its
counterpart.

## Safety rules this service enforces

* Node 4 is a **client** of Node 3; a dead Node 3 link means no new positions,
  while monitoring, reconciliation and the kill switch keep working.
* One venue, chosen explicitly, never by fallback.
* Demo guards are layered: Node 4 checks account type/authorization/halt state,
  the bridge repeats the checks, and the EA refuses writes on a non-demo account.
* A broker-confirmed fill is the only thing that may open a trade; a timeout is
  `unknown` and is reconciled, never retried as a new order.
* `MT5_BRIDGE_TOKEN` and `MT5_CONTROL_TOKEN` are separate secrets, and the Node 3
  `NODE4_SHARED_TOKEN` is a third one: one secret per link.
* Live trading is off by default: keep `EXECUTION_VENUE=none` or run MT5 in
  monitor-only mode until the demo integration gate in
  `../docs/mt5/EXECUTION_ARCHITECTURE.md` has passed on a demo account.
