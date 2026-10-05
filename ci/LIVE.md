# Live engine probe

generated: 2026-10-05T15:15:26Z

## /status

* version `0.3.0` venue `DerivDemo`
* feeds connected: 7 / 12
* **candles: interval='15m' seed_bars=2000**
* volume profile: row_mode=rows rows=128 bin=0.5 va=70.0 input=tv

## /candles

* **2000 bars**, 2026-09-13 02:45 UTC -> 2026-10-04 23:45 UTC
* deltas (min): [(15.0, 1986), (195.0, 4), (45.0, 2), (165.0, 2), (105.0, 1)]  -> **15m**
* open==close bars: 486 (24%) in 51 runs; longest 55 bars
* price range: 4111.09 .. 4399.60
* newest bar is **930 minutes behind now** -- the REST history seed lags the live stream

## /levels

* windows: PW, PS, SWING_BULL
* `meta` present: **True** (the branch adds it)

| window | poc | vah | val | start | end |
|---|---|---|---|---|---|
| PW | 4155.780000000001 | 4182.6900000000005 | 4134.39 | 2026-09-27 22:00 UTC | 2026-10-02 22:00 UTC |
| PS | 4137.549999999999 | 4139.009999999999 | 4137.53 | 2026-10-03 22:00 UTC | 2026-10-04 22:00 UTC |
| SWING_BULL | 4141.99 | 4144.929999999999 | 4138.929999999999 | 2026-10-04 22:00 UTC | 2026-10-04 23:00 UTC |

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
* `vp-6h`: the deployed build predates `?start=&end=` (answered with window=PW) — deploy the branch to check whether SiftingIO's history covers the last hours
* `vp-24h`: the deployed build predates `?start=&end=` (answered with window=PW) — deploy the branch to check whether SiftingIO's history covers the last hours
* `vp-bad`: http 404 -> {"error": "no profile computed for this window yet", "window": "NOPE"}
* `vp-bad-range`: window=PW rows=128 row_height=1.3800000000000001 row_mode=rows
  range 4110.93..4287.25 input=5m (1428 bars) volume=17577744099.00134 va=0.7
  poc=4155.780000000001 (row 32) val=4134.39 (row 17) vah=4182.6900000000005 (row 51) histogram rows in payload: 128

## WebSocket sample

* frames: {'probe': 1, 'heartbeat': 2, 'levels': 3, 'candle': 262, 'tick_volume': 247, 'calendar': 1, 'status': 1}
* **live `candle` frames: 262, deltas (min) [(0.0, 245), (15.0, 15), (915.0, 1)] -> no bars**
* first frame: {"time": 1791144900000, "open": 4137.53, "high": 4138.24, "low": 4137.53, "close": 4137.53, "volume": 4766.0, "source": "sifting"}
* live `tick_volume` deltas (min): [(0.0, 245), (15.0, 1)] -> no bars
* `levels` frames carrying `meta`: **True**

## Verdict

* the deployed engine serves **15m** chart candles (2000 bars, 2026-09-13 02:45 UTC -> 2026-10-04 23:45 UTC)
