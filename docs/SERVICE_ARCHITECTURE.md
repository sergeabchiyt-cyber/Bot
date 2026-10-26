# Node 2 dashboard service architecture

Node 2 remains a static browser frontend using the existing XAUUSD Terminal shell and Lightweight Charts presentation. `js/node2-client.js` centralizes service origins, GET transport, parsers and WebSocket URL derivation. Existing Node 1 chart/calendar/status panels consume the typed client through `js/api.js`; `js/node3-dashboard.js` owns Node 3 snapshots and its read-only subscription.

```text
Browser — original XAUUSD Terminal UI
  ├─ Node 1: GET candles, tick-volume, calendar, status
  │           WS candle, tick_volume, calendar, status
  └─ Node 3: GET health, levels, drawings, scanning, signals, diagnostics
              optional GET vp
              WS levels, drawing, vp, scanning, signals, diagnostics
```

## Boundaries

- HTTP requests are `GET` only. WebSocket messages are limited to `subscribe` and `heartbeat`.
- The active page has no Node 4 connection, execution endpoint, order entry, strategy calculations, volume-profile calculations, setup generation, or position sizing.
- Node 3 snapshots replace their topics. Drawing identity is the pair `id` + `timeframe`, retaining both hourly and daily drawings with the same ID.
- Node 1 history is replaced as a snapshot; duplicate candle timestamps collapse. A visible gap warning triggers a candle refetch.
- Failed reads preserve last-good state and surface service/freshness warnings.
- Sensitive-looking keys are stripped from contract payloads before retention. Backend text is escaped before insertion into panel HTML.

## Main files

| File | Responsibility |
|---|---|
| `index.html`, `style.css` | Existing terminal shell, panel styling, chart-library host and service labels |
| `js/node2-client.js` | Base URL validation, typed GET routes, parsers, WebSocket URL construction |
| `js/api.js`, `js/socket.js` | Typed Node 1 API facade and allowed Node 1 subscription |
| `js/chart.js`, `js/calendar.js`, `js/status.js`, `js/levels.js` | Existing terminal presentation modules, rewired to the new contracts |
| `js/node3-dashboard.js` | Node 3 snapshots/subscription, scanner, signals, diagnostics and drawings |
| `tests/fixtures/dashboard-contracts.json` | Contract-shaped Node 1 and Node 3 smoke fixtures |
| `docs/DASHBOARD_WIRING.md` | Before-state request inventory and migration findings |
