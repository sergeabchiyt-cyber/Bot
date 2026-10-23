// strategy.js — Node 1 strategy panel + demo account UI.
// Read-only: shows decisions Node 1 publishes; the dashboard makes no trading
// decisions and submits nothing. The strategy panel degrades to an
// "unavailable" state when the engine does not serve /strategy (the deployed
// engine currently 404s it — recorded by Node 1's own ci/live logs).

import { h, emptyState } from "./dom.js";
import { fmtPrice, fmtMoney, fmtLots, fmtPercent, fmtR, fmtConfidence, fmtTime, fmtCount } from "../lib/format.js";

const ACTION_TONE = { buy: "good", sell: "bad", flat: "muted", unknown: "muted" };

export function renderStrategyPanel(store, { now = () => Date.now() } = {}) {
  const errors = store.state.errors || {};
  const unavailable = errors["node1:strategy"];
  const last = store.state.strategy.last;

  if (!last) {
    return h("section", { class: "panel" },
      h("h2", { class: "panel__title" }, "Strategy"),
      unavailable
        ? emptyState("Strategy endpoint unavailable on this engine — panel disabled.", { tone: "warn" })
        : emptyState("Waiting for the first strategy decision…"),
    );
  }

  const confidence = last.confidence;
  const score = last.score;
  const confidencePct = confidence == null ? 0 : Math.min(100, Math.max(0, confidence));
  const scorePct = score == null ? 0 : Math.min(100, Math.max(0, score));

  const reasons = (last.reasons || [])
    .slice()
    .sort((a, b) => Math.abs(b.contribution ?? 0) - Math.abs(a.contribution ?? 0))
    .slice(0, 6);

  const nextEvent = h("div", { class: "strategy__next" },
    h("span", { class: "chip" }, "next event"),
    h("strong", {}, fmtTime(last.nextEventTs)),
  );

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Strategy"),
    h("div", { class: "strategy__head" },
      h("span", {
        class: `action-badge action-badge--${ACTION_TONE[last.action] || "muted"}`,
        title: `Action: ${last.action} · confidence ${fmtConfidence(confidence)}`,
      }, last.action),
      h("span", { class: "chip" }, fmtConfidence(confidence)),
      h("span", { class: "chip" }, last.regime || "regime —"),
      h("span", { class: "chip" }, last.volatility || "volatility —"),
      nextEvent,
    ),
    h("div", { class: "strategy__metrics" },
      h("div", { class: "metric" },
        h("span", { class: "metric__label" }, "Price / ATR"),
        h("span", { class: "metric__value" }, `${fmtPrice(last.price)} / ${fmtPrice(last.atr)}`)),
      h("div", { class: "metric" },
        h("span", { class: "metric__label" }, "Lots · Risk"),
        h("span", { class: "metric__value" }, `${fmtLots(last.lots)} · ${fmtMoney(last.riskUsd)}`)),
      h("div", { class: "metric" },
        h("span", { class: "metric__label" }, "SL / TP"),
        h("span", { class: "metric__value" }, `${fmtPrice(last.stopLoss)} / ${fmtPrice(last.takeProfit)}`)),
      h("div", { class: "metric" },
        h("span", { class: "metric__label" }, "Reward:Risk"),
        h("span", { class: "metric__value" }, fmtR(last.rewardRisk))),
    ),
    h("div", { class: "strategy__bars" },
      h("div", { class: "bar-row" },
        h("div", { class: "bar-label" }, "Confidence"),
        h("div", { class: "bar-track" },
          h("div", { class: "bar-fill bar-fill--good", style: `width:${confidencePct}%` }),
          // 60% threshold tick
          h("div", { class: "bar-tick", style: "left:60%", title: "60% threshold" })),
        h("span", { class: "bar-value" }, fmtConfidence(confidence))),
      h("div", { class: "bar-row" },
        h("div", { class: "bar-label" }, "Score"),
        h("div", { class: "bar-track" },
          h("div", { class: "bar-fill", style: `width:${scorePct}%` })),
        h("span", { class: "bar-value" }, score == null ? "—" : String(score))),
    ),
    h("div", { class: "strategy__reasons" },
      h("h3", { class: "panel__subtitle" }, "Reasons (by |contribution|)"),
      reasons.length === 0
        ? h("p", { class: "muted" }, "No reason breakdown in this decision.")
        : h("ul", { class: "reason-list" }, reasons.map((r) => h("li", {
            class: "reason",
            title: r.detail || undefined,
          },
          h("span", { class: "reason__name" }, r.name || "—"),
          h("span", { class: "reason__value" }, r.value ?? "—"),
          h("span", { class: "reason__contrib" },
            `weight ${r.weight ?? "—"} · contrib ${r.contribution ?? "—"}`)))),
    ),
    last.notes && last.notes.length
      ? h("ul", { class: "notes" }, last.notes.map((n) => h("li", {}, n)))
      : null,
  );
}

// Demo account strip: DEMO 1:1000 badge, balance/equity/free margin,
// margin level, open PnL.
export function renderDemoStrip(store) {
  const snap = store.state.demo;
  if (!snap || !snap.ok) {
    return h("section", { class: "panel" },
      h("h2", { class: "panel__title" }, "Demo Account"),
      emptyState("Demo engine snapshot unavailable on this host.", { tone: "warn" }));
  }
  const acc = snap.account;
  const demoBadge = h("span", {
    class: "demo-badge demo-badge--strong",
    title: "Simulated demo account — not real money, not connected to any broker.",
  }, `DEMO ${snap.leverage ?? ""}`.trim());

  const metrics = [
    ["Balance", fmtMoney(acc.balance), acc.currency],
    ["Equity", fmtMoney(acc.equity), acc.currency],
    ["Free margin", fmtMoney(acc.freeMargin), acc.currency],
    ["Margin level", fmtPercent(acc.marginLevel), ""],
    ["Open PnL", fmtMoney(acc.unrealizedPnl, { signed: true }), acc.currency],
  ];

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Demo Account"),
    h("div", { class: "demo__head" },
      demoBadge,
      h("span", { class: "chip" }, `${snap.symbol || "—"} · spread ${snap.quote.spread ?? "—"}`),
      h("span", { class: "chip" }, fmtTime(snap.ts)),
    ),
    h("div", { class: "metrics-row" }, metrics.map(([label, value, cur]) => h("div", { class: "metric" },
      h("span", { class: "metric__label" }, label),
      h("span", { class: "metric__value" }, cur ? `${value} ${cur}` : value)))),
    snap.note ? h("p", { class: "muted demo__note" }, snap.note) : null,
  );
}

// Open demo position card with entry→TP progress bar + stop marker.
export function renderPositionCard(store) {
  const snap = store.state.demo;
  const position = snap && snap.position;
  if (!position) {
    return h("section", { class: "panel" },
      h("h2", { class: "panel__title" }, "Open Position"),
      emptyState("No open demo position right now.", { tone: "muted" }),
      h("p", { class: "muted" }, "The dashboard never opens positions — this reflects the Node 1 demo engine only."));
  }

  const sideTone = position.side === "buy" ? "good" : "bad";
  // Progress: fraction of the SL→TP span already traveled.
  let progress = null;
  if (position.entry != null && position.stopLoss != null && position.takeProfit != null) {
    const span = position.takeProfit - position.stopLoss;
    if (span !== 0) {
      progress = position.side === "buy"
        ? (position.entry - position.stopLoss) / span
        : (position.takeProfit - position.entry) / span;
    }
  }
  const pct = progress == null ? 0 : Math.min(100, Math.max(0, progress * 100));

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Open Position"),
    h("div", { class: "position__head" },
      h("span", { class: `action-badge action-badge--${sideTone}` }, position.side || "—"),
      h("span", { class: "chip" }, `id ${position.id ?? "—"}`),
      h("span", { class: "chip" }, `${fmtLots(position.lots)} lots`),
    ),
    h("div", { class: "metrics-row" },
      h("div", { class: "metric" },
        h("span", { class: "metric__label" }, "Entry"),
        h("span", { class: "metric__value" }, fmtPrice(position.entry))),
      h("div", { class: "metric" },
        h("span", { class: "metric__label" }, "Stop / TP"),
        h("span", { class: "metric__value" }, `${fmtPrice(position.stopLoss)} / ${fmtPrice(position.takeProfit)}`)),
      h("div", { class: "metric" },
        h("span", { class: "metric__label" }, "Unrealized PnL"),
        h("span", {
          class: `metric__value ${position.unrealizedPnl < 0 ? "neg" : "pos"}`,
        }, fmtMoney(position.unrealizedPnl, { signed: true }))),
    ),
    h("div", { class: "progress-track", title: "Entry → Take Profit; the stop marks the risked span" },
      h("div", { class: "progress-fill", style: `width:${pct}%` }),
      h("div", { class: "progress-stop", title: "Stop" }),
      h("span", { class: "progress-label" }, `${pct.toFixed(0)}% of span to TP`),
    ),
    position.reason ? h("p", { class: "muted" }, `Reason: ${position.reason}`) : null,
    h("p", { class: "muted" }, `Opened ${fmtTime(position.openedAt)} · risk ${fmtMoney(position.riskUsd)}`),
  );
}

// Trade history table (recent_trades).
export function renderTradeHistory(store) {
  const trades = store.state.demo ? store.state.demo.recentTrades : [];
  const rows = trades.slice(0, 12).map((t) => h("tr", {},
    h("td", {}, t.id ?? "—"),
    h("td", {}, t.side === "buy"
      ? h("span", { class: "chip chip--good" }, "BUY")
      : t.side === "sell"
        ? h("span", { class: "chip chip--bad" }, "SELL")
        : "—"),
    h("td", { class: "num" }, fmtLots(t.lots)),
    h("td", { class: "num" }, fmtPrice(t.entry)),
    h("td", { class: "num" }, fmtPrice(t.exit)),
    h("td", { class: `num ${t.pnl < 0 ? "neg" : "pos"}` }, fmtMoney(t.pnl, { signed: true })),
    h("td", { class: "num" }, fmtR(t.rMultiple)),
    h("td", {}, t.exitReason || "—"),
  ));

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Trade History"),
    trades.length === 0
      ? emptyState("No closed demo trades yet.")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "ID"), h("th", {}, "Side"), h("th", {}, "Lots"),
            h("th", {}, "Entry"), h("th", {}, "Exit"), h("th", {}, "PnL"),
            h("th", {}, "R"), h("th", {}, "Exit reason"))),
          h("tbody", {}, rows)),
  );
}

// Performance stats card.
export function renderPerformance(store) {
  const stats = store.state.demo && store.state.demo.stats;
  if (!stats) {
    return h("section", { class: "panel" },
      h("h2", { class: "panel__title" }, "Performance"),
      emptyState("No performance stats available.", { tone: "muted" }));
  }
  const metrics = [
    ["Closed", fmtCount(stats.closedTrades), ""],
    ["Win rate", fmtPercent(stats.winRate), ""],
    ["Net PnL", fmtMoney(stats.netPnl, { signed: true }), "USD"],
    ["Profit factor", stats.profitFactor == null ? "—" : String(stats.profitFactor), ""],
    ["Avg R", fmtR(stats.avgR), ""],
    ["Max DD", fmtPercent(stats.maxDrawdownPct), ""],
  ];
  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Performance"),
    h("div", { class: "metrics-row" }, metrics.map(([label, value, cur]) => h("div", { class: "metric" },
      h("span", { class: "metric__label" }, label),
      h("span", { class: "metric__value" }, cur ? `${value} ${cur}` : value)))),
  );
}
