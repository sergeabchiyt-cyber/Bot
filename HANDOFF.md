# Node2 integration handoff

Ported Node1 backend (`xauusd-engine`) onto this branch and wired the frontend to the `/ws` protocol.

- Dashboard served by the engine: `static/index.html` (must contain `ORDERFLOW` for CI grep).
- Modular Node2 UI (`index.html`, `chart.js`, `bubbles.js`, `style.css`) updated to the same protocol/colors.
- Chart candles prefer `source:"binance"`; Sifting is fallback only.
- Subscribe: `{"type":"subscribe","topics":["candle","levels","bubbles","trades","calendar","status"]}`
- Colors: BUY `#22c55e` SELL `#ef4444` ABS_BUY `#3b82f6` ABS_SELL `#f97316`; PW `#facc15` PS `#fb923c` CW `#22d3ee`.
