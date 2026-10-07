# Live engine probe

generated: 2026-10-07T04:01:36Z

## /status

* version `0.3.0` service `market-data`
* feeds connected: 8 / 12
* **candles: interval='15m' seed_bars=2000**
* history edge: 2026-10-07 03:45 UTC (**15 min behind now**) over 2000 bars
* volume profile: row_mode=rows rows=128 bin=0.5 va=70.0 input=tv

## /candles

* **2000 bars**, 2026-09-15 07:15 UTC -> 2026-10-07 03:45 UTC
* deltas (min): [(15.0, 1987), (195.0, 4), (165.0, 2), (105.0, 1), (45.0, 1)]  -> **15m**
* open==close bars: 423 (21%) in 47 runs; longest 48 bars
* price range: 4105.44 .. 4399.60
* newest bar is **17 minutes behind now**

## /levels

* windows: PW, PS, CW, SWING_BULL
* `meta` present: **True** (the branch adds it)

| window | poc | vah | val | start | end |
|---|---|---|---|---|---|
| PW | 4155.780000000001 | 4182.6900000000005 | 4134.39 | 2026-09-27 22:00 UTC | 2026-10-02 22:00 UTC |
| PS | 4163.41 | 4178.599999999999 | 4137.679999999999 | 2026-10-05 22:00 UTC | 2026-10-06 22:00 UTC |
| CW | 4141.09 | 4160.62 | 4125.9 | 2026-10-04 22:00 UTC | 2026-10-06 22:00 UTC |
| SWING_BULL | 4147.214999999999 | 4147.8099999999995 | 4143.61 | 2026-10-07 02:00 UTC | 2026-10-07 03:00 UTC |

## /vp audit

* `vp-PW`: window=PW rows=128 row_height=1.3800000000000001 row_mode=rows
  range 4110.93..4287.25 input=5m (1428 bars) volume=17577744099.00134 va=0.7
  poc=4155.780000000001 (row 32) val=4134.39 (row 17) vah=4182.6900000000005 (row 51) histogram rows in payload: 128
* `vp-PS`: window=PS rows=128 row_height=0.62 row_mode=rows
  range 4105.44..4184.24 input=1m (1439 bars) volume=3694939553.9994354 va=0.7
  poc=4163.41 (row 93) val=4137.679999999999 (row 52) vah=4178.599999999999 (row 117) histogram rows in payload: 128
* `vp-CW`: window=CW rows=128 row_height=0.62 row_mode=rows
  range 4105.44..4184.24 input=1m (2879 bars) volume=6790264534.998914 va=0.7
  poc=4141.09 (row 57) val=4125.9 (row 33) vah=4160.62 (row 88) histogram rows in payload: 128
* `vp-PW-64`: window=PW rows=64 row_height=2.7600000000000002 row_mode=rows
  range 4110.93..4287.25 input=5m (1428 bars) volume=17577744099.00129 va=0.7
  poc=4156.47 (row 16) val=4135.77 (row 9) vah=4185.450000000001 (row 26) histogram rows in payload: 64
* `vp-6h`: window=CUSTOM rows=127 row_height=0.25 row_mode=rows
  range 4138.3..4169.95 input=1m (354 bars) volume=167022934.00000003 va=0.7
  poc=4166.675 (row 113) val=4163.3 (row 100) vah=4167.3 (row 115) histogram rows in payload: 127
* `vp-24h`: window=CUSTOM rows=129 row_height=0.55 row_mode=rows
  range 4113.57..4184.24 input=1m (1433 bars) volume=2867830752.00088 va=0.7
  poc=4163.344999999999 (row 90) val=4150.42 (row 67) vah=4177.37 (row 115) histogram rows in payload: 129
* `vp-bad`: http 404 -> {"error": "no profile computed for this window yet", "window": "NOPE"}
* `vp-bad-range`: http 400 -> {"error": "start and end must both be epoch milliseconds"}

## WebSocket sample

* frames: {'probe': 1, 'heartbeat': 2, 'levels': 4, 'candle': 236, 'tick_volume': 241, 'calendar': 1, 'status': 1}
* **live `candle` frames: 236, deltas (min) [(0.0, 220), (15.0, 15)] -> no bars**
* first frame: {"time": 1791332100000, "open": 4169.04, "high": 4169.36, "low": 4160.15, "close": 4160.15, "volume": 4314.0, "source": "sifting"}
* live `tick_volume` deltas (min): [(0.0, 220), (15.0, 20)] -> no bars
* `levels` frames carrying `meta`: **True**

## Verdict

* the deployed engine serves **15m** chart candles (2000 bars, 2026-09-15 07:15 UTC -> 2026-10-07 03:45 UTC)
* `/status` reports the chart edge: 2000 bars, last 2026-10-07 03:45 UTC, **15 min behind now**
