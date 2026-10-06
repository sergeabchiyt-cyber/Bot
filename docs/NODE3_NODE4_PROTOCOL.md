# Node 3 ↔ Node 4 execution protocol

Protocol version: **1**

Intent schema version: **1**

This contract is the only runtime coupling between the strategy service and the
execution service. Node 4 connects outbound to Node 3; Node 3 never needs an
inbound route to a broker host and never receives broker credentials.

## Transport and authentication

- URL: `wss://<node3-host>/execution`
- Node 4 sends `execution_hello` as the first frame within five seconds.
- Both services hold the same `NODE4_SHARED_TOKEN` secret.
- Node 3 permits one authenticated execution consumer. A second is rejected.
- A missing token, bad token, or protocol mismatch closes the socket.
- Never send this token to Node 2 or include it in logs/snapshots.

```json
{
  "type": "execution_hello",
  "token": "secret",
  "service": "xauusd-node4-execution",
  "protocol_version": 1
}
```

Success:

```json
{
  "type": "execution_hello_ack",
  "accepted": true,
  "protocol_version": 1
}
```

Failure has `accepted: false` and a non-secret `error`, then the socket closes.

## Trade intent: Node 3 → Node 4

```json
{
  "type": "trade_intent",
  "data": {
    "schema_version": 1,
    "intent_id": "n3-xauusd-1700000000000-buy-pw-poc",
    "strategy": "vp_break_retest_v1",
    "symbol": "XAUUSD",
    "side": "buy",
    "order_type": "market",
    "reference_price": 2650.25,
    "stop_loss": 2647.45,
    "take_profit": 2656.25,
    "risk_reward": 2.142857,
    "level_name": "PW PoC",
    "source_candle_time": 1700000000000,
    "created_at": 1700000000100,
    "expires_at": 1700000120100
  }
}
```

The intent deliberately contains no venue, account, stake, lots, leverage,
slippage, MT5 login, or broker credential. Strategy stop/target prices are hard
constraints; Node 4 may reject them but must not silently rewrite strategy
meaning. Broker normalization that changes a price must be recorded in the
execution report/audit ledger.

## Idempotency and delivery

`intent_id` is the idempotency key across Node 3, Node 4, the MT5 bridge, EA
comment, reconciliation, and audit logs.

Node 4 must perform these steps in order:

1. Parse and validate schema, finite numeric values, side, symbol, and order type.
2. Reject when `now >= expires_at`.
3. Atomically persist `intent_id` as received before any broker write.
4. If the ID already exists, return its recorded state; do not execute again.
5. Apply Node 4 account/venue/risk policy and either reject or accept.
6. Return an `execution_report`. An `accepted` report means Node 4 has durably
   assumed responsibility for that ID, not that a broker fill exists.
7. Place at most one broker order and reconcile unknown outcomes by ID; never
   retry an unclear write as a new order.

Node 3 keeps unacknowledged intents in a bounded in-memory queue and replays
unexpired ones after reconnect. Replay is expected and safe only because Node 4
persists/deduplicates the ID.

## Execution report: Node 4 → Node 3

```json
{
  "type": "execution_report",
  "data": {
    "schema_version": 1,
    "intent_id": "n3-xauusd-1700000000000-buy-pw-poc",
    "status": "filled",
    "venue": "deriv_mt5_demo",
    "symbol": "XAUUSD",
    "side": "buy",
    "timestamp": 1700000001400,
    "execution_id": "mt5-deal-456",
    "filled_price": 2650.28,
    "quantity": 0.01,
    "quantity_unit": "lots"
  }
}
```

Allowed status values:

| Status | Meaning |
|---|---|
| `accepted` | Node 4 durably owns the ID; broker result pending |
| `filled` | broker-authoritative full fill |
| `partial` | broker-authoritative partial fill |
| `rejected` | Node 4 or broker refused; no unknown write |
| `unknown` | a write may have reached the broker; reconcile, never resend |
| `cancelled` | pending order cancelled with broker confirmation |
| `closed` | position/deal lifecycle is complete |

Rejected/unknown reports should include `error_code` and `error_message`.
Secrets, raw tokens, passwords, and full broker request headers are forbidden.

Node 3 answers every syntactically valid report attempt:

```json
{
  "type": "execution_report_ack",
  "intent_id": "n3-xauusd-1700000000000-buy-pw-poc",
  "accepted": true
}
```

A report is rejected when the schema/status is unsupported, the intent ID is
unknown, symbol/side do not match, or venue is empty. A valid first report
removes the intent from Node 3's replay queue. Further lifecycle reports remain
valid and appear in public signal diagnostics.

## Heartbeats and reconnects

Either side may send `{"type":"heartbeat"}` and the peer replies in kind. Node
4 reconnects with bounded exponential backoff and repeats `execution_hello`.
Node 3 then replays currently valid pending intents.

Node 4 must also have an independent stale-link rule: no new broker write after
its Node 3 session is stale/disconnected, but broker monitoring, position
management, reconciliation, and operator kill switch must continue.

## Public/private separation

`/ws` is Node 3's public diagnostics socket. It never sends `trade_intent`,
`execution_hello*`, `execution_report`, or `execution_report_ack`, including for
an `all` subscription. `/execution` never sends browser diagnostics. This
prevents accidental execution consumers and keeps the browser credential-free.

`GET /signals` and public `signals` snapshots may show non-secret intent/report
metadata for operators. They are observability resources, not an execution
transport.

## Version changes

Additive optional fields may retain version 1. Any changed meaning, required
field, status semantics, authentication flow, or idempotency rule requires a
new protocol/schema version. Unknown versions fail closed; do not guess.
