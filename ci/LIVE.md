# Live engine probe

generated: 2026-10-05T15:09:11Z

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
Traceback (most recent call last):
  File "/home/runner/work/Bot/Bot/ci/live_report.py", line 228, in <module>
    raise SystemExit(main())
                     ^^^^^^
  File "/home/runner/work/Bot/Bot/ci/live_report.py", line 117, in main
    age_min = (time.time() * 1000 - times[-1]) / 60_000
               ^^^^
NameError: name 'time' is not defined. Did you mean: 'times'? Or did you forget to import 'time'?
