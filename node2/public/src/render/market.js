// market.js — Node 1 market-data panels: header, levels (VP clusters),
// tick volume, calendar.

import { h, freshnessPill, emptyState } from "./dom.js";
import { classifyCandles, classifySocket, feedLabel, feedLabel as feedText } from "../lib/freshness.js";
import { fmtPrice, fmtTime, fmtTimeShort, fmtCount } from "../lib/format.js";
import { candleBars } from "../state/store.js";

// Header strip: symbol, timeframe, last price, freshness, feed summary.
export function renderMarketHeader(store, { now = () => Date.now() } = {}) {
  const status = store.state.market.status;
  const bars = candleBars(store);
  const lastBar = bars.length ? bars[bars.length - 1] : null;
  const candleFresh = classifyCandles({ status, candles: bars, now: now() });
  const socketFresh = classifySocket({
    state: store.state.conn.node1.state,
    lastMsgTs: store.state.conn.node1.lastMsgTs,
    nowMs: now(),
    heartbeatMs: 30000,
  });

  const interval = status && status.candles.interval
    ? status.candles.interval
    : "—";
  const symbol = (store.state.strategy.last && store.state.strategy.last.symbol)
    || (store.state.demo && store.state.demo.symbol)
    || "XAUUSD";

  const feedChips = [];
  if (status && status.feeds) {
    for (const [name, feed] of Object.entries(status.feeds)) {
      feedChips.push(h("span", {
        class: `feed chip--${feed.state === "connected" ? "good" : feed.state === "error" ? "bad" : "muted"}`,
        title: `${feedText(feed.state, name)} · ${feed.msgs ?? 0} msgs · last ${fmtTime(feed.lastMsg)}`,
      }, `${name}: ${feed.state || "unknown"}`));
    }
  }

  return h("header", { class: "topbar" },
    h("div", { class: "topbar__brand" },
      h("span", { class: "topbar__logo" }, "N2"),
      h("div", {},
        h("div", { class: "topbar__title" }, "Node 2 · Demo Market Dashboard"),
        h("div", { class: "topbar__sub" },
          `${symbol} · ${interval} · last ${fmtPrice(lastBar ? lastBar.close : null)}`),
      ),
    ),
    h("div", { class: "topbar__status" },
      h("span", { class: "demo-badge" }, "DEMO"),
      h("span", { class: "chip" }, `candles ${fmtCount(bars.length)}`),
      freshnessPill(candleFresh.state, { reason: candleFresh.reason }),
      freshnessPill(socketFresh, { reason: `transport ${socketFresh}` }),
      h("div", { class: "topbar__feeds" }, feedChips),
    ),
  );
}

// Volume-profile cluster table.
export function renderLevelsPanel(store) {
  const levels = store.state.market.levels;
  const rows = levels.slice(-8).reverse().map((lv) => h("tr", {},
    h("td", {}, h("code", {}, lv.window || "—")),
    h("td", { class: "num" }, fmtPrice(lv.poc)),
    h("td", { class: "num" }, fmtPrice(lv.vah)),
    h("td", { class: "num" }, fmtPrice(lv.val)),
    h("td", {}, lv.direction === "up"
      ? h("span", { class: "chip chip--good" }, "▲ up")
      : lv.direction === "down"
        ? h("span", { class: "chip chip--bad" }, "▼ down")
        : h("span", { class: "chip" }, "—")),
    h("td", { class: "num" }, fmtTime(lv.timestamp)),
  ));

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Levels · POC/VAH/VAL"),
    levels.length === 0
      ? emptyState("Awaiting levels snapshot…")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "Window"), h("th", {}, "POC"), h("th", {}, "VAH"),
            h("th", {}, "VAL"), h("th", {}, "Dir"), h("th", {}, "Timestamp (UTC)"))),
          h("tbody", {}, rows)),
  );
}

// Tick-volume bars.
export function renderTickVolumePanel(store) {
  const order = store.state.market.tickBarsOrder;
  const bars = [];
  for (let i = order.length - 1; i >= 0 && bars.length < 8; i--) {
    const bar = store.state.market.tickBars.get(order[i]);
    if (bar) bars.push(bar);
  }
  const rows = bars.map((t) => h("tr", {},
    h("td", { class: "num" }, fmtTimeShort(t.time)),
    h("td", { class: "num" }, fmtCount(t.ticks)),
    h("td", { class: "num up" }, fmtCount(t.upTicks)),
    h("td", { class: "num down" }, fmtCount(t.downTicks)),
    h("td", { class: "num" }, fmtCount(t.flatTicks)),
    h("td", { class: "num" }, fmtPrice(t.close)),
    h("td", { class: "num" }, t.ticksPerSec == null ? "—" : `${t.ticksPerSec.toFixed(1)}/s`),
  ));

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Tick Volume"),
    bars.length === 0
      ? emptyState("Awaiting tick volume…")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "Time"), h("th", {}, "Ticks"), h("th", {}, "Up"),
            h("th", {}, "Down"), h("th", {}, "Flat"), h("th", {}, "Close"),
            h("th", {}, "Rate"))),
          h("tbody", {}, rows)),
  );
}

// Economic calendar: impact + gold-relevant events.
export function renderCalendarPanel(store) {
  const events = store.state.market.calendar || [];
  const impactTone = { High: "bad", Medium: "warn", Low: "muted" };
  const upcoming = events.slice(0, 10);
  const rows = upcoming.map((e) => h("tr", {},
    h("td", {}, h("span", {
      class: `chip ${impactTone[e.impact] ? `chip--${impactTone[e.impact]}` : ""}`,
    }, e.impact || "—")),
    h("td", {}, e.currency || "—"),
    h("td", {}, e.event || "—"),
    h("td", { class: "num" }, e.actual ?? "—"),
    h("td", { class: "num" }, fmtTimeShort(e.timestamp)),
    e.goldRelevant ? h("td", {}, h("span", { class: "chip chip--gold" }, "GOLD")) : h("td", {}),
  ));

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Calendar"),
    events.length === 0
      ? emptyState("No calendar events in this feed.")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "Impact"), h("th", {}, "CCY"), h("th", {}, "Event"),
            h("th", {}, "Actual"), h("th", {}, "Time"), h("th", {}, "Gold"))),
          h("tbody", {}, rows)),
  );
}
