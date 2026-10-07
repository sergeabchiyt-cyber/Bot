/* ============================================================
 * Node 4 public execution dashboard.
 * Only public diagnostics and read-only broker snapshots are consumed here.
 * Structured execution reports are mirrored by Node 3's public signals
 * snapshot; Node 4's public /ws has no execution_report topic.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml, numberOrNull, fmtNumber, fmtPrice, fmtDateTime, fetchPublicJson } = App.utils;
  const Normalise = App.normalize;
  const endpoints = App.config.endpoints;
  const settings = App.config.execution;
  const MAX_ROWS = settings.maxRows || 100;
  const revisions = Normalise.createRevisionGuard();
  let started = false;
  let refreshTimer = null;
  let refreshInProgress = false;

  const state = {
    work: null,
    node3: null,
    openTrades: null,
    deriv: null,
    mt5Account: null,
    mt5Positions: null,
    mt5History: null,
    bridgeStatus: null,
    events: [],
    bridgeEvents: [],
    tradeEvents: [],
    strategySignals: null,
    snapshotTimestamp: null,
    snapshotError: "",
    resourceErrors: Object.create(null),
  };

  const record = (value) => Normalise.isRecord(value) ? value : null;
  const has = (value, key) => record(value) !== null && Normalise.own(value, key);
  const text = (value, fallback = "—") =>
    value === null || value === undefined || value === "" ? fallback : String(value);
  const num = (value, digits = 0) => fmtNumber(value, digits);
  const price = (value) => fmtPrice(value, 2);
  const timestamp = (value) => fmtDateTime(value, true);
  const unwrap = (payload) => Normalise.unwrapFrame(payload);

  function money(value, currency) {
    const amount = numberOrNull(value);
    if (amount === null) return "—";
    const sign = amount < 0 ? "−" : "";
    return `${sign}${escapeHtml(text(currency, "currency unknown"))} ${num(Math.abs(amount), 2)}`;
  }

  function newestFirst(items, getTime) {
    return items.slice().sort((left, right) => {
      const a = Normalise.timestampMs(getTime(left));
      const b = Normalise.timestampMs(getTime(right));
      return (b ?? 0) - (a ?? 0);
    });
  }

  function replaceEvents(items) {
    const list = Array.isArray(items) ? items.map(record).filter(Boolean) : [];
    state.events = Normalise.dedupeBy(newestFirst(list, (item) => item.timestamp), Normalise.eventId, MAX_ROWS);
  }

  function addEvent(item) {
    const event = record(item);
    if (!event) return;
    state.events = Normalise.dedupeBy([event, ...state.events], Normalise.eventId, MAX_ROWS);
  }

  function bridgeEventKey(event) {
    const data = record(event.data) || {};
    return [
      event.event || "",
      data.intent_id || "",
      data.position_ticket || "",
      data.deal_ticket || "",
      data.order_ticket || "",
      data.reason || "",
      data.status || "",
      data.retcode || "",
      data.retcode_desc || "",
    ].join("|");
  }

  function addBridgeEvent(event, data, receivedAt = Date.now()) {
    const item = { event: text(event, "bridge event"), data: record(data) || {}, receivedAt };
    state.bridgeEvents = Normalise.dedupeBy([item, ...state.bridgeEvents], bridgeEventKey, MAX_ROWS);
  }

  function tradeEventId(trade) {
    return [trade.trade_id || "", trade.status || "", Normalise.timestampMs(trade.timestamp) ?? ""].join("|");
  }

  function addTradeEvent(trade) {
    const item = record(trade);
    if (!item) return;
    state.tradeEvents = Normalise.dedupeBy([item, ...state.tradeEvents], tradeEventId, MAX_ROWS);
  }

  function setDomain(key, value, source, token) {
    if (source === "rest") {
      const applied = revisions.applyRest(key, token, () => { state[key] = value; });
      if (applied) state.resourceErrors[key] = "";
      return applied;
    }
    revisions.mark(key);
    state[key] = value;
    state.resourceErrors[key] = "";
    return true;
  }

  function applyDiagnostics(payload, source = "ws", tokens = {}) {
    const data = unwrap(payload);
    if (!record(data)) return false;
    let changed = false;
    for (const key of ["work", "node3", "open_trades", "deriv_account"]) {
      if (has(data, key)) {
        const domain = {
          work: "work",
          node3: "node3",
          open_trades: "openTrades",
          deriv_account: "deriv",
        }[key];
        changed = setDomain(domain, data[key], source, tokens[domain]) || changed;
      }
    }
    if (record(data.mt5)) {
      for (const [sourceKey, domain] of [
        ["account", "mt5Account"],
        ["positions", "mt5Positions"],
        ["history", "mt5History"],
        ["status", "bridgeStatus"],
      ]) {
        if (has(data.mt5, sourceKey)) {
          changed = setDomain(domain, data.mt5[sourceKey], source, tokens[domain]) || changed;
        }
      }
    }
    if (has(data, "recent_events")) {
      if (source === "rest") {
        changed = revisions.applyRest("events", tokens.events, () => replaceEvents(data.recent_events)) || changed;
      } else {
        revisions.mark("events");
        replaceEvents(data.recent_events);
        changed = true;
      }
    }
    if (has(data, "timestamp")) {
      changed = setDomain("snapshotTimestamp", data.timestamp, source, tokens.snapshotTimestamp) || changed;
    }
    if (changed) {
      state.snapshotError = "";
      render();
    }
    return changed;
  }

  function applyOpenTrades(payload, source = "ws", token) {
    let data = unwrap(payload);
    if (record(data) && has(data, "open_trades") && !has(data, "node4_open_trades")) data = data.open_trades;
    if (!record(data)) return false;
    const changed = setDomain("openTrades", data, source, token);
    if (changed) render();
    return changed;
  }

  function applyDeriv(payload, source = "ws", token) {
    let data = unwrap(payload);
    if (record(data) && has(data, "deriv_account")) data = data.deriv_account;
    if (!record(data)) return false;
    const changed = setDomain("deriv", data, source, token);
    if (changed) render();
    return changed;
  }

  function applyMt5(key, payload, source = "ws", token) {
    let data = unwrap(payload);
    if (record(data) && has(data, "mt5")) {
      const nestedKey = { mt5Account: "account", mt5Positions: "positions", mt5History: "history", bridgeStatus: "status" }[key];
      if (nestedKey && has(data.mt5, nestedKey)) data = data.mt5[nestedKey];
    }
    const wrapperKey = { mt5Account: "account", mt5Positions: "positions", mt5History: "history", bridgeStatus: "status" }[key];
    if (record(data) && wrapperKey && record(data[wrapperKey]) && !has(data, "configured")) data = data[wrapperKey];
    if (!record(data)) return false;
    const changed = setDomain(key, data, source, token);
    if (changed) render();
    return changed;
  }

  function applyFrame(frame) {
    if (!record(frame) || typeof frame.type !== "string") return;
    switch (frame.type) {
      case "diagnostics":
        applyDiagnostics(frame, "ws");
        break;
      case "open_trades":
        applyOpenTrades(frame, "ws");
        break;
      case "deriv_account":
        applyDeriv(frame, "ws");
        break;
      // Node 4's public WsFrame is `type: "trade"`; the subscription topic is
      // `trades`. This is an ExecutionTrade record, not an ExecutionReport.
      case "trade":
        addTradeEvent(unwrap(frame));
        render();
        break;
      case "diagnostic_event":
        addEvent(unwrap(frame));
        revisions.mark("events");
        render();
        break;
      case "mt5_account":
        applyMt5("mt5Account", frame, "ws");
        break;
      case "mt5_positions":
        applyMt5("mt5Positions", frame, "ws");
        break;
      case "mt5_history":
        applyMt5("mt5History", frame, "ws");
        break;
      case "bridge_status":
        applyMt5("bridgeStatus", frame, "ws");
        break;
      case "bridge_event":
        addBridgeEvent(frame.event, frame.data);
        render();
        break;
      case "heartbeat":
        break;
      default:
        // Node 4 does not publish execution_report(s) on its public socket.
        break;
    }
  }

  function setStrategySignals(signals) {
    const data = record(signals);
    state.strategySignals = data;
    render();
  }

  async function requestJson(url) {
    return fetchPublicJson(url, 12000);
  }

  function captureAll() {
    return revisions.capture([
      "work", "node3", "openTrades", "deriv", "mt5Account", "mt5Positions",
      "mt5History", "bridgeStatus", "events", "snapshotTimestamp",
    ]);
  }

  async function fetchResource(key, url, apply, tokenKey = key) {
    if (!url) return false;
    const token = revisions.current(tokenKey);
    try {
      const payload = await requestJson(url);
      if (!record(unwrap(payload))) throw new Error("Snapshot returned no compatible data");
      const changed = apply(payload, "rest", token);
      if (changed || revisions.current(tokenKey) !== token) state.resourceErrors[key] = "";
      else state.resourceErrors[key] = "Snapshot returned no compatible data";
      return changed;
    } catch (error) {
      state.resourceErrors[key] = error && error.message ? error.message : "Snapshot unavailable";
      return false;
    }
  }

  async function refreshSnapshots() {
    if (refreshInProgress) return;
    const configured = [
      endpoints.executionDiagnostics, endpoints.executionOpenTrades,
      endpoints.executionDeriv, endpoints.executionAccount, endpoints.mt5Account,
      endpoints.mt5Positions, endpoints.mt5History, endpoints.mt5Status,
    ].some(Boolean);
    if (!configured) {
      state.snapshotError = "Node 4 public endpoints are not configured.";
      render();
      return;
    }

    refreshInProgress = true;
    let diagnosticsOk = false;
    const diagTokens = captureAll();
    try {
      const snapshot = await requestJson(endpoints.executionDiagnostics);
      diagnosticsOk = !!record(unwrap(snapshot));
      if (diagnosticsOk) applyDiagnostics(snapshot, "rest", diagTokens);
    } catch (error) {
      state.snapshotError = error && error.message ? error.message : "Node 4 diagnostics snapshot unavailable";
    }

    // The diagnostics snapshot normally contains every domain. If that public
    // resource is unavailable or incomplete, fall back to the exact standalone
    // Node 4 resources instead of blocking the rest of the dashboard.
    // Do not launch a fallback for a domain that a WebSocket populated while
    // the diagnostics request was pending. For any request already in flight,
    // fetchResource's captured revision protects newer WebSocket frames.
    const tasks = [];
    if (!state.openTrades) tasks.push(fetchResource("openTrades", endpoints.executionOpenTrades, applyOpenTrades));
    if (!state.deriv) {
      tasks.push((async () => {
        const primary = await fetchResource("deriv", endpoints.executionDeriv, applyDeriv);
        if (!primary && endpoints.executionAccount) {
          await fetchResource("deriv", endpoints.executionAccount, applyDeriv);
        }
      })());
    }
    if (!state.mt5Account) tasks.push(fetchResource("mt5Account", endpoints.mt5Account, (payload, source, token) => applyMt5("mt5Account", payload, source, token)));
    if (!state.mt5Positions) tasks.push(fetchResource("mt5Positions", endpoints.mt5Positions, (payload, source, token) => applyMt5("mt5Positions", payload, source, token)));
    if (!state.mt5History) tasks.push(fetchResource("mt5History", endpoints.mt5History, (payload, source, token) => applyMt5("mt5History", payload, source, token)));
    if (!state.bridgeStatus) tasks.push(fetchResource("bridgeStatus", endpoints.mt5Status, (payload, source, token) => applyMt5("bridgeStatus", payload, source, token)));
    await Promise.all(tasks);
    refreshInProgress = false;
    if (diagnosticsOk) state.snapshotError = "";
    render();
  }

  function badge(label, tone = "muted") {
    const allowed = ["live", "blue", "amber", "warn", "muted", "unknown", "complete"];
    return `<span class="state-badge is-${allowed.includes(tone) ? tone : "muted"}">${escapeHtml(label)}</span>`;
  }

  function metric(label, value, tone = "") {
    return `<div class="dash-stat"><span class="dash-stat-label">${escapeHtml(label)}</span><strong class="dash-stat-value ${tone}">${value}</strong></div>`;
  }

  function formatUptime(value) {
    const seconds = numberOrNull(value);
    if (seconds === null) return "—";
    const total = Math.floor(seconds);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    return hours ? `${hours}h ${String(minutes).padStart(2, "0")}m` : `${minutes}m ${String(total % 60).padStart(2, "0")}s`;
  }

  function modeForVenue(venue) {
    const key = String(venue || "").toLowerCase();
    if (key === "deriv_mt5_demo") return "Deriv MT5 demo";
    if (key === "deriv_demo") return "Deriv options demo";
    if (key === "chelsea_live") return "Chelsea live";
    if (key === "none") return "Signal-only / no venue";
    return "Unknown execution mode";
  }

  function venueHealth(venue, deriv, mt5) {
    const key = String(venue || "none").toLowerCase();
    if (key === "deriv_demo") {
      if (!deriv || deriv.configured !== true) return "Deriv options venue not configured";
      if (deriv.authorized === true) return "Deriv account authorized";
      return deriv.connected === true ? "Deriv connected, not authorized" : "Deriv account disconnected";
    }
    if (key === "deriv_mt5_demo") {
      if (!mt5 || mt5.configured !== true) return "MT5 venue not configured";
      return mt5.connected === true && mt5.ea_connected === true && mt5.authorized === true
        ? "MT5 bridge and EA connected / authorized"
        : "MT5 bridge or EA is not ready";
    }
    if (key === "chelsea_live") return "Chelsea selected; no separate public Chelsea health snapshot is exposed";
    if (key === "none") return "No execution venue selected";
    return "Venue health not exposed by the public snapshot";
  }

  function renderSummary(work, node3, deriv, mt5, streamState) {
    const venue = text(work.venue, "unknown");
    const ledgerTone = work.ledger_available === true ? "live" : "warn";
    const node3Connected = node3.connected === true;
    const node3Tone = node3Connected && node3.authenticated === true ? "live" : node3Connected ? "amber" : "warn";
    const resourceError = Object.values(state.resourceErrors).find(Boolean) || "";
    const lastError = work.last_error || node3.last_error || state.snapshotError || resourceError;
    const serviceEmpty = !state.work && !state.node3
      ? `<div class="empty">${streamState.status === "not-configured" ? "Node 4 is not configured. Set EXECUTION_HOST in the runtime override." : "Waiting for Node 4 diagnostics; standalone REST snapshots remain available."}</div>`
      : "";

    return `<section class="dash-section">
      <header class="dash-section-head">
        <div><span class="dash-kicker">Node 4</span><h2>Execution service summary</h2></div>
        ${badge(streamState.status.toUpperCase(), streamState.status === "live" ? "live" : streamState.status === "stale" ? "amber" : "muted")}
      </header>
      ${serviceEmpty}
      <div class="dash-grid">
        ${metric("Selected venue", escapeHtml(venue))}
        ${metric("Execution mode", escapeHtml(modeForVenue(venue)))}
        ${metric("Selected venue health", escapeHtml(venueHealth(venue, deriv, mt5)))}
        ${metric("Node 3 intent link", `${badge(text(node3.state, node3Connected ? "connected" : "disconnected"), node3Tone)} ${node3.authenticated === true ? "authenticated" : "not authenticated"}`)}
        ${metric("Service uptime", escapeHtml(formatUptime(work.uptime_secs)))}
        ${metric("Intent received / accepted", `${num(work.intents_received)} / ${num(work.intents_accepted)}`)}
        ${metric("Intent rejected / expired / duplicate", `${num(work.intents_rejected)} / ${num(work.intents_expired)} / ${num(work.intents_duplicate)}`)}
        ${metric("Orders placed / filled", `${num(work.orders_placed)} / ${num(work.orders_filled)}`)}
        ${metric("Partial / rejected / unknown", `${num(work.orders_partial)} / ${num(work.orders_rejected)} / ${num(work.orders_unknown)}`)}
        ${metric("Orders reconciled", num(work.orders_reconciled))}
        ${metric("Ledger", `${badge(work.ledger_available === true ? "available" : "unavailable", ledgerTone)} · ${num(work.ledger_records)} records / ${num(work.ledger_intents)} intents`)}
        ${metric("Last intent", escapeHtml(timestamp(work.last_intent_ts)))}
        ${metric("Last report", escapeHtml(timestamp(work.last_report_ts)))}
        ${metric("Node 3 reconnects / frames", `${num(node3.reconnect_count)} / ${num(node3.frames_received)}`)}
        ${metric("Reports sent / acknowledged", `${num(node3.reports_sent)} / ${num(node3.reports_acked)}`)}
      </div>
      ${lastError ? `<p class="inline-error"><b>Last error</b> · ${escapeHtml(lastError)}</p>` : ""}
    </section>`;
  }

  function reportTone(status) {
    return {
      accepted: "blue",
      filled: "live",
      partial: "amber",
      rejected: "warn",
      unknown: "unknown",
      cancelled: "muted",
      closed: "complete",
    }[status] || "unknown";
  }

  function reportIdFields(report) {
    const ids = [];
    for (const field of ["execution_id", "order_ticket", "deal_ticket", "position_ticket"]) {
      if (report[field] !== null && report[field] !== undefined && report[field] !== "") {
        ids.push(`${field}: ${String(report[field])}`);
      }
    }
    return ids.length ? ids.join(" · ") : "—";
  }

  function quantityText(report) {
    const quantity = numberOrNull(report.quantity);
    if (quantity === null) return "—";
    return `${num(quantity, 4)} ${text(report.quantity_unit, "unit not supplied")}`;
  }

  function renderReports(signals) {
    const reports = signals && Array.isArray(signals.execution_reports)
      ? Normalise.dedupeBy(
        newestFirst(signals.execution_reports.map(record).filter(Boolean), (item) => item.timestamp),
        Normalise.reportId,
        MAX_ROWS,
      )
      : [];
    const intents = new Map();
    if (signals) {
      for (const intent of [...(Array.isArray(signals.recent) ? signals.recent : []), ...(Array.isArray(signals.pending) ? signals.pending : [])]) {
        if (record(intent) && intent.intent_id) intents.set(String(intent.intent_id), intent);
      }
    }
    const unknownCount = reports.filter((report) => Normalise.executionStatus(report.status) === "unknown").length;
    const warning = unknownCount
      ? `<div class="warning-banner is-danger unknown-warning" role="note" aria-label="Unknown broker execution outcome warning"><strong>${unknownCount} UNKNOWN EXECUTION OUTCOME${unknownCount === 1 ? "" : "S"}</strong><span>A broker write may have reached the venue. Reconcile using the broker state; this is not a rejection and must not be treated as safe to retry.</span></div>`
      : "";
    const rows = reports.map((report) => {
      const status = Normalise.executionStatus(report.status);
      const intent = intents.get(String(report.intent_id || ""));
      const symbolSide = `${text(report.symbol)} / ${text(report.side).toUpperCase()}`;
      const error = [report.error_code, report.error_message].filter((value) => value !== null && value !== undefined && value !== "").map(String).join(": ");
      return `<tr class="report-row report-${status}">
        <td>${badge(status, reportTone(status))}</td>
        <td class="report-intent-id">${escapeHtml(text(report.intent_id))}</td>
        <td>${escapeHtml(text(report.venue))}</td>
        <td>${escapeHtml(symbolSide)}</td>
        <td>${escapeHtml(reportIdFields(report))}</td>
        <td class="tnum">${escapeHtml(intent ? price(intent.reference_price) : "—")}</td>
        <td class="tnum">${escapeHtml(price(report.filled_price))}</td>
        <td class="tnum">${escapeHtml(quantityText(report))}</td>
        <td>${escapeHtml(timestamp(report.timestamp))}</td>
        <td class="report-error">${error ? escapeHtml(error) : "—"}</td>
      </tr>`;
    }).join("");

    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Broker-authoritative lifecycle</span><h2>Execution report timeline</h2></div>${badge(`${reports.length} reports`, unknownCount ? "unknown" : "muted")}</header>
      <p class="dash-footnote">Node 4 sends structured execution reports to Node 3 over the private service link. Node 2 reads the public <code>signals.execution_reports</code> mirror; Node 4's public WebSocket does not define an execution_report topic.</p>
      ${warning}
      ${reports.length ? `<div class="table-wrap" role="region" aria-label="Broker-authoritative execution report timeline" tabindex="0"><table class="data-table wide-table report-table"><thead><tr>
        <th>Status</th><th>Intent ID</th><th>Venue</th><th>Symbol / side</th><th>Execution / order / deal / position ID</th><th>Reference price*</th><th>Fill price</th><th>Quantity / unit</th><th>Report time</th><th>Error code / message</th>
      </tr></thead><tbody>${rows}</tbody></table></div>` : `<div class="empty">${signals ? "No broker-authoritative report yet. The strategy intent is not a fill until Node 4 reports an outcome." : "Waiting for the Node 3 public signals snapshot that mirrors Node 4 reports."}</div>`}
      <p class="dash-footnote">* Reference price is joined from the originating Node 3 intent. Current public Node 3 report snapshots expose execution_id; order/deal/position ticket fields are displayed only if the report contract supplies them. See MT5 tables for broker tickets.</p>
    </section>`;
  }

  function renderDerivContract(contract) {
    const side = text(contract.side, text(contract.contract_type)).toUpperCase();
    const profitAmount = numberOrNull(contract.profit);
    const profitTone = profitAmount > 0 ? "is-positive" : profitAmount < 0 ? "is-negative" : "";
    const spot = contract.current_spot ?? contract.entry_spot;
    return `<article class="contract-card">
      <header class="contract-head"><div><strong>${escapeHtml(text(contract.display_symbol, text(contract.symbol)))}</strong><span>${escapeHtml(side)} · ${escapeHtml(text(contract.contract_type))}</span></div>${badge(text(contract.status, "unknown"), String(contract.status || "").toLowerCase() === "open" ? "live" : "muted")}</header>
      <div class="contract-grid">
        ${metric("Contract ID", escapeHtml(text(contract.contract_id)))}
        ${metric("Buy price / stake", money(contract.buy_price, contract.currency))}
        ${metric("Bid price", money(contract.bid_price, contract.currency))}
        ${metric("Payout", money(contract.payout, contract.currency))}
        ${metric("Entry / current spot", `${escapeHtml(price(contract.entry_spot))} / ${escapeHtml(price(spot))}`)}
        ${metric("Profit / percent", `<span class="${profitTone}">${money(contract.profit, contract.currency)} · ${num(contract.profit_pct, 2)}%</span>`, profitTone)}
        ${metric("Start / expiry", `${escapeHtml(timestamp(contract.date_start))} / ${escapeHtml(timestamp(contract.date_expiry))}`)}
        ${metric("Barrier", escapeHtml(text(contract.barrier)))}
      </div>
      ${contract.longcode ? `<p class="inline-note">${escapeHtml(contract.longcode)}</p>` : ""}
    </article>`;
  }

  function derivStatus(deriv) {
    if (!deriv) return { label: "snapshot unavailable", tone: "muted" };
    if (deriv.configured === false) return { label: "not configured", tone: "muted" };
    if (deriv.authorized === true) return { label: "authorized", tone: "live" };
    if (deriv.connected === true) return { label: "configured · unauthorized", tone: "warn" };
    return { label: "configured · disconnected", tone: "amber" };
  }

  function renderDeriv(deriv, openTrades, venue) {
    const account = record(deriv);
    const openSnapshot = record(openTrades) || {};
    const contracts = account && Array.isArray(account.open_trades)
      ? account.open_trades.map(record).filter(Boolean)
      : Array.isArray(openSnapshot.deriv_open_trades) ? openSnapshot.deriv_open_trades.map(record).filter(Boolean) : [];
    const status = derivStatus(account);
    if (!account) {
      return `<section class="dash-section">
        <header class="dash-section-head"><div><span class="dash-kicker">Node 4 · options venue</span><h2>Deriv account and contracts</h2></div>${badge(status.label, status.tone)}</header>
        <div class="empty">No Deriv account snapshot received. This does not affect MT5 or strategy panels.</div>
      </section>`;
    }
    const accountType = text(account.account_type, "unknown");
    const count = numberOrNull(account.open_trades_count) ?? contracts.length;
    const hint = account.setup_hint || account.error || (String(venue) !== "deriv_demo" ? "Deriv options venue is not selected; account monitoring can still be configured independently on Node 4." : "");
    const tokenHealth = `token kind: ${text(account.token_kind, "unknown")} · app ID configured: ${account.app_id_configured === true ? "yes" : "no"}`;

    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 4 · Deriv options</span><h2>Deriv account and contracts</h2></div>${badge(status.label, status.tone)}</header>
      <div class="dash-grid">
        ${metric("Configured / connected", `${account.configured === true ? "Yes" : "No"} / ${account.connected === true ? "Yes" : "No"}`)}
        ${metric("Authorized", account.authorized === true ? badge("authorized", "live") : badge("not authorized", account.configured ? "warn" : "muted"))}
        ${metric("Demo account ID / type", `${escapeHtml(text(account.account_id))} / ${escapeHtml(accountType)}`)}
        ${metric("Balance", money(account.balance, account.currency))}
        ${metric("Open contracts", num(count))}
        ${metric("Total open stake", money(account.total_open_stake, account.currency))}
        ${metric("Total unrealized P&L", money(account.total_unrealized_pnl, account.currency))}
        ${metric("Last update", escapeHtml(timestamp(account.last_updated)))}
      </div>
      <p class="dash-footnote">Configuration health only — ${escapeHtml(tokenHealth)}. No token or app credential value is read by this page.</p>
      ${hint ? `<p class="${account.error ? "inline-error" : "inline-note"}"><b>${account.error ? "Account error" : "Setup hint"}</b> · ${escapeHtml(hint)}</p>` : ""}
      <h3 class="dash-subtitle">Deriv options contracts · stake is currency, not MT5 lots · ${num(count)}</h3>
      ${contracts.length ? contracts.slice(0, MAX_ROWS).map(renderDerivContract).join("") : `<div class="empty">${account.configured === false ? "Deriv options account is not configured." : account.authorized === false ? "Configured but not authorized; no open contracts are available." : "No open Deriv options contracts."}</div>`}
    </section>`;
  }

  function renderTrackedTrades(openTrades, venue) {
    const snapshot = record(openTrades);
    const trades = snapshot && Array.isArray(snapshot.node4_open_trades)
      ? snapshot.node4_open_trades.map(record).filter(Boolean).slice(0, MAX_ROWS)
      : [];
    const rows = trades.map((trade) => {
      const value = numberOrNull(trade.unrealized_pnl);
      const pnlTone = value > 0 ? "is-positive" : value < 0 ? "is-negative" : "";
      return `<tr>
        <td>${escapeHtml(text(trade.trade_id))}</td><td>${escapeHtml(text(trade.venue, text(venue)))}</td>
        <td>${escapeHtml(text(trade.symbol))}</td><td>${escapeHtml(text(trade.side))}</td>
        <td>${escapeHtml(text(trade.status))}</td><td class="tnum">${escapeHtml(price(trade.entry))}</td>
        <td class="tnum">${escapeHtml(price(trade.current_price))}</td><td class="tnum">${escapeHtml(price(trade.sl))}</td>
        <td class="tnum">${escapeHtml(price(trade.tp))}</td><td class="tnum ${pnlTone}">${escapeHtml(num(trade.unrealized_pnl, 2))}</td>
        <td class="tnum">${escapeHtml(num(trade.rr, 2))}</td>
        <td class="tnum">${escapeHtml(num(trade.size, 4))} · unit not supplied</td>
        <td>${escapeHtml(text(trade.intent_id))}</td><td>${escapeHtml(timestamp(trade.timestamp))}</td>
      </tr>`;
    }).join("");
    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 4 · selected venue</span><h2>Node 4 tracked open trades</h2></div>${badge(`${trades.length} records`, trades.length ? "blue" : "muted")}</header>
      <p class="dash-footnote">This is the exact node4_open_trades list. Its size unit and unrealized P&amp;L currency are not supplied by the public ExecutionTrade type, so these values are not combined with Deriv currency stake or MT5 lots/account-currency P&amp;L.</p>
      ${trades.length ? `<div class="table-wrap" role="region" aria-label="Node 4 tracked open trades" tabindex="0"><table class="data-table wide-table"><thead><tr><th>Trade ID</th><th>Venue</th><th>Symbol</th><th>Side</th><th>Status</th><th>Entry</th><th>Current</th><th>SL</th><th>TP</th><th>Unrealized P&amp;L (currency unknown)</th><th>RR</th><th>Size</th><th>Intent ID</th><th>Trade time</th></tr></thead><tbody>${rows}</tbody></table></div>` : '<div class="empty">No Node 4 tracked open trade records. Deriv contracts and MT5 positions are shown in their own units and panels.</div>'}
    </section>`;
  }

  function renderExecutionEvents(events, streamedTrades, openTrades) {
    const recentSnapshotTrades = record(openTrades) && Array.isArray(openTrades.recent_trades)
      ? openTrades.recent_trades.map(record).filter(Boolean)
      : [];
    const tradeRows = Normalise.dedupeBy(
      newestFirst([...(Array.isArray(streamedTrades) ? streamedTrades : []), ...recentSnapshotTrades], (item) => item.timestamp),
      tradeEventId,
      MAX_ROWS,
    ).map((trade) => {
      const parts = [
        `${text(trade.side, "side unknown").toUpperCase()} ${text(trade.symbol, "symbol unknown")}`,
        `trade ${text(trade.trade_id, "ID unavailable")}`,
        `status ${text(trade.status, "unknown")}`,
        `size ${num(trade.size, 4)} (unit not supplied)`,
        `entry ${price(trade.entry)}`,
      ];
      if (trade.current_price !== null && trade.current_price !== undefined) parts.push(`current ${price(trade.current_price)}`);
      if (trade.unrealized_pnl !== null && trade.unrealized_pnl !== undefined) {
        parts.push(`unrealized P&L ${num(trade.unrealized_pnl, 2)} (currency unknown)`);
      }
      return {
        timestamp: trade.timestamp,
        category: `${text(trade.venue, "Node 4")} trade`,
        level: /unknown|reject|fail|error/i.test(String(trade.status || "")) ? "warn" : "trade",
        message: parts.join(" · "),
      };
    });
    const diagnosticRows = (Array.isArray(events) ? events : []).map((event) => ({
      ...event,
      activityKind: "diagnostic",
    }));
    const items = newestFirst([...diagnosticRows, ...tradeRows], (item) => item.timestamp).slice(0, 24);
    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 4</span><h2>Execution activity</h2></div>${badge(`${items.length} recent`, "muted")}</header>
      ${items.length ? `<div class="activity-list">${items.map((event) => `
        <article class="activity-row"><time>${escapeHtml(timestamp(event.timestamp))}</time>
          ${badge(text(event.category, text(event.level, "execution")), /warn|error|reject|unknown/i.test(`${event.level || ""} ${event.category || ""}`) ? "warn" : "muted")}
          <span>${escapeHtml(text(event.message, ""))}</span></article>`).join("")}</div>` : '<div class="empty">No execution diagnostic events or trade records yet.</div>'}
    </section>`;
  }

  function render() {
    const body = $("execution-body");
    if (!body) return;
    const work = record(state.work) || {};
    const node3 = record(state.node3) || {};
    const deriv = record(state.deriv);
    const bridgeStatus = record(state.bridgeStatus);
    const streamState = App.execution.stream.getState();
    const reports = state.strategySignals && Array.isArray(state.strategySignals.execution_reports)
      ? state.strategySignals.execution_reports.filter(record) : [];
    const node4Trades = state.openTrades && Array.isArray(state.openTrades.node4_open_trades)
      ? state.openTrades.node4_open_trades.length : 0;
    const derivContracts = deriv && Array.isArray(deriv.open_trades) ? deriv.open_trades.length : 0;
    const mt5Positions = state.mt5Positions && numberOrNull(state.mt5Positions.count) !== null
      ? numberOrNull(state.mt5Positions.count)
      : state.mt5Positions && Array.isArray(state.mt5Positions.positions) ? state.mt5Positions.positions.length : 0;
    const count = $("execution-count");
    if (count) count.textContent = String(reports.length + node4Trades + derivContracts + mt5Positions);

    body.innerHTML = [
      renderSummary(work, node3, deriv, bridgeStatus, streamState),
      renderReports(state.strategySignals),
      renderDeriv(deriv, state.openTrades, work.venue),
      renderTrackedTrades(state.openTrades, work.venue),
      renderExecutionEvents(state.events, state.tradeEvents, state.openTrades),
    ].join("");

    if (App.mt5 && typeof App.mt5.render === "function") {
      App.mt5.render({
        account: state.mt5Account,
        positions: state.mt5Positions,
        history: state.mt5History,
        status: state.bridgeStatus,
        work,
        venue: work.venue,
        fallbackPositions: state.openTrades && state.openTrades.mt5_open_positions,
        bridgeEvents: state.bridgeEvents,
      });
    }
  }

  const stream = App.createServiceStream({
    service: "execution",
    endpoint: endpoints.executionWs,
    topics: settings.topics,
    reconnectMinMs: settings.reconnectMinMs,
    reconnectMaxMs: settings.reconnectMaxMs,
    staleAfterMs: settings.staleAfterMs,
    heartbeatIntervalMs: settings.heartbeatIntervalMs,
    heartbeatTimeoutMs: settings.heartbeatTimeoutMs,
    refreshSnapshots,
    onFrame: applyFrame,
    onState: render,
  });

  function start() {
    if (started) return;
    started = true;
    render();
    stream.start();
    refreshTimer = setInterval(refreshSnapshots, settings.snapshotRefreshMs);
    if (App.strategy && typeof App.strategy.getSignals === "function") {
      setStrategySignals(App.strategy.getSignals());
    }
  }

  App.execution = {
    start,
    stop() {
      stream.stop();
      if (refreshTimer !== null) clearInterval(refreshTimer);
      refreshTimer = null;
      started = false;
    },
    applyFrame,
    applyDiagnostics,
    setStrategySignals,
    refreshSnapshots,
    render,
    getState: () => ({ ...state, events: state.events.slice(), bridgeEvents: state.bridgeEvents.slice(), tradeEvents: state.tradeEvents.slice() }),
    stream,
  };
})((window.App = window.App || {}));
