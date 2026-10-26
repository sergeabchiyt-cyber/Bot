# Live engine probe

generated: 2026-10-26T13:59:33Z

Read-only snapshot of the deployed `xauusd-engine` (C1 v2 market feed).

## /status

* service `None` version `0.4.0`
* **contract_version `None`** boot_id `None` uptime 0s
* the deployed build **predates the C1 v2 cutover** — redeploy to serve the contract this branch implements
* market_open **None**; freshness `None` feed `None` age None ms, last tick ?
* **retired sections still present**: ['strategy', 'demo', 'volume_profile']

## /candles

### 1m

* **2000 bars**, 2026-10-03 12:30 UTC -> 2026-10-26 13:30 UTC
* deltas (min): [(15.0, 1984), (45.0, 3), (30.0, 3), (60.0, 1)] -> **15m**
* on the 1m grid: 2000/2000; closed: 0/2000; ascending: True
* sources: ['sifting']; price range: 3975.87 .. 4217.36
* newest bar is **30 minutes behind now**

### 15m

* **2000 bars**, 2026-10-03 12:30 UTC -> 2026-10-26 13:30 UTC
* deltas (min): [(15.0, 1984), (45.0, 3), (30.0, 3), (60.0, 1)] -> **15m**
* on the 15m grid: 2000/2000; closed: 0/2000; ascending: True
* sources: ['sifting']; price range: 3975.87 .. 4217.36
* newest bar is **30 minutes behind now**

### 1h

* **2000 bars**, 2026-10-03 12:30 UTC -> 2026-10-26 13:30 UTC
* deltas (min): [(15.0, 1984), (45.0, 3), (30.0, 3), (60.0, 1)] -> **15m**
* on the 1h grid: 500/2000; closed: 0/2000; ascending: True
* sources: ['sifting']; price range: 3975.87 .. 4217.36
* newest bar is **30 minutes behind now**

## /tick-volume

* 196 bar(s); newest 2026-10-26 13:45 UTC closed=False ticks=6145 (1692↑ 2086↓ 2367=) at 3989.49

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

* frames: {'heartbeat': 2, 'candle': 231, 'tick_volume': 411, 'calendar': 1, 'status': 1}
* replay: {}; live (seq'd): 0; feed frames with neither: 644
* `candle` None: 231 frames, deltas (min) [(0.0, 215), (15.0, 15)]
* last candle frame: {"time": 1793022300000, "open": 4000.77, "high": 4001.64, "low": 3988.78, "close": 3991.64, "volume": 6377.0, "source": "sifting"}

## C1 contract matrix

    Traceback (most recent call last):
      File "/home/runner/work/Bot/Bot/ci/verify_contract.py", line 165, in <module>
        check(candle["interval"] == name, f"{where}: interval is {candle['interval']!r}")
              ~~~~~~^^^^^^^^^^^^
    KeyError: 'interval'

## Verdict

* the deployed engine serves contract `None` — the branch head is 2.0, so the deployment is behind
* chart candles: **15m** (2000 bars, 2026-10-03 12:30 UTC -> 2026-10-26 13:30 UTC)
