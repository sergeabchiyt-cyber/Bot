# Live engine probe

generated: 2026-10-26T19:48:52Z

Read-only snapshot of the deployed `xauusd-engine` (C1 v2 market feed).

## /status

* service `xauusd-engine` version `0.4.0`
* **contract_version `2.0`** boot_id `boot-1793039839228-1` uptime 4242s
* market_open **True**; freshness `live` feed `sifting` age 283 ms, last tick 2026-10-26 19:48 UTC
* session: 2026-10-25 22:00 UTC -> 2026-10-26 22:00 UTC (18:00 America/New_York anchor)

| interval | bars | capacity | first bar | last closed bar | edge lag |
|---|---|---|---|---|---|
| 15m | 5 | 2000 | 2026-10-26 18:30 UTC | 2026-10-26 19:30 UTC | 18 min |
| 1h | 1 | 2000 | 2026-10-26 18:00 UTC | 2026-10-26 18:00 UTC | 108 min |
| 1m | 14468 | 43200 | 2026-10-12 18:38 UTC | 2026-10-26 19:47 UTC | 1 min |

* ticks: accepted 32324 / dropped 0 (price 0, seconds-ts 0, future 0, repeat 0, late 0)
* ws: topics ['candle', 'tick_volume', 'calendar', 'status'] heartbeat 30s buffer 4096
* calendar: source `pending` count 0 updated 1970-01-01 00:00 UTC

## /candles

### 1m

* **14468 bars**, 2026-10-12 18:38 UTC -> 2026-10-26 19:47 UTC
* deltas (min): [(1.0, 14464), (2881.0, 2), (3.0, 1)] -> **1m**
* on the 1m grid: 14468/14468; closed: 14468/14468; ascending: True
* sources: ['sifting']; price range: 3975.87 .. 4200.56
* newest bar is **2 minutes behind now**

### 15m

* **5 bars**, 2026-10-26 18:30 UTC -> 2026-10-26 19:30 UTC
* deltas (min): [(15.0, 4)] -> **15m**
* on the 15m grid: 5/5; closed: 5/5; ascending: True
* sources: ['sifting']; price range: 4008.49 .. 4015.86
* newest bar is **19 minutes behind now**

### 1h

* **1 bars**, 2026-10-26 18:00 UTC -> 2026-10-26 18:00 UTC
* deltas (min): [] -> **single bar**
* on the 1h grid: 1/1; closed: 1/1; ascending: True
* sources: ['sifting']; price range: 4012.14 .. 4015.86
* newest bar is **109 minutes behind now**

## /tick-volume

* 6 bar(s); newest 2026-10-26 19:45 UTC closed=False ticks=1377 (286↑ 322↓ 769=) at 4012.06

## /calendar

* source `pending` count **0** updated 1970-01-01 00:00 UTC

## Retired surfaces

C1 v2 has no fields to draw, no strategy and no simulator. A deployed build
still answering these predates the cutover (404 is the expected answer).

* `/levels` -> http 404
* `/vp` -> http 404
* `/strategy` -> http 404
* `/demo` -> http 404
* `/ai` -> http 404

## WebSocket sample

* frames: {'heartbeat': 2, 'candle': 1534, 'tick_volume': 435, 'calendar': 1, 'status': 1}
* replay: {'candle': 246, 'tick_volume': 6, 'calendar': 1, 'status': 1}; live (seq'd): 1717; feed frames with neither: 0
* live seq range: 129543 .. 131259, strictly increasing: True
* `candle` 15m: 434 frames, deltas (min) [(0.0, 428), (15.0, 5)]
* `candle` 1h: 430 frames, deltas (min) [(0.0, 428), (60.0, 1)]
* `candle` 1m: 670 frames, deltas (min) [(0.0, 429), (1.0, 240)]
* last candle frame: {"close": 4012.08, "closed": false, "high": 4012.39, "interval": "1m", "low": 4012.0, "open": 4012.21, "source": "sifting", "symbol": "XAUUSD", "time": 1793044080000, "volume": 473.0}

## C1 contract matrix

    frames: 5 kind(s) ['calendar', 'candle', 'heartbeat', 'status', 'tick_volume'], 254 replay, 1717 live (seq [129543]..[131259])
    C1 CONTRACT MATRIX PASSED

## Verdict

* the deployed engine serves **C1 v2**
* chart candles: **15m** (5 bars, 2026-10-26 18:30 UTC -> 2026-10-26 19:30 UTC)
* edge lag now: {'15m': 18, '1h': 108, '1m': 1}
