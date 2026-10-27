# Node 2 service architecture

This document describes the current frontend wiring. The pre-change inventory
and route decisions are recorded in [`DASHBOARD_WIRING.md`](DASHBOARD_WIRING.md).

```
Node 1 ── market candles / tick volume / calendar / feed status ──┐
  │                                                              │
  └── public /ws: candle, tick_volume, calendar, status           │
                                                                  ▼
                                                             Node 2 browser
                                                                  ▲
Node 3 ── levels / drawings / VP / setups / signals / diagnostics ┘
  │
  └── public /ws: levels, drawing, vp, scanning, signals, diagnostics
```

Node 3 also consumes Node 1's public feed, but that service-to-service link is
outside this frontend. Node 4 is intentionally absent from the diagram: Node 2
never connects to it. Broker-authoritative fills appear in the UI only as
execution report metadata already published by Node 3's `/signals`.

## Configuration

`js/config.js` defines exactly two base URLs:

- `NODE1_HOST` — default `https://engine-southeastasia-sng-main.onrender.com`;
  `MARKET_HOST` is accepted as an alias.
- `NODE3_HOST` — default `https://strategy-api-vp-levels.onrender.com`.

A deployment may supply absolute HTTPS origins or same-origin proxy prefixes
before `config.js` loads. `js/client.js` derives both REST and WSS URLs from
these bases. No endpoint paths, credentials, or third backend origin appear in
configuration.

## Node 1 — C1 v2 market feed

REST:

| Path | Payload |
|---|---|
| `GET /candles?interval=1m\|15m\|1h` | `Candle[]`, ascending and unique by bucket-open epoch ms; `closed:false` is a forming bar |
| `GET /tick-volume` | `TickVolumeBar[]`; last bar can be in progress |
| `GET /calendar` | `{source,count,updated,events[]}` |
| `GET /status` | `{contract_version:"2.0",boot_id,freshness:{state,feed,age_ms,last_tick_ms,market_open},feeds,candles,ws,session}` |

Socket `/ws` subscribes to exactly `["candle","tick_volume","calendar","status"]`.
The only outbound frames are `subscribe` and `heartbeat`. Replay frames have
`replay:true` and no sequence; live frames have a `seq`. A `gap` frame with
`data.dropped`, or a skipped `seq`, triggers a visible warning and a candle
refetch. Stale/offline/closed-market freshness is shown as a badge with a
reason; unknown frame types are ignored.

## Node 3 — published strategy state

REST:

| Path | Payload |
|---|---|
| `GET /health` | `{status,ok,contract_version,node1_connected,freshness_state,vp_ready,tradeable,contract_violations,intents_pending}` |
| `GET /levels` | `VpLevels[]` for PW/PS/CW; PS has PoC only, no `vah`/`val` |
| `GET /drawings` | `{timestamp,items[],pending[]}`; identity is `id` + `timeframe` |
| `GET /vp?window=PW\|PS\|CW[&rows=N]` or `?start=<ms>&end=<ms>[&rows=N]` | `VpAudit` with published histogram rows |
| `GET /scanning` | setup states, ATR/pips, SL/TP pips, RR, volume status, projected orders |
| `GET /signals` | pending/recent trade intents and execution report metadata |
| `GET /diagnostics` | `{work,scanning,signals,recent_events}` with violation count/text |

Socket `/ws` subscribes to exactly
`["levels","drawing","vp","scanning","signals","diagnostics"]`. Every replay or
live frame is a full replacement for that topic. Drawings are deduped by
`id` + `timeframe`; the same level on `1H` and `daily` is drawn separately.
Horizontal levels use the payload's `color` and `line_style`; Sunday-open is
dotted; daily rays start at `ray_from_time`; FRVP rectangles render the
published row `volume`, `up_volume`, and `down_volume`. The browser performs no
volume-profile maths.

`ProjectedOrder.sizing` is always `node4_managed`. SL/TP/RR are display-only and
labelled “Node 4 sizes this”. Node 2 never derives a position size.

## Failure and integrity behavior

- REST seeds and two sockets share the same parsers. Each panel can paint from
  either source.
- `/health` failures retain last-good data, show “Node 3 unreachable” with the
  last-good timestamp, and never blank the chart.
- Node 1 CORS/network failures produce an actionable origin warning.
- A disconnected socket uses a labelled, delayed REST poll; a `DELAYED` badge is
  shown.
- The connection banner reports Node 3 disconnection, `node1_connected:false`,
  stale/offline freshness, sequence gaps, and the diagnostics violation count
  and last text.
- Both contract versions are shown in the header. A mismatch is a visible
  warning.

## Build boundary

`npm run build` copies only `index.html`, `style.css`, and its referenced
scripts into ignored `dist/`. It fails the build if output contains any
loopback URL, private execution route, retired endpoint, write request, or a
transport constructor outside `js/client.js`. No credentials are present in the
bundle.
