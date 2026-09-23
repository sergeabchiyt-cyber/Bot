# XAUUSD OrderFlow Engine

Rust backend for a gold (XAUUSD) order-flow desk: it ingests live trades from
multiple venues, detects order-flow events (bubbles / absorption), computes
weekly volume-profile levels, scrapes the economic calendar through a browser
MCP server, and serves everything — chart included — on one page.

## Run

```bash
cargo run            # or: docker build -t engine . && docker run -p 10000:10000 engine
```

Then open **http://localhost:3000/** — the dashboard is served by the engine
itself (no separate frontend process).

## HTTP endpoints

| Route      | What                                                |
|------------|-----------------------------------------------------|
| `/`        | Dashboard (candles, levels, bubbles, trades, calendar, feed status) |
| `/health`  | `ok`                                                |
| `/status`  | JSON snapshot of every feed's liveness              |
| `/levels`  | Current PW/PS/CW PoC/VaH/VaL levels                 |
| `/candles` | Recent 15m candles (Binance source)                 |
| `/ws`      | WebSocket stream — send `{"type":"subscribe","topics":["candle","levels","bubbles","trades","calendar","status"]}` |

WebSocket frames are tagged with `type`: `candle`, `levels`, `bubbles`,
`trades`, `calendar`, `status`, `heartbeat`. The four order-flow colors:
`BUY_BUBBLE` green, `SELL_BUBBLE` red, `ABS_BUY` blue, `ABS_SELL` orange.

## Data sources

| Feed    | What it provides | Needs |
|---------|------------------|-------|
| `binance` | XAUUSDT perp aggTrade (order flow) + 15m klines (chart / volume profile) | — |
| `bybit`   | XAUUSDT linear trades (order flow) | — |
| `okx`     | XAU-USDT-SWAP trades (order flow) | — |
| `bitget`  | XAUTUSDT trades (order flow) | — |
| `gate`    | XAUT_USDT futures trades (order flow) | — |
| `kraken`  | PF_XAUTUSD trade feed (order flow) | — |
| `alltick` | Spot XAUUSD ticks | `ALLTICK_TOKEN` |
| `itick`   | Spot XAUUSD ticks | `ITICK_TOKEN` |
| `sifting` | Spot XAUUSD chart candles + 15m history (cold start) | `SIFTING_API_KEY` |

Every feed auto-reconnects with status broadcasting; feeds without taker-side
data count toward volume but never toward delta.

## Environment variables (all optional)

```
PORT=3000
BINANCE_SYMBOL=XAUUSDT
SIFTING_API_KEY=            SIFTING_SYMBOL=XAUUSD
SIFTING_WS_URL=             SIFTING_HIST_URL=https://api.sifting.io
FEED_BINANCE=on|off|auto    FEED_BYBIT=on|off|auto      FEED_OKX=on|off|auto
FEED_BITGET=on|off|auto     FEED_GATE=on|off|auto       FEED_KRAKEN=on|off|auto
FEED_ALLTICK=on|off|auto    FEED_ITICK=on|off|auto      FEED_SIFTING=on|off|auto
BITGET_SYMBOL=XAUTUSDT      GATE_SYMBOL=XAUT_USDT       KRAKEN_PRODUCT=PF_XAUTUSD
ALLTICK_TOKEN=              ALLTICK_WS_URL=             ALLTICK_CODE=XAUUSD
ITICK_TOKEN=                ITICK_WS_URL=               ITICK_SYMBOL=XAUUSD
MCP_BROWSER_URL=http://localhost:3001     MCP_BROWSER_TOKEN=
MCP_BROWSER_TOOL_NAVIGATE=  MCP_BROWSER_TOOL_READ=      MCP_SCRAPE_SECS=900
LEVEL_PROXIMITY_PIPS=5      SL_MIN_PIPS=10  SL_MAX_PIPS=50  TP_MIN_PIPS=15  TP_MAX_PIPS=100
RR_MIN=1 RR_MAX=3           DERIV_DEMO_API=  DERIV_APP_ID=  DERIV_API_URL=
MCP_CHELSEA_URL=            (set to route orders through the ChelseaAI MCP tool)
```

`auto` (default) = on for keyless public feeds, on for tokened feeds only when
the token exists.

## Browser MCP

The engine speaks the MCP Streamable-HTTP transport (`initialize` →
`notifications/initialized` → `tools/list` → `tools/call`, `Mcp-Session-Id`
echo, JSON **and** SSE responses). Point `MCP_BROWSER_URL` at any browser MCP
server, e.g. Playwright MCP in HTTP mode:

```bash
npx @playwright/mcp@latest --transport streamable-http --port 3001
```

Tool names are discovered via `tools/list` (`browser_navigate` +
`browser_snapshot` for Playwright MCP); override with
`MCP_BROWSER_TOOL_NAVIGATE` / `MCP_BROWSER_TOOL_READ` if your server differs.
