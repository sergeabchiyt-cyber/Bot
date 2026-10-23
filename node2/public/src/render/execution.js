// execution.js — Node 4 execution panel.
//
// Read-only view of broker-authoritative state. Rendering rules enforced here:
//   - lifecycle badges per lib/lifecycle.js; accepted/unknown are NEVER
//     shown as confirmed fills
//   - orders_unknown is surfaced as a counter, never presented as open/filled
//   - MT5 account state rules from FRONTEND_RESOURCES.md §3.3: configured,
//     connected, authorized, demo-account check, halted kill switch, heartbeat
//     staleness
//   - zero order controls; operator endpoints are never referenced

import { h, emptyState, lifecycleBadgeEl } from "./dom.js";
import { lifecycleBadge, isConfirmedFill } from "../lib/lifecycle.js";
import { fmtMoney, fmtPrice, fmtLots, fmtTime, fmtDuration, fmtCount } from "../lib/format.js";

function badge(label, tone) {
  return h("span", { class: `chip chip--${tone}` }, label);
}

// MT5 venue status block.
export function renderMt5Venue(account, bridge) {
  const chips = [];
  if (!account) {
    return h("div", { class: "venue" },
      badge("venue unknown", "muted"),
      emptyState("MT5 account snapshot not received.", { tone: "muted" }));
  }
  if (account.configured !== true) {
    chips.push(badge("venue OFF (not configured)", "muted"));
  } else {
    chips.push(badge(account.connected ? "venue connected" : "venue disconnected", account.connected ? "good" : "bad"));
    if (!account.connected && account.setupHint) {
      chips.push(badge("setup: " + account.setupHint, "warn"));
    }
    if (account.authorized !== true) {
      chips.push(badge("authorized=false · cannot trade", "warn"));
    }
    if (account.accountType && account.accountType !== "demo") {
      chips.push(badge(`account_type=${account.accountType} (not demo!)`, "bad"));
    } else {
      chips.push(badge("demo account", "good"));
    }
    if (account.halted === true) {
      chips.push(badge("KILL SWITCH ENGAGED", "bad"));
    }
    // Heartbeat staleness while connected (~15s).
    if (account.connected && account.lastHeartbeat != null) {
      const ageS = (Date.now() - account.lastHeartbeat) / 1000;
      if (ageS > 15) chips.push(badge(`heartbeat ${fmtDuration(ageS * 1000)} stale`, "warn"));
    }
    if (account.tradeAllowed === false) {
      chips.push(badge("trade_allowed=false", "warn"));
    }
  }
  if (account.error) chips.push(badge("error: " + account.error, "bad"));

  return h("div", { class: "venue" }, chips);
}

// Bridge status counters.
export function renderBridgeStatus(status) {
  if (!status) return emptyState("Bridge status not received.", { tone: "muted" });
  const counters = [
    ["orders sent", status.ordersSent],
    ["filled", status.ordersFilled],
    ["rejected", status.ordersRejected],
    ["unknown", status.ordersUnknown],
    ["open positions", status.positionsOpen],
    ["history deals", status.historyDeals],
  ];
  return h("div", { class: "bridge" },
    h("div", { class: "bridge__row" },
      h("span", { class: "chip" }, `protocol ${status.protocol ?? "—"}`),
      h("span", { class: "chip" }, `bridge ${status.bridgeVersion ?? "—"}`),
      h("span", { class: "chip" }, `EA ${status.eaConnected ? "connected" : "disconnected"} · write ${status.eaWriteEnabled ? "on" : "off"}`),
      h("span", { class: "chip" }, status.halted ? "HALTED" : "running"),
      status.lastError ? badge("last error: " + status.lastError, "bad") : null,
    ),
    h("div", { class: "metrics-row" }, counters.map(([label, value]) => h("div", { class: "metric" },
      h("span", { class: "metric__label" }, label),
      h("span", { class: "metric__value" }, fmtCount(value)))),
    h("p", { class: "muted" },
      `EA ${status.eaLogin ?? "—"} @ ${status.eaServer ?? "—"} · mode ${status.eaMode ?? "—"} · uptime ${fmtDuration(status.uptimeSecs ? status.uptimeSecs * 1000 : null)}`),
  ));
}

// Execution diagnostics (Node 3 link + work counters + ledger).
export function renderDiagnostics(diag) {
  if (!diag) return emptyState("Diagnostics not received.", { tone: "muted" });
  const work = diag.work;
  const workMetrics = [
    ["open trades", work.openTrades],
    ["positions", work.positions],
    ["fills attempted", work.fillsAttempted],
    ["fills confirmed", work.fillsConfirmed],
    ["fills rejected", work.fillsRejected],
    ["fills UNKNOWN", work.fillsUnknown],
    ["cancels sent", work.cancelsSent],
    ["cancels accepted", work.cancelsAccepted],
  ];
  const node3 = diag.node3;
  const node3Chips = [
    node3.degraded
      ? badge("node3 link DEGRADED", "bad")
      : badge("node3 link ok", "good"),
    node3.latencyMs != null ? h("span", { class: "chip" }, `latency ${node3.latencyMs}ms`) : null,
    node3.attempts != null ? h("span", { class: "chip" }, `attempts ${node3.attempts}`) : null,
    node3.errors.length
      ? badge(`${node3.errors.length} error(s): ${node3.errors.slice(-2).join(" | ")}`, "bad")
      : null,
  ];
  return h("div", {},
    h("div", { class: "bridge__row" }, node3Chips),
    h("div", { class: "metrics-row" }, workMetrics.map(([label, value]) => h("div", { class: "metric" },
      h("span", { class: "metric__label" }, label),
      h("span", {
        class: `metric__value ${label.includes("UNKNOWN") ? "neg" : ""}`,
        title: label.includes("UNKNOWN") ? "Outcomes the broker never confirmed — never shown as open/filled." : undefined,
      }, fmtCount(value))))),
    diag.ledger
      ? h("details", { class: "ledger" },
          h("summary", {}, "Node 3 ledger (raw)"),
          h("pre", { class: "ledger__json" }, JSON.stringify(diag.ledger, null, 2)))
      : null,
  );
}

// Open trades table. Each row's Status column uses a lifecycle badge; an
// "Accepted" or "Unknown" trade is never rendered as a confirmed fill.
export function renderOpenTrades(trades) {
  const rows = trades.slice(0, 20).map((t) => {
    const badgeInfo = lifecycleBadge(t.lifecycle);
    const confirmed = isConfirmedFill(t.lifecycle, t.filledVolume);
    return h("tr", {},
      h("td", {}, t.id ?? "—"),
      h("td", {}, t.side === "buy"
        ? h("span", { class: "chip chip--good" }, "BUY")
        : t.side === "sell"
          ? h("span", { class: "chip chip--bad" }, "SELL")
          : "—"),
      h("td", {}, t.symbol || "—"),
      h("td", { class: "num" }, fmtLots(t.lots)),
      h("td", { class: "num" }, fmtLots(t.filledVolume)),
      h("td", { class: "num" }, fmtPrice(t.entry)),
      h("td", { class: "num" }, fmtPrice(t.mark)),
      h("td", { class: `num ${t.unrealized < 0 ? "neg" : "pos"}` }, fmtMoney(t.unrealized, { signed: true })),
      h("td", {},
        lifecycleBadgeEl(badgeInfo),
        confirmed ? h("span", { class: "chip chip--confirmed", title: "Broker-confirmed fill" }, "confirmed") : null),
      h("td", {},
        t.mt5
          ? h("span", { class: `chip ${t.mt5.matched ? "chip--good" : "chip--muted"}` },
              t.mt5.matched ? `MT5 #${t.mt5.ticket ?? "?"}` : `MT5 ${t.mt5.status ?? "unmatched"}`)
          : h("span", { class: "chip" }, "deriv"),
        t.note ? h("div", { class: "muted trade__note" }, t.note) : null),
    );
  });

  const unknownCount = trades.filter((t) => t.lifecycle === "unknown").length;
  const acceptedCount = trades.filter((t) => t.lifecycle === "accepted").length;
  const confirmedCount = trades.filter((t) => isConfirmedFill(t.lifecycle, t.filledVolume)).length;

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Open Trades"),
    h("div", { class: "bridge__row" },
      h("span", { class: "chip" }, `${trades.length} total`),
      h("span", { class: "chip" }, `${confirmedCount} broker-confirmed fills`),
      acceptedCount ? badge(`${acceptedCount} accepted (pending broker)`, "warn") : null,
      unknownCount
        ? badge(`${unknownCount} unknown — NOT confirmed, never resent`, "bad")
        : null,
    ),
    trades.length === 0
      ? emptyState("No open trades on Node 4.")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "ID"), h("th", {}, "Side"), h("th", {}, "Symbol"),
            h("th", {}, "Lots"), h("th", {}, "Filled"), h("th", {}, "Entry"),
            h("th", {}, "Mark"), h("th", {}, "Unrealized"), h("th", {}, "Status"),
            h("th", {}, "MT5"))),
          h("tbody", {}, rows)),
  );
}

// MT5 positions table.
export function renderMt5Positions(positions) {
  if (!positions) return emptyState("MT5 positions not received.", { tone: "muted" });
  const rows = positions.positions.slice(0, 20).map((p) => h("tr", {},
    h("td", {}, `#${p.ticket ?? "?"}`),
    h("td", {}, p.side === "buy"
      ? h("span", { class: "chip chip--good" }, "BUY")
      : h("span", { class: "chip chip--bad" }, "SELL")),
    h("td", {}, p.symbol || "—"),
    h("td", { class: "num" }, fmtLots(p.volume)),
    h("td", { class: "num" }, fmtPrice(p.priceOpen)),
    h("td", { class: "num" }, fmtPrice(p.currentPrice)),
    h("td", { class: "num" }, fmtPrice(p.sl)),
    h("td", { class: "num" }, fmtPrice(p.tp)),
    h("td", { class: `num ${p.profit < 0 ? "neg" : "pos"}` }, fmtMoney(p.profit, { signed: true })),
    h("td", {}, p.comment || "—"),
  ));
  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "MT5 Positions"),
    positions.halted ? badge("positions halted: " + (positions.haltReason || "—"), "bad") : null,
    positions.positions.length === 0
      ? emptyState("No open MT5 positions.")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "Ticket"), h("th", {}, "Side"), h("th", {}, "Symbol"),
            h("th", {}, "Volume"), h("th", {}, "Open"), h("th", {}, "Current"),
            h("th", {}, "SL"), h("th", {}, "TP"), h("th", {}, "PnL"), h("th", {}, "Comment"))),
          h("tbody", {}, rows)),
  );
}

// MT5 deal history table.
export function renderMt5History(history) {
  if (!history) return emptyState("MT5 history not received.", { tone: "muted" });
  const rows = history.deals.slice(0, 20).map((d) => h("tr", {},
    h("td", {}, `#${d.ticket ?? "?"}`),
    h("td", {}, d.side === "buy"
      ? h("span", { class: "chip chip--good" }, "BUY")
      : h("span", { class: "chip chip--bad" }, "SELL")),
    h("td", {}, d.symbol || "—"),
    h("td", { class: "num" }, fmtLots(d.volume)),
    h("td", { class: "num" }, fmtPrice(d.price)),
    h("td", { class: `num ${d.profit < 0 ? "neg" : "pos"}` }, fmtMoney(d.profit, { signed: true })),
    h("td", { class: "num" }, d.commission == null ? "—" : `${fmtMoney(d.commission)}`),
    h("td", {}, d.reason || d.comment || "—"),
    h("td", { class: "num" }, fmtTime(d.timeMs ? d.timeMs : null)),
  ));
  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "MT5 History"),
    h("div", { class: "bridge__row" },
      h("span", { class: "chip" }, `${fmtCount(history.count)} deals`),
      h("span", { class: `chip ${history.complete ? "chip--good" : "chip--warn"}` },
        history.complete ? "complete" : "partial (cursor: " + (history.cursor ?? "—") + ")"),
      h("span", { class: "chip" }, `realized ${fmtMoney(history.totalRealizedPnl, { signed: true })}`),
    ),
    history.deals.length === 0
      ? emptyState("No deals in this history window.")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "Ticket"), h("th", {}, "Side"), h("th", {}, "Symbol"),
            h("th", {}, "Volume"), h("th", {}, "Price"), h("th", {}, "Profit"),
            h("th", {}, "Commission"), h("th", {}, "Reason"), h("th", {}, "Time"))),
          h("tbody", {}, rows)),
  );
}

// Bridge event log with lifecycle badges.
export function renderEventLog(events) {
  const rows = events.slice(0, 30).map((e) => {
    let detail = "";
    if (e.outcome) {
      const o = e.outcome;
      detail = `${o.symbol || o.requestedSymbol || "—"} ${o.side || "—"} ${fmtLots(o.requestedVolume)}→${fmtLots(o.filledVolume)} @ ${fmtPrice(o.price)} · ${o.retcodeDesc || o.error || "ok"}`;
      return h("tr", {},
        h("td", { class: "num" }, fmtTime(e.ts)),
        h("td", {}, lifecycleBadgeEl(lifecycleBadge(o.status))),
        h("td", {}, detail),
        h("td", {}, o.intentId ? h("code", {}, o.intentId) : "—"),
        h("td", {}, o.reconciled ? h("span", { class: "chip chip--good" }, "reconciled") : h("span", { class: "chip" }, "—")),
      );
    }
    const detail2 = e.type === "halted"
      ? `halted: ${e.reason || "—"} · flattened ${e.flattened ?? "—"}`
      : e.type === "resumed"
        ? "resume signal"
        : e.type === "ea_link_disconnected"
          ? `EA link down: ${e.reason || "—"}`
          : e.type === "trade_transaction"
            ? "position state update (fresh snapshots follow)"
            : "";
    return h("tr", {},
      h("td", { class: "num" }, fmtTime(e.ts)),
      h("td", {}, h("span", { class: `chip ${e.type === "halted" ? "chip--bad" : "chip--info"}` }, e.type)),
      h("td", {}, detail2),
      h("td", {}),
      h("td", {}),
    );
  });

  return h("section", { class: "panel" },
    h("h2", { class: "panel__title" }, "Bridge Event Log"),
    events.length === 0
      ? emptyState("No bridge events in this window.")
      : h("table", { class: "table" },
          h("thead", {}, h("tr", {},
            h("th", {}, "Time"), h("th", {}, "Event"), h("th", {}, "Detail"),
            h("th", {}, "Intent"), h("th", {}, "Reconciled"))),
          h("tbody", {}, rows)),
  );
}
