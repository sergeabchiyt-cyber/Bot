/* ============================================================
 * XAUUSD Terminal — demo (paper) account.
 *
 * The Node 1 demo engine trades itself; this module only observes it:
 * account strip, open position, performance card and closed-trade table.
 *
 * `GET /demo` seeds the state; each `demo` WS frame is a COMPLETE snapshot
 * and replaces it wholesale, guarded only against an older `ts`. There is no
 * partial-merge semantics and no client-side trading, sizing or margin math.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml } = App.utils;
  const format = App.format;
  const math = App.strategyMath;
  const normalise = App.normalize;
  const cfg = App.config.demo;

  const html = (value) => escapeHtml(value);
  const finite = (value) => math.finite(value);
  const EMPTY_POSITION = "No open position \u2014 the engine is waiting for a qualifying signal";

  let snapshot = null;
  let wsRevision = 0;
  let lastFrameAt = null;
  let lastError = "";
  let fetchInProgress = false;
  let started = false;
  const updateListeners = new Set();

  function normaliseQuote(raw) {
    const quote = normalise.isRecord(raw) ? raw : {};
    return {
      bid: finite(quote.bid),
      ask: finite(quote.ask),
      mid: finite(quote.mid),
      spread: finite(quote.spread),
      ts: normalise.timestampMs(quote.ts),
    };
  }

  function normaliseAccount(raw) {
    const account = normalise.isRecord(raw) ? raw : {};
    return {
      currency: account.currency ? String(account.currency) : "USD",
      balance: finite(account.balance),
      equity: finite(account.equity),
      margin: finite(account.margin),
      free_margin: finite(account.free_margin),
      margin_level: finite(account.margin_level),
      unrealized_pnl: finite(account.unrealized_pnl),
      peak_equity: finite(account.peak_equity),
      max_drawdown_pct: finite(account.max_drawdown_pct),
    };
  }

  function normalisePosition(raw) {
    if (!normalise.isRecord(raw)) return null;
    return {
      id: raw.id,
      side: String(raw.side || "").toLowerCase() === "sell" ? "sell" : "buy",
      lots: finite(raw.lots),
      entry: finite(raw.entry),
      stop_loss: finite(raw.stop_loss),
      take_profit: finite(raw.take_profit),
      opened_at: normalise.timestampMs(raw.opened_at),
      risk_usd: finite(raw.risk_usd),
      margin: finite(raw.margin),
      unrealized_pnl: finite(raw.unrealized_pnl),
      reason: String(raw.reason == null ? "" : raw.reason),
    };
  }

  function normaliseTrade(raw) {
    if (!normalise.isRecord(raw)) return null;
    return {
      id: raw.id,
      side: String(raw.side || "").toLowerCase() === "sell" ? "sell" : "buy",
      lots: finite(raw.lots),
      entry: finite(raw.entry),
      exit: finite(raw.exit),
      opened_at: normalise.timestampMs(raw.opened_at),
      closed_at: normalise.timestampMs(raw.closed_at),
      pnl: finite(raw.pnl),
      r_multiple: finite(raw.r_multiple),
      exit_reason: String(raw.exit_reason == null ? "" : raw.exit_reason),
    };
  }

  function normaliseStats(raw) {
    const stats = normalise.isRecord(raw) ? raw : {};
    return {
      closed_trades: finite(stats.closed_trades),
      wins: finite(stats.wins),
      losses: finite(stats.losses),
      win_rate: finite(stats.win_rate),
      gross_profit: finite(stats.gross_profit),
      gross_loss: finite(stats.gross_loss),
      net_pnl: finite(stats.net_pnl),
      profit_factor: finite(stats.profit_factor),
      avg_r: finite(stats.avg_r),
      max_drawdown_pct: finite(stats.max_drawdown_pct),
    };
  }

  function normaliseParams(raw) {
    const params = normalise.isRecord(raw) ? raw : {};
    return {
      start_balance: finite(params.start_balance),
      leverage: finite(params.leverage),
      spread: finite(params.spread),
      min_lots: finite(params.min_lots),
      max_lots: finite(params.max_lots),
      lot_step: finite(params.lot_step),
      stop_out_pct: finite(params.stop_out_pct),
    };
  }

  function normaliseSnapshot(raw) {
    if (!normalise.isRecord(raw)) return null;
    const trades = (Array.isArray(raw.recent_trades) ? raw.recent_trades : [])
      .map(normaliseTrade)
      .filter(Boolean)
      .sort((a, b) => (b.closed_at || b.opened_at || 0) - (a.closed_at || a.opened_at || 0))
      .slice(0, cfg.maxTrades);
    const position = normalisePosition(raw.position);
    return {
      enabled: raw.enabled === true,
      mode: raw.mode ? String(raw.mode) : "demo",
      symbol: raw.symbol ? String(raw.symbol) : "XAUUSD",
      leverage: finite(raw.leverage),
      quote: normaliseQuote(raw.quote),
      account: normaliseAccount(raw.account),
      params: normaliseParams(raw.params),
      position,
      stats: normaliseStats(raw.stats),
      recent_trades: trades,
      note: String(raw.note == null ? "" : raw.note),
      ts: normalise.timestampMs(raw.ts),
    };
  }

  function getState() {
    return { snapshot, lastFrameAt, lastError };
  }

  function notify() {
    for (const listener of updateListeners) {
      try { listener(snapshot); } catch (error) { console.error("Demo update listener failed:", error); }
    }
  }

  function applySnapshot(raw, source = "ws", startRevision = wsRevision) {
    if (source === "rest" && startRevision !== wsRevision) return false;
    const next = normaliseSnapshot(raw);
    if (!next) return false;
    // Complete snapshots only move forward. Equal ts still replaces (the
    // payload may be a richer replay of the same instant).
    if (snapshot && snapshot.ts !== null && next.ts !== null && next.ts < snapshot.ts) {
      return false;
    }
    snapshot = next;
    if (source === "ws") {
      wsRevision += 1;
      lastFrameAt = Date.now();
      lastError = "";
    }
    render();
    notify();
    return true;
  }

  async function load() {
    if (fetchInProgress) return false;
    fetchInProgress = true;
    const startRevision = wsRevision;
    try {
      const payload = await App.api.fetchDemo();
      return applySnapshot(payload, "rest", startRevision);
    } catch (error) {
      lastError = error && error.message ? error.message : "demo snapshot unavailable";
      console.warn("Demo snapshot unavailable:", error);
      if (!snapshot) render();
      return false;
    } finally {
      fetchInProgress = false;
    }
  }

  // ---------- Account strip ----------

  function setText(id, text, className) {
    const el = $(id);
    if (!el) return;
    el.textContent = text;
    if (className !== undefined && el.classList) {
      el.className = className;
    }
  }

  function signClass(value, neutral = "") {
    const number = finite(value);
    if (number === null || number === 0) return neutral;
    return number > 0 ? "is-pos" : "is-neg";
  }

  function renderStrip() {
    const strip = $("demo-strip");
    if (!strip) return;
    const account = snapshot ? snapshot.account : normaliseAccount(null);
    const params = snapshot ? snapshot.params : normaliseParams(null);
    const leverage = snapshot
      ? (finite(snapshot.leverage) ?? finite(params.leverage))
      : null;
    const spread = params.spread;

    if (strip.classList) strip.classList.toggle("is-off", !!snapshot && !snapshot.enabled);
    strip.title = "Simulated fills \u2014 no broker, no real money.";

    setText("demo-mode", snapshot ? String(snapshot.mode || "demo").toUpperCase() : "DEMO");
    setText("demo-spec",
      `${leverage === null ? format.DASH : `1:${String(leverage)}`} \u00b7 spread ${format.price(spread)}`);

    const negative = (value) => finite(value) !== null && value < 0 ? "is-neg" : "";
    setText("demo-balance", format.money(account.balance), `tnum ${negative(account.balance)}`.trim());
    setText("demo-equity", format.money(account.equity), `tnum ${negative(account.equity)}`.trim());
    setText("demo-free-margin", format.money(account.free_margin), `tnum ${negative(account.free_margin)}`.trim());

    // A flat account has margin 0: show "—", never 0 % or infinity.
    const level = finite(account.margin_level);
    const flat = finite(account.margin) === 0 || level === 0;
    if (level === null || flat) {
      setText("demo-margin-level", format.DASH, "tnum");
    } else {
      const tone = level < cfg.marginDangerPct ? "is-neg" : level < cfg.marginWarnPct ? "is-warn" : "";
      setText("demo-margin-level", format.percent(level), `tnum ${tone}`.trim());
    }

    setText("demo-open-pnl", format.signedMoney(account.unrealized_pnl),
      `tnum ${signClass(account.unrealized_pnl)}`.trim());

    const engine = $("demo-engine");
    if (engine) {
      engine.hidden = !snapshot || snapshot.enabled;
      engine.textContent = "demo engine off";
    }
    const updated = $("demo-updated");
    if (updated) {
      updated.textContent = snapshot && snapshot.ts !== null
        ? `updated ${format.time(snapshot.ts)}`
        : format.DASH;
    }
  }

  // ---------- Open position ----------

  function currentPrice(position) {
    if (!snapshot) return null;
    const quote = snapshot.quote;
    const side = position.side;
    return (side === "buy" ? quote.bid : quote.ask) ?? quote.mid ?? quote.bid ?? quote.ask;
  }

  function progressHtml(position) {
    const geometry = math.progressToTarget({
      side: position.side,
      entry: position.entry,
      stop: position.stop_loss,
      target: position.take_profit,
      price: currentPrice(position),
    });
    if (!geometry.available) {
      return `<div class="pos-no-target">No target set \u2014 progress unavailable.</div>`;
    }
    const profitLeft = Math.min(geometry.entryPct, geometry.targetPct);
    const profitWidth = Math.abs(geometry.targetPct - geometry.entryPct);
    const loss = geometry.hasStop
      ? {
          left: Math.min(geometry.entryPct, geometry.stopPct),
          width: Math.abs(geometry.entryPct - geometry.stopPct),
        }
      : null;
    const progressText = geometry.progressPct === null ? format.DASH : format.percent(geometry.progressPct);
    // Scale labels follow the track (low price on the left), so a short shows
    // TP on the left and SL on the right.
    const scale = [
      { label: "SL", value: position.stop_loss },
      { label: "Entry", value: position.entry },
      { label: "TP", value: position.take_profit },
    ].filter((item) => item.value !== null).sort((a, b) => a.value - b.value);
    return `
      <div class="pos-bar" role="progressbar" aria-valuemin="0" aria-valuemax="100"
           aria-valuenow="${geometry.progressPct === null ? 0 : geometry.progressPct.toFixed(1)}"
           aria-valuetext="${html(progressText)} to target" aria-label="Progress to target">
        ${loss ? `<span class="pos-zone is-loss" style="left:${loss.left.toFixed(2)}%;width:${loss.width.toFixed(2)}%"></span>` : ""}
        <span class="pos-zone is-profit" style="left:${profitLeft.toFixed(2)}%;width:${profitWidth.toFixed(2)}%"></span>
        ${geometry.stopPct === null ? "" : `<span class="pos-stop" style="left:${geometry.stopPct.toFixed(2)}%" title="Stop loss"></span>`}
        <span class="pos-entry" style="left:${geometry.entryPct.toFixed(2)}%" title="Entry"></span>
        ${geometry.pricePct === null ? "" : `<span class="pos-price is-${position.side}" style="left:${geometry.pricePct.toFixed(2)}%" title="Current price"></span>`}
      </div>
      <div class="pos-scale">
        ${scale.map((item) => `<span>${html(item.label)} ${html(format.price(item.value))}</span>`).join("")}
      </div>`;
  }

  function renderPosition() {
    const body = $("position-body");
    const stateEl = $("position-state");
    if (!body) return;

    if (!snapshot) {
      if (stateEl) stateEl.textContent = format.DASH;
      body.innerHTML = `<div class="skeleton-line w60"></div><div class="empty">Loading the demo account\u2026</div>`;
      return;
    }

    const position = snapshot.position;
    if (stateEl) stateEl.textContent = position ? "OPEN" : "FLAT";

    if (!position) {
      body.innerHTML = `
        <div class="empty">${html(EMPTY_POSITION)}</div>
        ${snapshot.note ? `<div class="pos-last-note"><span class="muted">Last engine action:</span> ${html(snapshot.note)}</div>` : ""}`;
      return;
    }

    const pnl = position.unrealized_pnl;
    body.innerHTML = `
      <div class="pos-head">
        <span class="side-chip is-${position.side}">${html(position.side.toUpperCase())}</span>
        <span class="pos-lots tnum">${html(format.lots(position.lots))} lots</span>
        <span class="pos-pnl tnum ${signClass(pnl)}">${html(format.signedMoney(pnl))}</span>
      </div>
      ${progressHtml(position)}
      <dl class="kv-grid">
        <div><dt>Entry</dt><dd class="tnum">${html(format.price(position.entry))}</dd></div>
        <div><dt>Stop</dt><dd class="tnum">${html(format.price(position.stop_loss))}</dd></div>
        <div><dt>Target</dt><dd class="tnum">${html(format.price(position.take_profit))}</dd></div>
        <div><dt>Margin</dt><dd class="tnum">${html(format.money(position.margin))}</dd></div>
        <div><dt>Risk</dt><dd class="tnum">${html(format.money(position.risk_usd))}</dd></div>
        <div><dt>Opened</dt><dd class="tnum">${html(format.dateTime(position.opened_at))}</dd></div>
        <div class="kv-wide"><dt>Reason</dt><dd>${html(position.reason || format.DASH)}</dd></div>
      </dl>
      ${snapshot.note ? `<div class="pos-last-note"><span class="muted">Engine:</span> ${html(snapshot.note)}</div>` : ""}`;
  }

  // ---------- Performance ----------

  function profitFactorText(stats) {
    const closed = finite(stats.closed_trades);
    if (!closed || closed <= 0) return format.DASH;
    const factor = finite(stats.profit_factor);
    if (factor === null) return format.DASH;
    if (factor >= cfg.profitFactorCap) return `${cfg.profitFactorCap}+`;
    return format.number(factor, 2);
  }

  function renderPerformance() {
    const body = $("performance-body");
    if (!body) return;
    if (!snapshot) {
      body.innerHTML = `<div class="empty">Waiting for demo statistics\u2026</div>`;
      return;
    }
    const stats = snapshot.stats;
    const closed = finite(stats.closed_trades) ?? 0;
    const winRate = finite(stats.win_rate);
    const winRateText = closed <= 0 ? format.percent(0) : format.percent(winRate);
    const netClass = signClass(stats.net_pnl);
    const avgR = finite(stats.avg_r);

    const cells = [
      ["Closed trades", format.count(closed), ""],
      ["Win rate", winRateText, ""],
      ["Net P&L", format.signedMoney(stats.net_pnl), netClass],
      ["Profit factor", profitFactorText(stats), ""],
      ["Avg R", avgR === null ? format.DASH : format.rMultiple(avgR), signClass(avgR)],
      ["Max drawdown", format.percent(stats.max_drawdown_pct), ""],
    ];
    body.innerHTML = `
      <dl class="perf-grid">
        ${cells.map(([label, value, cls]) => `
          <div class="perf-cell">
            <dt>${html(label)}</dt>
            <dd class="tnum ${cls}">${html(value)}</dd>
          </div>`).join("")}
      </dl>`;
  }

  // ---------- Trade history ----------

  const EXIT_REASONS = {
    stop_loss: { label: "SL", cls: "sl" },
    take_profit: { label: "TP", cls: "tp" },
    signal_flip: { label: "flip", cls: "flip" },
    stop_out: { label: "stop-out", cls: "stop-out" },
  };

  function exitChip(reason) {
    const style = EXIT_REASONS[reason] || {
      label: reason ? reason.replace(/_/g, " ") : format.DASH,
      cls: "other",
    };
    return `<span class="exit-chip is-${style.cls}">${html(style.label)}</span>`;
  }

  function renderTrades() {
    const body = $("trades-body");
    const count = $("trades-count");
    if (!body) return;
    if (!snapshot) {
      if (count) count.textContent = "0";
      body.innerHTML = `<div class="skeleton-line w70"></div><div class="empty">Loading demo trades\u2026</div>`;
      return;
    }
    const trades = snapshot.recent_trades;
    if (count) count.textContent = String(trades.length);
    if (!trades.length) {
      body.innerHTML = `<div class="empty">No closed demo trades yet.</div>`;
      return;
    }
    body.innerHTML = `
      <div class="table-scroll">
        <table class="trades-table">
          <thead>
            <tr>
              <th scope="col">#</th><th scope="col">Side</th><th scope="col">Lots</th>
              <th scope="col">Entry</th><th scope="col">Exit</th><th scope="col" class="num">P&amp;L (USD)</th>
              <th scope="col" class="num">R</th><th scope="col">Exit reason</th>
              <th scope="col">Opened</th><th scope="col">Closed</th>
            </tr>
          </thead>
          <tbody>
            ${trades.slice(0, cfg.maxTrades).map((trade) => `
              <tr>
                <td class="tnum muted">${html(trade.id == null ? format.DASH : trade.id)}</td>
                <td><span class="side-chip is-${trade.side}">${html(trade.side.toUpperCase())}</span></td>
                <td class="tnum">${html(format.lots(trade.lots))}</td>
                <td class="tnum">${html(format.price(trade.entry))}</td>
                <td class="tnum">${html(format.price(trade.exit))}</td>
                <td class="tnum num ${signClass(trade.pnl)}">${html(format.signedMoney(trade.pnl))}</td>
                <td class="tnum num ${signClass(trade.r_multiple)}">${html(format.rMultiple(trade.r_multiple))}</td>
                <td>${exitChip(trade.exit_reason)}</td>
                <td class="tnum muted">${html(format.dateTime(trade.opened_at))}</td>
                <td class="tnum muted">${html(format.dateTime(trade.closed_at))}</td>
              </tr>`).join("")}
          </tbody>
        </table>
      </div>`;
  }

  function render() {
    renderStrip();
    renderPosition();
    renderPerformance();
    renderTrades();
  }

  function tick() {
    // Freshness caption in the drawer / strip; values themselves only change
    // with a complete snapshot.
    const updated = $("demo-updated");
    if (updated) {
      updated.textContent = snapshot && snapshot.ts !== null
        ? `updated ${format.time(snapshot.ts)}`
        : format.DASH;
    }
  }

  function start() {
    if (started) return;
    started = true;
    render();
    load();
    setInterval(tick, 1000);
  }

  App.demo = {
    start,
    load,
    poll: load,
    applySnapshot,
    render,
    getState,
    onUpdate(listener) {
      if (typeof listener !== "function") return () => {};
      updateListeners.add(listener);
      return () => updateListeners.delete(listener);
    },
  };
})((window.App = window.App || {}));
