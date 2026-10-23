# Live engine probe

generated: 2026-10-23T18:58:41Z

## /status

* version `0.3.0` service `market-data`
* feeds connected: 8 / 12
* **candles: interval='15m' seed_bars=2000**
* history edge: 2026-10-23 18:30 UTC (**27 min behind now**) over 2000 bars
* volume profile: row_mode=rows rows=128 bin=0.5 va=70.0 input=tv

## /candles

* **2000 bars**, 2026-10-02 00:45 UTC -> 2026-10-23 18:30 UTC
* deltas (min): [(15.0, 1987), (60.0, 2), (30.0, 2), (165.0, 2), (135.0, 1)]  -> **15m**
* open==close bars: 385 (19%) in 64 runs; longest 65 bars
* price range: 3975.87 .. 4225.10
* newest bar is **29 minutes behind now**

## /levels

* windows: PW, PS, CW, SWING_BEAR
* `meta` present: **True** (the branch adds it)

| window | poc | vah | val | start | end |
|---|---|---|---|---|---|
| PW | 4156.695 | 4183.25 | 4141.44 | 2026-10-11 22:00 UTC | 2026-10-16 22:00 UTC |
| PS | 4091.69 | 4109.45 | 4021.13 | 2026-10-21 22:00 UTC | 2026-10-22 22:00 UTC |
| CW | 4121.545 | 4167.43 | 4104.92 | 2026-10-18 22:00 UTC | 2026-10-22 22:00 UTC |
| SWING_BEAR | 3996.7400000000002 | 4000.16 | 3990.92 | 2026-10-23 16:45 UTC | 2026-10-23 17:45 UTC |

## /vp audit

* `vp-PW`: window=PW rows=128 row_height=1.1300000000000001 row_mode=rows
  range 4073.64..4217.33 input=5m (1438 bars) volume=3012721.0000002873 va=0.7
  poc=4156.695 (row 73) val=4141.44 (row 60) vah=4183.25 (row 96) histogram rows in payload: 128
* `vp-PS`: window=PS rows=128 row_height=0.96 row_mode=rows
  range 4021.13..4143.05 input=1m (1440 bars) volume=460045.0000000139 va=0.7
  poc=4091.69 (row 73) val=4021.13 (row 0) vah=4109.45 (row 91) histogram rows in payload: 128
* `vp-CW`: window=CW rows=128 row_height=1.33 row_mode=rows
  range 4021.13..4190.22 input=5m (1152 bars) volume=2105749.999999891 va=0.7
  poc=4121.545 (row 75) val=4104.92 (row 63) vah=4167.43 (row 109) histogram rows in payload: 128
* `vp-PW-64`: window=PW rows=64 row_height=2.25 row_mode=rows
  range 4073.64..4217.33 input=5m (1438 bars) volume=3012721.0000000014 va=0.7
  poc=4158.014999999999 (row 37) val=4141.139999999999 (row 30) vah=4183.889999999999 (row 48) histogram rows in payload: 64
* `vp-6h`: window=CUSTOM rows=128 row_height=0.45 row_mode=rows
  range 3975.87..4033.33 input=1m (359 bars) volume=156339.99999993568 va=0.7
  poc=3999.0449999999996 (row 51) val=3993.42 (row 39) vah=4004.6699999999996 (row 63) histogram rows in payload: 128
* `vp-24h`: window=CUSTOM rows=128 row_height=0.45 row_mode=rows
  range 3975.87..4033.33 input=1m (1439 bars) volume=536032.9999997823 va=0.7
  poc=4024.245 (row 107) val=4009.62 (row 75) vah=4032.12 (row 124) histogram rows in payload: 128
* `vp-bad`: http 404 -> {"error": "no profile computed for this window yet", "window": "NOPE"}
* `vp-bad-range`: http 400 -> {"error": "start and end must both be epoch milliseconds"}

## WebSocket sample

* frames: {'probe': 1, 'heartbeat': 2, 'levels': 4, 'candle': 203, 'tick_volume': 1706, 'calendar': 1, 'status': 1}
* **live `candle` frames: 203, deltas (min) [(0.0, 187), (15.0, 15)] -> no bars**
* first frame: {"time": 1792767600000, "open": 4000.28, "high": 4003.54, "low": 3993.08, "close": 3995.92, "volume": 6329.0, "source": "sifting"}
* live `tick_volume` deltas (min): [(15.0, 1508), (0.0, 187), (30.0, 2), (165.0, 2), (90.0, 1)] -> 15m
* `levels` frames carrying `meta`: **True**

## Verdict

* the deployed engine serves **15m** chart candles (2000 bars, 2026-10-02 00:45 UTC -> 2026-10-23 18:30 UTC)
* `/status` reports the chart edge: 2000 bars, last 2026-10-23 18:30 UTC, **27 min behind now**
