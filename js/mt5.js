/* ============================================================
 * Read-only MT5 account, bridge, position, history, and safety panels.
 * Field names mirror Node 4's public Mt5*Snapshot types.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml, numberOrNull, fmtNumber, fmtPrice, fmtDateTime } = App.utils;
  const Normalise = App.normalize;
  const FRESH_MS = App.config.execution.brokerFreshMs;
  const MAX_ROWS = App.config.execution.maxRows || 100;
  const record = (value) => Normalise.isRecord(value) ? value : null;
  const text = (value, fallback = "—") =>
    value === null || value === undefined || value === "" ? fallback : String(value);
  const num = (value, digits = 0) => fmtNumber(value, digits);
  const price = (value) => fmtPrice(value, 2);
  const time = (value) => fmtDateTime(value, true);

  function badge(label, tone = "muted") {
    const allowed = ["live", "blue", "amber", "warn", "muted", "unknown", "complete"];
    return `<span class="state-badge is-${allowed.includes(tone) ? tone : "muted"}">${escapeHtml(label)}</span>`;
  }

  function metric(label, value, tone = "") {
    return `<div class="dash-stat"><span class="dash-stat-label">${escapeHtml(label)}</span><strong class="dash-stat-value ${tone}">${value}</strong></div>`;
  }

  function money(value, currency) {
    const amount = numberOrNull(value);
    if (amount === null) return "—";
    return `${amount < 0 ? "−" : ""}${escapeHtml(text(currency, "currency unknown"))} ${num(Math.abs(amount), 2)}`;
  }

  function pnl(value, currency) {
    const amount = numberOrNull(value);
    if (amount === null) return { text: "—", tone: "" };
    const sign = amount > 0 ? "+" : amount < 0 ? "−" : "";
    return {
      text: `${sign}${escapeHtml(text(currency, "currency unknown"))} ${num(Math.abs(amount), 2)}`,
      tone: amount > 0 ? "is-positive" : amount < 0 ? "is-negative" : "",
    };
  }

  function age(value) {
    const ms = Normalise.timestampMs(value);
    if (ms === null) return { known: false, stale: true, text: "unknown" };
    const seconds = Math.max(0, Math.floor((Date.now() - ms) / 1000));
    const label = seconds < 60 ? `${seconds}s ago` : seconds < 3600
      ? `${Math.floor(seconds / 60)}m ago`
      : `${Math.floor(seconds / 3600)}h ago`;
    return { known: true, stale: Date.now() - ms > FRESH_MS, text: label, ms };
  }

  function latestTimestamp(values) {
    return values.map(Normalise.timestampMs).filter((value) => value !== null)
      .reduce((newest, value) => newest === null || value > newest ? value : newest, null);
  }

  function row(label, value) {
    return `<div class="detail-row"><span>${escapeHtml(label)}</span><strong>${value}</strong></div>`;
  }

  function renderAccount(account, status, venue) {
    if (!record(account)) {
      return `<section class="dash-section">
        <header class="dash-section-head"><div><span class="dash-kicker">Node 4</span><h2>MT5 account</h2></div>${badge("waiting for snapshot", "muted")}</header>
        <div class="empty">No public MT5 account snapshot received yet. The section remains independent from the other services.</div>
      </section>`;
    }

    const mode = String(account.account_type || "unknown").trim().toLowerCase();
    const configured = account.configured === true || status.configured === true;
    const demoGuardConfirmed = account.configured === true && account.connected === true &&
      account.authorized === true && mode === "demo" && status.ea_connected === true &&
      String(status.ea_mode || "").toLowerCase() === "demo";
    const demoGuardFailed = !demoGuardConfirmed;
    const balance = account.balance;
    const equity = account.equity;
    const margin = numberOrNull(account.margin);
    const equityValue = numberOrNull(equity);
    const marginLevel = margin !== null && margin > 0 && equityValue !== null
      ? `${num((equityValue / margin) * 100, 1)}% (derived)`
      : "— (not exposed / no margin)";
    const accountAge = age(account.last_updated ?? account.last_heartbeat);
    const titleBadge = configured
      ? account.connected === true && account.authorized === true ? badge("connected / authorized", "live") : badge("configured · not authorized", "warn")
      : badge("not configured", "muted");
    const guardMessage = mode !== "demo"
      ? `Account mode is “${escapeHtml(text(account.account_type, "unknown"))}”; only a confirmed demo account is allowed.`
      : account.configured !== true
        ? "The MT5 account is not configured, so demo mode cannot be confirmed."
        : account.connected !== true
          ? "The MT5 account connection is offline; demo authorization is not current."
          : account.authorized !== true
            ? "The MT5 account is not authorized; demo-only guard is not confirmed."
            : "The connected Node 4 bridge / EA has not confirmed a demo mode.";
    const guardWarning = demoGuardFailed
      ? `<div class="warning-banner is-danger" role="note"><strong>DEMO-ONLY GUARD NOT CONFIRMED</strong><span>${guardMessage}</span></div>`
      : `<div class="warning-banner is-safe"><strong>DEMO MODE CONFIRMED</strong><span>Node 4 account and connected EA both report an authorized demo account. This page is informational only.</span></div>`;

    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 4 · ${escapeHtml(text(venue, "venue unknown"))}</span><h2>MT5 account</h2></div>${titleBadge}</header>
      ${guardWarning}
      <div class="dash-grid">
        ${metric("Configured", account.configured === true ? "Yes" : "No")}
        ${metric("Bridge connected", account.connected === true ? badge("connected", "live") : badge("disconnected", "warn"))}
        ${metric("Authorized / demo guard", account.authorized === true ? badge("authorized", "live") : badge("not authorized", "warn"))}
        ${metric("Account type", badge(text(account.account_type, "unknown"), mode === "demo" ? "live" : "unknown"))}
        ${metric("Login", escapeHtml(text(account.login)))}
        ${metric("Server / company", `${escapeHtml(text(account.server))} / ${escapeHtml(text(account.company))}`)}
        ${metric("Balance", money(balance, account.currency))}
        ${metric("Equity", money(equity, account.currency))}
        ${metric("Margin", money(account.margin, account.currency))}
        ${metric("Free margin", money(account.margin_free, account.currency))}
        ${metric("Margin level", escapeHtml(marginLevel))}
        ${metric("Currency / leverage", `${escapeHtml(text(account.currency))} / ${escapeHtml(account.leverage == null ? "—" : `${num(account.leverage)}:1`)}`)}
        ${metric("Requested / resolved symbol", `${escapeHtml(text(account.requested_symbol))} / ${escapeHtml(text(account.broker_symbol))}`)}
        ${metric("Terminal trade permission", account.trade_allowed === true ? badge("enabled", "amber") : badge("disabled", "muted"))}
        ${metric("Halted", account.halted === true ? badge("halted", "warn") : badge("not halted", "muted"))}
        ${metric("Last account update", `${escapeHtml(time(account.last_updated))} · ${escapeHtml(accountAge.text)}`)}
      </div>
      ${account.halt_reason ? `<p class="inline-warning"><b>Halt reason</b> · ${escapeHtml(account.halt_reason)}</p>` : ""}
      <div class="detail-grid">
        ${row("Bridge / EA version", `${escapeHtml(text(account.bridge_version))} / ${escapeHtml(text(account.ea_version))}`)}
        ${row("Symbol digits", escapeHtml(text(account.symbol_digits)))}
        ${row("Volume min / step / max", `${escapeHtml(text(account.symbol_volume_min))} / ${escapeHtml(text(account.symbol_volume_step))} / ${escapeHtml(text(account.symbol_volume_max))} lots`)}
        ${row("Contract size", `${escapeHtml(text(account.symbol_contract_size))} (broker-reported)`)}
        ${row("Terminal build / latency", `${escapeHtml(text(account.terminal_build))} / ${escapeHtml(text(account.latency_ms))} ms`)}
        ${row("Last heartbeat", `${escapeHtml(time(account.last_heartbeat))} · ${escapeHtml(age(account.last_heartbeat).text)}`)}
      </div>
      ${account.setup_hint ? `<p class="inline-note"><b>Setup hint</b> · ${escapeHtml(account.setup_hint)}</p>` : ""}
      ${account.error ? `<p class="inline-error"><b>Account error</b> · ${escapeHtml(account.error)}</p>` : ""}
    </section>`;
  }

  function renderBridgeHealth(account, positions, history, status, events) {
    const bridge = record(status) || {};
    const acct = record(account) || {};
    const positionSnapshot = record(positions) || {};
    const historySnapshot = record(history) || {};
    const frameTimestamp = latestTimestamp([
      bridge.timestamp,
      acct.last_updated,
      acct.last_heartbeat,
      positionSnapshot.timestamp,
      historySnapshot.timestamp,
    ]);
    const frameAge = age(frameTimestamp);
    const accountAge = age(acct.last_updated ?? acct.last_heartbeat);
    const quoteAge = age(positionSnapshot.timestamp);
    const configured = bridge.configured === true || acct.configured === true;
    const bridgeOk = bridge.connected === true;
    const eaOk = bridge.ea_connected === true;
    const eventRows = Array.isArray(events) ? events.slice(0, 12) : [];

    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 4</span><h2>MT5 bridge / EA health</h2></div>${badge(bridgeOk ? "bridge connected" : configured ? "bridge disconnected" : "not configured", bridgeOk ? "live" : configured ? "warn" : "muted")}</header>
      <div class="dash-grid">
        ${metric("Bridge protocol / version", `${escapeHtml(text(bridge.protocol))} / ${escapeHtml(text(bridge.bridge_version))}`)}
        ${metric("Bridge connected / authorized", `${badge(bridgeOk ? "connected" : "disconnected", bridgeOk ? "live" : "warn")} ${badge(bridge.authorized === true ? "authorized" : "not authorized", bridge.authorized === true ? "live" : "warn")}`)}
        ${metric("EA connected", eaOk ? badge("connected", "live") : badge("disconnected", configured ? "warn" : "muted"))}
        ${metric("EA write permission", bridge.ea_write_enabled === true ? badge("enabled", "amber") : badge("disabled", "muted"))}
        ${metric("EA mode / account mode", `${escapeHtml(text(bridge.ea_mode, "unknown"))} / ${escapeHtml(text(acct.account_type, "unknown"))}`)}
        ${metric("EA login / server", `${escapeHtml(text(bridge.ea_login))} / ${escapeHtml(text(bridge.ea_server))}`)}
        ${metric("Configured / resolved symbol", `${escapeHtml(text(acct.requested_symbol))} / ${escapeHtml(text(acct.broker_symbol))}`)}
        ${metric("EA heartbeat age", escapeHtml(age(bridge.ea_last_heartbeat).text))}
        ${metric("Latest public snapshot age", escapeHtml(frameAge.text))}
        ${metric("Account snapshot age", `${escapeHtml(accountAge.text)}${accountAge.stale ? " · stale" : ""}`)}
        ${metric("Position / quote snapshot age", `${escapeHtml(quoteAge.text)}${quoteAge.stale ? " · stale" : ""}`)}
        ${metric("Trading enabled / halted", `${badge(bridge.trading_enabled === true ? "enabled" : "disabled", bridge.trading_enabled === true ? "amber" : "muted")} ${badge(bridge.halted === true ? "halted" : "not halted", bridge.halted === true ? "warn" : "muted")}`)}
        ${metric("Bridge uptime", `${num(bridge.uptime_secs)} sec`)}
        ${metric("Orders sent / filled", `${num(bridge.orders_sent)} / ${num(bridge.orders_filled)}`)}
        ${metric("Rejected / unknown", `${num(bridge.orders_rejected)} / ${num(bridge.orders_unknown)}`)}
        ${metric("Positions / history deals", `${num(bridge.positions_open)} / ${num(bridge.history_deals)}`)}
      </div>
      <p class="dash-footnote">The public Mt5BridgeStatus has no reconnect, raw-frame, or request counters. The displayed counters are the exact broker order counters exposed by Node 4.</p>
      ${bridge.halted ? `<p class="inline-warning"><b>Halted</b> · ${escapeHtml(text(bridge.halt_reason, "reason not supplied"))}</p>` : ""}
      ${bridge.last_error ? `<p class="inline-error"><b>Bridge error</b> · ${escapeHtml(bridge.last_error)}</p>` : ""}
      ${eventRows.length ? `<h3 class="dash-subtitle">Recent bridge events</h3><div class="activity-list">${eventRows.map(renderBridgeEvent).join("")}</div>` : '<div class="empty compact-empty">No bridge events received in this browser session.</div>'}
    </section>`;
  }

  function renderBridgeEvent(event) {
    const name = text(event.event, text(event.category, "bridge event"));
    const data = record(event.data);
    let detail = text(event.message, "");
    if (!detail && data) {
      const keys = ["intent_id", "reason", "status", "code", "retcode", "retcode_desc", "order_ticket", "deal_ticket", "position_ticket", "reconciled", "message"];
      detail = keys.filter((key) => data[key] !== undefined && data[key] !== null)
        .map((key) => `${key}: ${String(data[key])}`).join(" · ");
    }
    return `<article class="activity-row">
      <time>${escapeHtml(time(event.timestamp))}</time>
      ${badge(name, /unknown|halt|disconnect|reject|error/i.test(name) ? "warn" : "muted")}
      <span>${escapeHtml(detail || "No public detail")}</span>
    </article>`;
  }

  function renderPositions(snapshot, fallbackPositions, venue, currency) {
    const data = record(snapshot) || {};
    const pnlCurrency = text(currency, "currency unknown");
    const positions = Array.isArray(data.positions) ? data.positions.map(record).filter(Boolean)
      : Array.isArray(fallbackPositions) ? fallbackPositions.map(record).filter(Boolean) : [];
    const updated = data.timestamp;
    const configured = data.account_type === "demo" || data.account_login !== null && data.account_login !== undefined;
    const emptyMessage = positions.length ? "" : configured
      ? "No open MT5 positions."
      : String(venue || "none") === "deriv_mt5_demo"
        ? "MT5 positions are not available yet; check the public MT5 status snapshot."
        : "No MT5 positions. The selected venue does not currently expose an MT5 account.";
    const rows = positions.slice(0, MAX_ROWS).map((position) => {
      const pnlValue = pnl(position.unrealized_pnl ?? position.profit, pnlCurrency);
      return `<tr>
        <td>${escapeHtml(text(position.ticket))}</td>
        <td>${escapeHtml(text(position.symbol))}</td>
        <td>${escapeHtml(text(position.side))}</td>
        <td class="tnum">${escapeHtml(num(position.volume, 2))} lots</td>
        <td class="tnum">${escapeHtml(price(position.price_open))}</td>
        <td class="tnum">${escapeHtml(price(position.current_price))}</td>
        <td class="tnum">${escapeHtml(price(position.sl))}</td>
        <td class="tnum">${escapeHtml(price(position.tp))}</td>
        <td class="tnum ${pnlValue.tone}">${pnlValue.text}</td>
        <td class="tnum">${escapeHtml(money(position.swap, pnlCurrency))}</td>
        <td>${escapeHtml(text(position.comment, "—"))}<small>magic ${escapeHtml(text(position.magic))}</small></td>
        <td>${escapeHtml(time(position.time_ms))}</td>
        <td>${escapeHtml(time(updated))}</td>
      </tr>`;
    }).join("");

    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 4 · broker positions</span><h2>MT5 open positions</h2></div>${badge(`${num(data.count ?? positions.length)} positions · lots`, positions.length ? "blue" : "muted")}</header>
      <div class="dash-grid compact-grid">
        ${metric("Position count", num(data.count ?? positions.length))}
        ${metric("Total volume", `${num(data.total_volume, 2)} lots`)}
        ${metric("Total unrealized P&L", money(data.total_unrealized_pnl, pnlCurrency))}
        ${metric("Snapshot source / time", `${escapeHtml(text(data.source))} / ${escapeHtml(time(updated))}`)}
      </div>
      ${data.halted ? `<p class="inline-warning"><b>Halted</b> · ${escapeHtml(text(data.halt_reason, "reason not supplied"))}</p>` : ""}
      ${positions.length ? `<div class="table-wrap" role="region" aria-label="MT5 open positions" tabindex="0"><table class="data-table wide-table"><thead><tr>
        <th>Ticket</th><th>Symbol</th><th>Side</th><th>Volume</th><th>Open price</th><th>Current price</th><th>SL</th><th>TP</th><th>Unrealized P&amp;L</th><th>Swap</th><th>Comment / magic</th><th>Open time</th><th>Snapshot update</th>
      </tr></thead><tbody>${rows}</tbody></table></div>` : `<div class="empty">${escapeHtml(emptyMessage)}</div>`}
      <p class="dash-footnote">MT5 position volume is measured in lots. The Node 4 Mt5Position type exposes swap, but not a separate open-position commission field.</p>
    </section>`;
  }

  function historyRows(deals) {
    const opensByPosition = new Map();
    for (const deal of deals) {
      if (String(deal.entry || "").toLowerCase() !== "in") continue;
      opensByPosition.set(String(deal.position_ticket), deal);
    }
    const closed = deals.filter((deal) => ["out", "inout"].includes(String(deal.entry || "").toLowerCase()));
    return closed.map((deal) => ({ deal, open: opensByPosition.get(String(deal.position_ticket)) || null }));
  }

  function renderHistory(snapshot, venue, currency) {
    const data = record(snapshot) || {};
    const pnlCurrency = text(currency, "currency unknown");
    const deals = Array.isArray(data.deals) ? data.deals.map(record).filter(Boolean) : [];
    const rows = historyRows(deals).slice(0, MAX_ROWS);
    const loadedNet = deals.length ? deals.reduce((sum, deal) => {
      const profitValue = numberOrNull(deal.profit) ?? 0;
      const swapValue = numberOrNull(deal.swap) ?? 0;
      const commissionValue = numberOrNull(deal.commission) ?? 0;
      return sum + profitValue + swapValue + commissionValue;
    }, 0) : null;
    const reportedTotal = numberOrNull(data.total_realized_pnl);
    const closedRows = rows.map(({ deal, open }) => {
      const net = (numberOrNull(deal.profit) ?? 0) + (numberOrNull(deal.swap) ?? 0) + (numberOrNull(deal.commission) ?? 0);
      const netValue = pnl(net, pnlCurrency);
      return `<tr>
        <td>${escapeHtml(text(deal.ticket))}</td>
        <td>${escapeHtml(text(deal.order_ticket))}</td>
        <td>${escapeHtml(text(deal.position_ticket))}</td>
        <td>${escapeHtml(text(deal.comment, "—"))}</td>
        <td>${escapeHtml(text(deal.reason, "—"))}</td>
        <td>${escapeHtml(text(deal.symbol))}</td>
        <td>${escapeHtml(text(deal.side))}</td>
        <td class="tnum">${escapeHtml(num(deal.volume, 2))} lots</td>
        <td class="tnum">${escapeHtml(open ? price(open.price) : "—")}</td>
        <td class="tnum">${escapeHtml(price(deal.price))}</td>
        <td>${escapeHtml(time(deal.time_ms))}</td>
        <td class="tnum">${escapeHtml(money(deal.profit, pnlCurrency))}</td>
        <td class="tnum">${escapeHtml(money(deal.swap, pnlCurrency))}</td>
        <td class="tnum">${escapeHtml(money(deal.commission, pnlCurrency))}</td>
        <td class="tnum ${netValue.tone}">${netValue.text}</td>
      </tr>`;
    }).join("");
    const unavailable = String(venue || "none") === "deriv_mt5_demo"
      ? "No closed deals loaded yet. Check the Node 4 MT5 history endpoint and bridge status."
      : "No MT5 closed-deal history for the selected venue.";

    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 4 · bounded broker snapshot</span><h2>MT5 closed-deal history</h2></div>${badge(`${num(data.count ?? deals.length)} deals`, deals.length ? "blue" : "muted")}</header>
      <div class="dash-grid compact-grid">
        ${metric("Node 4 snapshot realized total", money(reportedTotal, pnlCurrency))}
        ${metric("Loaded-deal net (profit + swap + commission)", money(loadedNet, pnlCurrency))}
        ${metric("Loaded range", `${escapeHtml(time(data.first_ms))} – ${escapeHtml(time(data.last_ms))}`)}
        ${metric("History snapshot update", escapeHtml(time(data.timestamp)))}
      </div>
      ${rows.length ? `<div class="table-wrap" role="region" aria-label="MT5 closed deal history" tabindex="0"><table class="data-table wide-table"><thead><tr>
        <th>Deal ticket</th><th>Order ticket</th><th>Position ticket</th><th>Comment / intent</th><th>Close reason</th><th>Symbol</th><th>Side</th><th>Volume</th><th>Open price*</th><th>Close deal price</th><th>Close time</th><th>Gross profit</th><th>Swap</th><th>Commission</th><th>Net realized P&amp;L</th>
      </tr></thead><tbody>${closedRows}</tbody></table></div>` : `<div class="empty">${escapeHtml(unavailable)}</div>`}
      <p class="dash-footnote">* Open price is shown only when an entry: in deal for the same position_ticket exists in this loaded snapshot; otherwise the public contract has no linked open price. Showing up to ${MAX_ROWS} closed rows; Node 4 reports whether its history snapshot is complete.</p>
      <p class="dash-footnote">Snapshot complete: ${escapeHtml(text(data.complete, "unknown"))} · Source: ${escapeHtml(text(data.source, "unknown"))}</p>
    </section>`;
  }

  function renderSafety(account, positions, status, work, venue) {
    const acct = record(account) || {};
    const pos = record(positions) || {};
    const bridge = record(status) || {};
    const diag = record(work) || {};
    const mode = String(acct.account_type || "unknown").toLowerCase();
    const accountAge = age(acct.last_updated ?? acct.last_heartbeat);
    const quoteAge = age(pos.timestamp);
    const halted = acct.halted === true || pos.halted === true || bridge.halted === true;
    const configured = acct.configured === true || bridge.configured === true;
    const guardOk = acct.configured === true && acct.connected === true && mode === "demo" &&
      acct.authorized === true && bridge.connected === true && bridge.ea_connected === true &&
      String(bridge.ea_mode || "").toLowerCase() === "demo";
    const ledgerOk = diag.ledger_available === true;
    const venueSelected = String(venue || diag.venue || "none").toLowerCase() !== "none";
    const warnings = [];

    if (mode !== "demo") warnings.push("Account mode is not confirmed as demo");
    if (configured && acct.authorized !== true) warnings.push("MT5 demo guard is not authorized");
    if (halted) warnings.push(`Halted${acct.halt_reason || bridge.halt_reason || pos.halt_reason ? `: ${text(acct.halt_reason || bridge.halt_reason || pos.halt_reason)}` : ""}`);
    if (configured && bridge.connected !== true) warnings.push("Bridge disconnected");
    if (configured && bridge.ea_connected !== true) warnings.push("EA disconnected");
    if (configured && accountAge.stale) warnings.push("Account snapshot stale / unavailable");
    if (configured && quoteAge.stale) warnings.push("Position / quote snapshot stale / unavailable");
    if (!ledgerOk) warnings.push("Execution ledger unavailable");
    if (!venueSelected) warnings.push("Execution venue unconfigured (venue is none)");

    const items = [
      ["Demo mode", guardOk ? "confirmed" : mode === "demo" ? "not authorized" : mode, guardOk ? "live" : "unknown"],
      ["Trading enabled", bridge.trading_enabled === true && acct.trade_allowed === true ? "enabled" : "disabled / not confirmed", bridge.trading_enabled === true && acct.trade_allowed === true ? "amber" : "muted"],
      ["Halted", halted ? "halted" : "not halted", halted ? "warn" : "muted"],
      ["Halt reason", acct.halt_reason || bridge.halt_reason || pos.halt_reason || "none reported", halted ? "warn" : "muted"],
      ["Bridge disconnected", configured && bridge.connected !== true ? "yes" : configured ? "no" : "not configured", configured && bridge.connected !== true ? "warn" : "muted"],
      ["EA disconnected", configured && bridge.ea_connected !== true ? "yes" : configured ? "no" : "not configured", configured && bridge.ea_connected !== true ? "warn" : "muted"],
      ["Stale quote / account", configured && (quoteAge.stale || accountAge.stale) ? "yes" : configured ? "no" : "not available", configured && (quoteAge.stale || accountAge.stale) ? "warn" : "muted"],
      ["Ledger unavailable", ledgerOk ? "no" : "yes / unknown", ledgerOk ? "muted" : "warn"],
      ["Venue unconfigured", venueSelected ? "no" : "yes", venueSelected ? "muted" : "warn"],
    ];
    const warning = warnings.length
      ? `<div class="warning-banner is-danger" role="note"><strong>SAFETY ATTENTION</strong><span>${escapeHtml(warnings.join(" · "))}</span></div>`
      : `<div class="warning-banner is-safe"><strong>NO REPORTED SAFETY ALERT</strong><span>Read-only status only. This dashboard has no halt, resume, flatten, or close controls.</span></div>`;

    return `<section class="dash-section safety-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Read-only</span><h2>MT5 safety state</h2></div>${badge(halted ? "halted" : "monitoring", halted ? "warn" : "muted")}</header>
      ${warning}
      <div class="safety-grid">${items.map(([label, value, tone]) => `<div class="safety-item"><span>${escapeHtml(label)}</span>${badge(String(value), tone)}</div>`).join("")}</div>
      <p class="dash-footnote">No privileged operator actions are available in Node 2.</p>
    </section>`;
  }

  function render(snapshot) {
    const state = record(snapshot) || {};
    const account = record(state.account);
    const positions = record(state.positions);
    const history = record(state.history);
    const status = record(state.status);
    const venue = text(state.venue, "none");
    const bridgeEvents = Array.isArray(state.bridgeEvents) ? state.bridgeEvents : [];
    const body = $("mt5-body");
    if (body) {
      body.innerHTML = [
        renderSafety(account, positions, status, state.work, venue),
        renderAccount(account, status || {}, venue),
        renderBridgeHealth(account, positions, history, status, bridgeEvents),
        renderPositions(positions, state.fallbackPositions, venue, account && account.currency),
        renderHistory(history, venue, account && account.currency),
      ].join("");
    }
    const count = $("mt5-count");
    if (count) {
      const positionCount = positions && numberOrNull(positions.count) !== null
        ? numberOrNull(positions.count)
        : positions && Array.isArray(positions.positions) ? positions.positions.length : 0;
      const dealCount = history && numberOrNull(history.count) !== null
        ? numberOrNull(history.count)
        : history && Array.isArray(history.deals) ? history.deals.length : 0;
      count.textContent = String(positionCount + dealCount);
    }
    return body ? body.innerHTML : "";
  }

  App.mt5 = { render };
})((window.App = window.App || {}));
