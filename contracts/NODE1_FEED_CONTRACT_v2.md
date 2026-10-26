# C1 v2 — Node 1 market feed contract (NORMATIVE)

**Producer:** Node 1 (`xauusd-engine`). **Consumers:** Node 3 (VP + strategy),
Node 2 (dashboard), operators. **Version:** `2.0`. Supersedes
`NODE1_FEED_CONTRACT_v1_DRAFT.md` (historical, unsigned).

Node 1 is a **read-only market-data + calendar** service. No frame and no route
may carry a trade intent, signal, side, stop, target, size, order, fill or
account state in either direction.

## 1. Transport

* `WS /ws`, JSON text frames, one `subscribe` per connection.
* Client → Node 1: `{"type":"subscribe","topics":[...]}` and
  `{"type":"heartbeat"}`.
* Node 1 → clients: `candle`, `tick_volume`, `calendar`, `status`, `gap`,
  `heartbeat`.
* Heartbeat every 30 s in both directions. No `POST`/`PUT`/`PATCH`/`DELETE`
  route exists anywhere.

## 2. REST (all GET)

| Route | Purpose |
|---|---|
| `GET /health` | `ok` — process liveness only |
| `GET /status` | feed states, `version`, candle-cache edge, `contract_version`, `boot_id`, `freshness`, `market_open` |
| `GET /candles?interval=1m\|15m\|1h` | closed bars, ascending, unique by `time`, boundary-aligned |
| `GET /tick-volume` | tick-volume bars, `Cache-Control: no-store` |
| `GET /calendar` | economic calendar snapshot |

Node 1 may temporarily keep `/levels` and `/vp` as a deprecated read-through
proxy during migration. They are removed at cutover; the authoritative levels
endpoint is Node 3's `GET /levels`.

## 3. Envelope and ordering

* `contract_version: "2.0"` and `boot_id` (constant within a process) appear in
  `/status` and every `status` frame.
* **Live** frames carry a strictly increasing `seq` (across topics).
  **Replay** frames carry `replay: true` and **no** `seq`. A replay burst is
  ascending and the last replayed candles are all `closed: true`.
* Emit `{"type":"gap","data":{"dropped":N}}` when a subscriber overruns the
  broadcast buffer (4,096).
* Every `candle` carries `symbol: "XAUUSD"`, `interval` (`1m`/`15m`/`1h`),
  `time` (bucket-open epoch ms), `open`, `high`, `low`, `close`, `volume`
  (tick count), `source`, `closed`.
* `closed == true` is emitted **exactly once** per bucket, before the first
  frame of the next bucket. `/candles` contains only closed, ascending, unique
  bars. A full cache channel must not drop a closed candle.
* `tick_volume` carries `symbol` and `interval`; `ticks == candle.volume` for
  the same bucket.
* Levels identity is `(window, start, end)`; a newer `timestamp` replaces an
  older one. During migration only — Node 3 owns levels after cutover.

## 4. Freshness

```json
"freshness": {
  "feed": "sifting",
  "last_tick_ms": 1792781867153,
  "age_ms": 1240,
  "state": "live",
  "market_open": true
}
```

| State | Rule |
|---|---|
| `live` | connected and `age_ms <= 60000` while `market_open` |
| `stale` | connected but `age_ms > 60000` while `market_open` |
| `offline` | feed not connected (or never received a message) |
| `closed_market` | outside the XAUUSD session (Fri 17:00 → Sun 18:00 New York), regardless of age |

Published in `/status` and every `status` frame. A `status` frame is broadcast
on **every** `state` transition. Consumers must not derive decisions from a bar
that arrived while `freshness.state != live`.

## 5. Clock

All session/week boundaries are **18:00 America/New_York** (DST-aware;
UTC−4 EDT / UTC−5 EST). A trading week is Sun 18:00 → Fri 18:00. Sessions are
18:00 → next 18:00. Flat closed-market filler bars (`open == close`) are
published but are not a traded session.

## 6. Negative / boundary guarantees

* Inbound frames that look like execution (`trades`, `trade_intent`,
  `execution_hello`, `execution_report`, `order`, `bubbles`) are ignored and
  never rebroadcast.
* Published frames contain none of `side`, `stop_loss`, `take_profit`, `lots`,
  `risk_usd`, `action`, `venue`.
* No write route exists (`no_route_accepts_a_broker_write`).
* Non-finite / ≤ 0 prices, ticks > 5 s in the future, second-based timestamps
  and late ticks for a closed bucket are dropped with a counter increment.

## 7. Golden fixtures

Keep `ci/live/ws-sample.jsonl`, `candles.json`, `levels.json`,
`tick-volume.json`, `status.json`, `vp-{PW,PS,CW}.json` from the Node 1 branch
as the FRVP parity baseline. Node 3 must reproduce PW/PS/CW from Node 1
candles alone within one histogram row.
