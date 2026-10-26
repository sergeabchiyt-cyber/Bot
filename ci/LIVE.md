# Live engine probe

generated: 2026-10-26T14:01:45Z

Read-only snapshot of the deployed `xauusd-engine` (C1 v2 market feed).

## /status

* service `None` version `0.4.0`
* **contract_version `None`** boot_id `None` uptime 0s
* the deployed build **predates the C1 v2 cutover** — redeploy to serve the contract this branch implements
* market_open **None**; freshness `None` feed `None` age None ms, last tick ?
* **retired sections still present**: ['strategy', 'demo', 'volume_profile']

## /candles

### 1m

* **2000 bars**, 2026-10-03 12:45 UTC -> 2026-10-26 13:45 UTC
* deltas (min): [(15.0, 1984), (45.0, 3), (30.0, 3), (60.0, 1)] -> **15m**
* on the 1m grid: 2000/2000; closed: 0/2000; ascending: True
* sources: ['sifting']; price range: 3975.87 .. 4217.36
* newest bar is **17 minutes behind now**

### 15m

* **2000 bars**, 2026-10-03 12:45 UTC -> 2026-10-26 13:45 UTC
* deltas (min): [(15.0, 1984), (45.0, 3), (30.0, 3), (60.0, 1)] -> **15m**
* on the 15m grid: 2000/2000; closed: 0/2000; ascending: True
* sources: ['sifting']; price range: 3975.87 .. 4217.36
* newest bar is **17 minutes behind now**

### 1h

* **2000 bars**, 2026-10-03 12:45 UTC -> 2026-10-26 13:45 UTC
* deltas (min): [(15.0, 1984), (45.0, 3), (30.0, 3), (60.0, 1)] -> **15m**
* on the 1h grid: 500/2000; closed: 0/2000; ascending: True
* sources: ['sifting']; price range: 3975.87 .. 4217.36
* newest bar is **17 minutes behind now**

## /tick-volume

* 197 bar(s); newest 2026-10-26 14:00 UTC closed=False ticks=150 (65↑ 45↓ 40=) at 3996.64

## /calendar

* source `ff_json` count **96** updated 2026-10-26 06:31 UTC
  * 2026-10-25T19:00:00+00:00    |  NZD | Holiday  | Bank Holiday
  * 2026-10-25T21:10:00+00:00    |  AUD | Low      | RBA Assist Gov Jones Speaks
  * 2026-10-26T09:00:00+00:00    |  EUR | Low      | German ifo Business Climate
  * 2026-10-26T11:00:00+00:00    |  GBP | Low      | CBI Realized Sales
  * 2026-10-26T23:50:00+00:00    |  JPY | Low      | SPPI y/y
  * 2026-10-27T00:01:00+00:00    |  GBP | Low      | BRC Shop Price Index y/y
  * 2026-10-27T05:00:00+00:00    |  JPY | Low      | BOJ Core CPI y/y
  * 2026-10-27T08:00:00+00:00    |  EUR | Low      | Spanish Unemployment Rate
  * 2026-10-27T09:00:00+00:00    |  EUR | Low      | M3 Money Supply y/y
  * 2026-10-27T09:00:00+00:00    |  EUR | Low      | Private Loans y/y

## Retired surfaces

C1 v2 has no fields to draw, no strategy and no simulator. A deployed build
still answering these predates the cutover (404 is the expected answer).

* `/levels` -> http 200
* `/vp` -> http 200
* `/strategy` -> http 200
* `/demo` -> http 200
* `/ai` -> http 200

## WebSocket sample

* frames: {'heartbeat': 2, 'candle': 249, 'tick_volume': 430, 'calendar': 1, 'status': 3}
* replay: {}; live (seq'd): 0; feed frames with neither: 683
* `candle` None: 249 frames, deltas (min) [(0.0, 233), (15.0, 15)]
* last candle frame: {"time": 1793023200000, "open": 3993.15, "high": 3997.55, "low": 3992.78, "close": 3996.79, "volume": 449.0, "source": "sifting"}

## C1 contract matrix

      $: matches none of the anyOf branches
      - frames.jsonl:684: a live frame has no integer seq
      - frames.jsonl:685 violates the C1 schema:
      $: matches none of the anyOf branches
      $.data: missing required property 'symbol'
      $.data: missing required property 'interval'
      $.data: missing required property 'closed'
      - frames.jsonl:685: a live frame has no integer seq

## Verdict

* the deployed engine serves contract `None` — the branch head is 2.0, so the deployment is behind
* chart candles: **15m** (2000 bars, 2026-10-03 12:45 UTC -> 2026-10-26 13:45 UTC)
