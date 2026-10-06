# Live engine probe

generated: 2026-10-05T15:27:39Z

## /status

* version `0.3.0` venue `DerivDemo`
* feeds connected: 7 / 12
* **candles: interval='15m' seed_bars=2000**
* history edge: 2026-10-05 15:15 UTC (**11 min behind now**) over 2062 bars
* volume profile: row_mode=rows rows=128 bin=0.5 va=70.0 input=tv

## /candles

* **2062 bars**, 2026-09-13 02:45 UTC -> 2026-10-05 15:15 UTC
* deltas (min): [(15.0, 2048), (195.0, 4), (45.0, 2), (165.0, 2), (105.0, 1)]  -> **15m**
* open==close bars: 486 (23%) in 51 runs; longest 55 bars
* price range: 4111.09 .. 4399.60
* newest bar is **13 minutes behind now**

## /levels

* windows: PW, PS, SWING_BEAR
* `meta` present: **True** (the branch adds it)

| window | poc | vah | val | start | end |
|---|---|---|---|---|---|
| PW | 4155.780000000001 | 4182.6900000000005 | 4134.39 | 2026-09-27 22:00 UTC | 2026-10-02 22:00 UTC |
| PS | 4137.549999999999 | 4139.009999999999 | 4137.53 | 2026-10-03 22:00 UTC | 2026-10-04 22:00 UTC |
| SWING_BEAR | 4153.325000000001 | 4165.070000000001 | 4146.9800000000005 | 2026-10-05 12:30 UTC | 2026-10-05 14:00 UTC |

## /vp audit

* `vp-PW`: window=PW rows=128 row_height=1.3800000000000001 row_mode=rows
  range 4110.93..4287.25 input=5m (1428 bars) volume=17577744099.00134 va=0.7
  poc=4155.780000000001 (row 32) val=4134.39 (row 17) vah=4182.6900000000005 (row 51) histogram rows in payload: 128
* `vp-PS`: window=PS rows=138 row_height=0.04 row_mode=rows
  range 4137.53..4143.02 input=1m (60 bars) volume=699645.9999994023 va=0.7
  poc=4137.549999999999 (row 0) val=4137.53 (row 0) vah=4139.009999999999 (row 36) histogram rows in payload: 138
* `vp-CW`: http 404 -> {"error": "no profile computed for this window yet", "window": "CW"}
* `vp-PW-64`: window=PW rows=64 row_height=2.7600000000000002 row_mode=rows
  range 4110.93..4287.25 input=5m (1428 bars) volume=17577744099.00129 va=0.7
  poc=4156.47 (row 16) val=4135.77 (row 9) vah=4185.450000000001 (row 26) histogram rows in payload: 64
* `vp-6h`: window=CUSTOM rows=129 row_height=0.35000000000000003 row_mode=rows
  range 4123.34..4168.47 input=1m (355 bars) volume=921674447.000986 va=0.7
  poc=4158.165 (row 99) val=4150.29 (row 77) vah=4166.39 (row 122) histogram rows in payload: 129
* `vp-24h`: window=CUSTOM rows=128 row_height=0.37 row_mode=rows
  range 4123.34..4170.38 input=1m (1102 bars) volume=2225348733.999372 va=0.7
  poc=4158.305 (row 94) val=4144.43 (row 57) vah=4169.22 (row 123) histogram rows in payload: 128
* `vp-bad`: http 404 -> {"error": "no profile computed for this window yet", "window": "NOPE"}
* `vp-bad-range`: http 400 -> {"error": "start and end must both be epoch milliseconds"}

## WebSocket sample

* frames: {'probe': 1, 'heartbeat': 2, 'levels': 3, 'candle': 170, 'tick_volume': 156, 'calendar': 1, 'status': 1, 'probe-end': 1}
* **live `candle` frames: 170, deltas (min) [(0.0, 155), (15.0, 14)] -> no bars**
* first frame: {"time": 1791200700000, "open": 4154.39, "high": 4156.6, "low": 4151.09, "close": 4156.06, "volume": 36409807.0, "source": "sifting"}
* live `tick_volume` deltas (min): [(0.0, 155)] -> no bars
* `levels` frames carrying `meta`: **True**

## Verdict

* the deployed engine serves **15m** chart candles (2062 bars, 2026-09-13 02:45 UTC -> 2026-10-05 15:15 UTC)
* `/status` reports the chart edge: 2062 bars, last 2026-10-05 15:15 UTC, **11 min behind now**
