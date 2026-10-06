# Live engine probe

generated: 2026-10-06T16:51:57Z

## /status

* version `0.3.0` venue `DerivDemo`
* feeds connected: 8 / 12
* **candles: interval='15m' seed_bars=2000**
* history edge: 2026-10-06 16:30 UTC (**20 min behind now**) over 2000 bars
* volume profile: row_mode=rows rows=128 bin=0.5 va=70.0 input=tv

## /candles

* **2000 bars**, 2026-09-14 20:00 UTC -> 2026-10-06 16:30 UTC
* deltas (min): [(15.0, 1987), (195.0, 4), (165.0, 2), (105.0, 1), (45.0, 1)]  -> **15m**
* open==close bars: 423 (21%) in 47 runs; longest 48 bars
* price range: 4105.44 .. 4399.60
* newest bar is **22 minutes behind now**

## /levels

* windows: PW, PS, CW, SWING_BULL
* `meta` present: **True** (the branch adds it)

| window | poc | vah | val | start | end |
|---|---|---|---|---|---|
| PW | 4155.780000000001 | 4182.6900000000005 | 4134.39 | 2026-09-27 22:00 UTC | 2026-10-02 22:00 UTC |
| PS | 4158.305 | 4169.22 | 4144.06 | 2026-10-04 22:00 UTC | 2026-10-05 22:00 UTC |
| CW | 4158.305 | 4169.22 | 4144.06 | 2026-10-04 22:00 UTC | 2026-10-05 22:00 UTC |
| SWING_BULL | 4164.705 | 4170.48 | 4155.570000000001 | 2026-10-06 14:30 UTC | 2026-10-06 16:00 UTC |

## /vp audit

* `vp-PW`: window=PW rows=128 row_height=1.3800000000000001 row_mode=rows
  range 4110.93..4287.25 input=5m (1428 bars) volume=17577744099.00134 va=0.7
  poc=4155.780000000001 (row 32) val=4134.39 (row 17) vah=4182.6900000000005 (row 51) histogram rows in payload: 128
* `vp-PS`: window=PS rows=128 row_height=0.37 row_mode=rows
  range 4123.34..4170.38 input=1m (1439 bars) volume=2262765129.999364 va=0.7
  poc=4158.305 (row 94) val=4144.06 (row 56) vah=4169.22 (row 123) histogram rows in payload: 128
* `vp-CW`: window=CW rows=128 row_height=0.37 row_mode=rows
  range 4123.34..4170.38 input=1m (1439 bars) volume=2262765129.999364 va=0.7
  poc=4158.305 (row 94) val=4144.06 (row 56) vah=4169.22 (row 123) histogram rows in payload: 128
* `vp-PW-64`: window=PW rows=64 row_height=2.7600000000000002 row_mode=rows
  range 4110.93..4287.25 input=5m (1428 bars) volume=17577744099.00129 va=0.7
  poc=4156.47 (row 16) val=4135.77 (row 9) vah=4185.450000000001 (row 26) histogram rows in payload: 64
* `vp-6h`: window=CUSTOM rows=128 row_height=0.28 row_mode=rows
  range 4143.81..4179.64 input=1m (359 bars) volume=106190.99999990451 va=0.7
  poc=4163.27 (row 69) val=4152.21 (row 30) vah=4170.6900000000005 (row 95) histogram rows in payload: 128
* `vp-24h`: window=CUSTOM rows=128 row_height=0.58 row_mode=rows
  range 4105.44..4179.64 input=1m (1439 bars) volume=406784.9999999543 va=0.7
  poc=4139.95 (row 59) val=4125.16 (row 34) vah=4157.0599999999995 (row 88) histogram rows in payload: 128
* `vp-bad`: http 404 -> {"error": "no profile computed for this window yet", "window": "NOPE"}
* `vp-bad-range`: http 400 -> {"error": "start and end must both be epoch milliseconds"}

## WebSocket sample

* frames: {'probe': 1, 'heartbeat': 2, 'levels': 4, 'candle': 238, 'tick_volume': 324, 'calendar': 1, 'status': 1}
* **live `candle` frames: 238, deltas (min) [(0.0, 222), (15.0, 15)] -> no bars**
* first frame: {"time": 1791291600000, "open": 4170.1, "high": 4174.53, "low": 4167.9, "close": 4172.37, "volume": 4501.0, "source": "sifting"}
* live `tick_volume` deltas (min): [(0.0, 222), (15.0, 101)] -> no bars
* `levels` frames carrying `meta`: **True**

## Verdict

* the deployed engine serves **15m** chart candles (2000 bars, 2026-09-14 20:00 UTC -> 2026-10-06 16:30 UTC)
* `/status` reports the chart edge: 2000 bars, last 2026-10-06 16:30 UTC, **20 min behind now**
