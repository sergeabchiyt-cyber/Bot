// main.js — dashboard bootstrap and wiring.
//
// Read-only by construction:
//   - REST: GET-only resources from Node 1 / Node 4 public endpoints
//   - WS: subscribe-only frames; no writes, no tokens
//   - Node 4 operator endpoints (control/bridge) are never referenced
//     anywhere in the browser bundle

import { resolveEndpoints } from "./config.js";
import { createStore, candleBars } from "./state/store.js";
import { safeJsonParse } from "./lib/safejson.js";
import { createReconnectingSocket } from "./ws/socket.js";
import { node1SubscribeMessage, node4SubscribeMessage } from "./ws/topics.js";
import { createPollingFallback } from "./ws/polling.js";
import { seedNode1, fetchNode1Resource } from "./api/node1.js";
import { seedNode4, fetchNode4Resource } from "./api/node4.js";
import { classifyCandles } from "./lib/freshness.js";
import { h, clear } from "./render/dom.js";
import { layoutClass } from "./render/layout.js";
import { drawChart } from "./render/chart.js";
import {
  renderMarketHeader, renderLevelsPanel, renderTickVolumePanel, renderCalendarPanel,
} from "./render/market.js";
import {
  renderStrategyPanel, renderDemoStrip, renderPositionCard,
  renderTradeHistory, renderPerformance,
} from "./render/strategy.js";
import {
  renderMt5Venue, renderBridgeStatus, renderDiagnostics, renderOpenTrades,
  renderMt5Positions, renderMt5History, renderEventLog,
} from "./render/execution.js";

const runtimeConfig = (typeof window !== "undefined" && window.__RUNTIME_CONFIG__) || {};
const endpoints = resolveEndpoints(runtimeConfig, (typeof location !== "undefined" ? location.search : ""));

const store = createStore();
const sockets = [];

// ---------------------------------------------------------------- shell -----

function buildShell() {
  const app = document.getElementById("app");
  clear(app);

  const headerHost = h("div");
  const delayedHost = h("div", { class: "topbar__delayed" });
  const chartHost = h("div", { class: "chart-panel" },
    h("h2", { class: "panel__title" }, "Price · 15m (Node 1)"),
    h("canvas", { id: "chart", class: "chart" }),
    h("div", { class: "chart__legend" },
      h("span", { class: "legend" }, h("i", { class: "dot dot--up" }), "up"),
      h("span", { class: "legend" }, h("i", { class: "dot dot--down" }), "down"),
      h("span", { class: "legend" }, h("i", { class: "dot dot--poc" }), "POC/VAH/VAL"),
      h("span", { class: "legend" }, h("i", { class: "dot dot--pos" }), "entry/SL/TP"),
    ),
  );

  const node1Panels = h("div", { class: "panels panels--node1" });
  const node4Panels = h("div", { class: "panels panels--node4" });

  const footer = h("footer", { class: "footer" },
    h("div", { class: "footer__note" },
      h("span", { class: "demo-badge" }, "DEMO"),
      "Read-only dashboard. Market data from Node 1; execution state from Node 4 (broker-authoritative). "
      + "No trading decisions, no order submission. Node 4 operator endpoints (control/bridge) "
      + "are never called from the browser.",
    ),
    h("div", { class: "footer__endpoints", id: "endpoints" }, ""),
  );

  const layout = h("main", { class: layoutClass(window.innerWidth) },
    h("section", { class: "layout__chart" }, chartHost),
    node1Panels,
    node4Panels,
  );

  app.appendChild(headerHost);
  app.appendChild(delayedHost);
  app.appendChild(layout);
  app.appendChild(footer);

  return { headerHost, delayedHost, layout, node1Panels, node4Panels, chartHost };
}

// --------------------------------------------------------------- rendering -

const shell = buildShell();

function renderHeader() {
  const header = renderMarketHeader(store);
  clear(shell.headerHost);
  shell.headerHost.appendChild(header);
}

function renderDelayed() {
  clear(shell.delayedHost);
  const n1 = store.state.conn.node1.state;
  const n4 = store.state.conn.node4.state;
  if (n1 === "reconnecting" || n1 === "closed") {
    shell.delayedHost.appendChild(h("div", { class: "delayed delayed--warn" },
      "Node 1 feed down — polling /strategy + /demo every 5s (delayed)."));
  }
  if (n4 === "reconnecting" || n4 === "closed") {
    shell.delayedHost.appendChild(h("div", { class: "delayed delayed--warn" },
      "Node 4 feed down — polling public resources every 5s (delayed)."));
  }
}

function renderPanels() {
  clear(shell.node1Panels);
  shell.node1Panels.append(
    renderStrategyPanel(store),
    renderDemoStrip(store),
    renderPositionCard(store),
    renderTradeHistory(store),
    renderPerformance(store),
    renderLevelsPanel(store),
    renderTickVolumePanel(store),
    renderCalendarPanel(store),
  );

  clear(shell.node4Panels);
  const mt5 = store.state.node4.mt5;
  shell.node4Panels.append(
    h("section", { class: "panel panel--node4" },
      h("h2", { class: "panel__title" }, "Execution · Node 4"),
      renderMt5Venue(mt5.account),
      renderBridgeStatus(mt5.status),
    ),
    h("section", { class: "panel panel--node4" },
      h("h2", { class: "panel__title" }, "Diagnostics"),
      renderDiagnostics(store.state.node4.diagnostics),
    ),
    renderOpenTrades(store.state.node4.openTrades),
    renderMt5Positions(mt5.positions),
    renderMt5History(mt5.history),
    renderEventLog(store.state.node4.events),
  );

  renderHeader();
  renderDelayed();
  renderFooter();
}

function renderFooter() {
  const host = document.getElementById("endpoints");
  if (!host) return;
  const st = store.state;
  const errors = Object.entries(st.errors);
  const chips = [];
  chips.push(h("span", { class: `chip ${st.seeds.node1 ? "chip--good" : "chip--muted"}` },
    `Node 1 REST ${st.seeds.node1 ? "seeded" : "…"}`));
  chips.push(h("span", { class: `chip ${st.seeds.node4 ? "chip--good" : "chip--muted"}` },
    `Node 4 REST ${st.seeds.node4 ? "seeded" : "…"}`));
  const s = st.market.status;
  if (s && s.version) {
    chips.push(h("span", { class: "chip" },
      `engine ${s.version} · ${s.candles.bars ?? 0} bars · edge lag ${s.candles.edgeLagMinutes ?? "—"}min`));
  }
  for (const [ep, msg] of errors) {
    chips.push(h("span", { class: "chip chip--bad", title: ep }, `${ep}: ${msg}`));
  }
  clear(host);
  host.append(...chips);
}

// --------------------------------------------------------------- chart -------

function renderChart() {
  const canvas = document.getElementById("chart");
  if (!canvas) return;
  const bars = candleBars(store);
  const status = store.state.market.status;
  const candleFresh = classifyCandles({ status, candles: bars, now: Date.now() });
  drawChart(canvas, {
    bars,
    position: store.state.demo && store.state.demo.position,
    levels: store.state.market.levels,
    atr: store.state.strategy.last ? store.state.strategy.last.atr : null,
    freshnessState: candleFresh.state,
    interval: status && status.candles.interval,
    lastPrice: bars.length ? bars[bars.length - 1].close : null,
  });
}

// --------------------------------------------------------------- wiring ------

// REST bootstrap seeds.
(async () => {
  try {
    const payload1 = await seedNode1(endpoints.node1Rest, {
      onEndpointError: (ep, msg) => store.setEndpointError(ep, msg),
      clearEndpointError: (ep) => store.clearEndpointError(ep),
    });
    store.seedNode1Rest(payload1);
  } catch (err) {
    store.setEndpointError("node1:bootstrap", String(err));
  }
  try {
    const payload4 = await seedNode4(endpoints.node4Rest, {
      onEndpointError: (ep, msg) => store.setEndpointError(ep, msg),
      clearEndpointError: (ep) => store.clearEndpointError(ep),
    });
    store.seedNode4Rest(payload4);
  } catch (err) {
    store.setEndpointError("node4:bootstrap", String(err));
  }
})();

// WebSocket feeds.
const node1Socket = createReconnectingSocket({
  url: endpoints.node1Ws,
  subscribeMessage: node1SubscribeMessage(),
  heartbeatMs: 30000,
  WebSocketImpl: window.WebSocket,
  onMessage: (data) => {
    const parsed = safeJsonParse(data);
    if (parsed.ok) store.applyNode1Frame(parsed.data);
  },
  onStateChange: (state) => store.setConn("node1", { state }),
});
sockets.push(node1Socket);

const node4Socket = createReconnectingSocket({
  url: endpoints.node4Ws,
  subscribeMessage: node4SubscribeMessage(),
  heartbeatMs: 25000,
  WebSocketImpl: window.WebSocket,
  onMessage: (data) => {
    const parsed = safeJsonParse(data);
    if (parsed.ok) store.applyNode4Frame(parsed.data);
  },
  onStateChange: (state) => store.setConn("node4", { state }),
});
sockets.push(node4Socket);

// Polling fallbacks (REST, GET-only) while the sockets are down.
const node1Polling = createPollingFallback({
  poll: async () => {
    const [strategy, demo] = await Promise.all([
      fetchNode1Resource(endpoints.node1Rest, "/strategy"),
      fetchNode1Resource(endpoints.node1Rest, "/demo"),
    ]);
    const payload = {};
    if (strategy.ok && strategy.data != null) payload.strategy = strategy.data;
    if (demo.ok && demo.data != null) payload.demo = demo.data;
    if (Object.keys(payload).length) store.seedNode1Rest(payload);
  },
});
const node4Polling = createPollingFallback({
  poll: async () => {
    const [diagnostics, openTrades] = await Promise.all([
      fetchNode4Resource(endpoints.node4Rest, "/diagnostics"),
      fetchNode4Resource(endpoints.node4Rest, "/open-trades"),
    ]);
    const payload = {};
    if (diagnostics.ok && diagnostics.data != null) payload.diagnostics = diagnostics.data;
    if (openTrades.ok && openTrades.data != null) payload.open_trades = openTrades.data;
    if (Object.keys(payload).length) store.seedNode4Rest(payload);
  },
});

function syncPolling() {
  const n1 = store.state.conn.node1.state;
  const n4 = store.state.conn.node4.state;
  if (n1 === "reconnecting" || n1 === "closed") node1Polling.start();
  else node1Polling.stop();
  if (n4 === "reconnecting" || n4 === "closed") node4Polling.start();
  else node4Polling.stop();
}

// Render loop: store changes re-render panels (throttled — candle streams can
// arrive several times per second); a 1s tick refreshes freshness badges and
// the chart overlay.
let panelQueued = false;
store.subscribe(() => {
  if (panelQueued) return;
  panelQueued = true;
  setTimeout(() => {
    panelQueued = false;
    renderPanels();
    renderChart();
  }, 250);
});
store.subscribe(syncPolling);
setInterval(() => {
  renderHeader();
  renderDelayed();
  renderFooter();
  renderChart();
}, 1000);

let resizeTimer = null;
window.addEventListener("resize", () => {
  if (resizeTimer) clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    const layout = document.querySelector("main.layout");
    if (layout) {
      layout.className = layoutClass(window.innerWidth);
    }
    renderChart();
  }, 150);
});

window.addEventListener("beforeunload", () => {
  for (const s of sockets) s.close();
});

renderPanels();
renderChart();
