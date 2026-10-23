/* ============================================================
 * XAUUSD Terminal — Strategy panel (Node 1 built-in evaluator).
 *
 * Reads `GET /strategy` once on mount, then trusts the `strategy` WS frames.
 * State is one reducer keyed on `decision.ts`: a reconnect replay replaces
 * the matching history entry instead of duplicating it (see strategy-math).
 *
 * Read-only: the engine evaluates and trades the demo account itself. There
 * is no control here — no enable toggle, no order ticket, no sizing input.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml } = App.utils;
  const format = App.format;
  const math = App.strategyMath;
  const normalise = App.normalize;
  const cfg = App.config.strategy;

  const EMPTY_TEXT = "Waiting for the first evaluation\u2026";

  const state = {
    symbol: "XAUUSD",
    enabled: null, // unknown until /strategy or a status frame says otherwise
    params: Object.assign({}, cfg.defaultParams),
    last: null,
    history: [],
  };

  let wsRevision = 0;
  let lastFrameAt = null;
  let lastError = "";
  let fetchInProgress = false;
  let mounted = false;
  let started = false;
  const updateListeners = new Set();

  const html = (value) => escapeHtml(value);

  function finite(value) {
    return math.finite(value);
  }

  function normalizedParams(raw) {
    const params = Object.assign({}, cfg.defaultParams);
    if (normalise.isRecord(raw)) {
      for (const key of Object.keys(cfg.defaultParams)) {
        if (raw[key] !== undefined && raw[key] !== null) params[key] = raw[key];
      }
    }
    return params;
  }

  function normalizedDecision(raw) {
    if (!normalise.isRecord(raw)) return null;
    const ts = normalise.timestampMs(raw.ts);
    if (ts === null) return null;
    const reasons = Array.isArray(raw.reasons)
      ? raw.reasons.filter(normalise.isRecord).map((reason) => ({
          name: String(reason.name == null ? "" : reason.name),
          value: finite(reason.value),
          weight: finite(reason.weight),
          contribution: finite(reason.contribution),
          detail: String(reason.detail == null ? "" : reason.detail),
        }))
      : [];
    return {
      ts,
      symbol: String(raw.symbol == null ? "XAUUSD" : raw.symbol),
      action: math.actionOf(raw),
      score: finite(raw.score),
      confidence: finite(raw.confidence),
      price: finite(raw.price),
      atr: finite(raw.atr),
      regime: String(raw.regime == null ? "" : raw.regime),
      volatility: String(raw.volatility == null ? "" : raw.volatility),
      stop_loss: finite(raw.stop_loss),
      take_profit: finite(raw.take_profit),
      reward_risk: finite(raw.reward_risk),
      lots: finite(raw.lots),
      risk_usd: finite(raw.risk_usd),
      reasons,
      notes: Array.isArray(raw.notes) ? raw.notes.map((note) => String(note == null ? "" : note)).filter(Boolean) : [],
      next_event_ts: normalise.timestampMs(raw.next_event_ts),
    };
  }

  function notify() {
    for (const listener of updateListeners) {
      try { listener(getState()); } catch (error) { console.error("Strategy update listener failed:", error); }
    }
  }

  function getState() {
    return {
      symbol: state.symbol,
      enabled: state.enabled,
      params: state.params,
      last: state.last,
      history: state.history,
      lastFrameAt,
      lastError,
    };
  }

  /**
   * Replace the seed.
   *
   * The decision data is guarded by the WS revision: a REST response that
   * started before a newer `strategy` frame must not rewind the panel. The
   * evaluator's configuration (enabled + params) is not carried by decision
   * frames, so it is seeded even when the decision data is stale.
   */
  function applySnapshot(raw, source = "rest", startRevision = wsRevision) {
    const snapshot = normalise.isRecord(raw) ? raw : {};
    const stale = source === "rest" && startRevision !== wsRevision;
    state.symbol = snapshot.symbol ? String(snapshot.symbol) : state.symbol;
    if (typeof snapshot.enabled === "boolean") state.enabled = snapshot.enabled;
    state.params = normalizedParams(snapshot.params);

    if (!stale) {
      const history = Array.isArray(snapshot.history) ? snapshot.history : [];
      let merged = { last: state.last, history: state.history };
      for (const item of history) {
        const decision = normalizedDecision(item);
        if (decision) merged = math.mergeDecision(merged, decision, cfg.historyLimit);
      }
      const last = normalizedDecision(snapshot.last);
      if (last) merged = math.mergeDecision(merged, last, cfg.historyLimit);
      state.last = merged.last;
      state.history = merged.history;
    }

    render();
    notify();
    return !stale;
  }

  /** One `{type:"strategy", data: StrategyDecision}` frame. */
  function applyDecision(raw, source = "ws") {
    const decision = normalizedDecision(raw);
    if (!decision) return false;
    if (source === "ws") {
      wsRevision += 1;
      lastFrameAt = Date.now();
      lastError = "";
    }
    const merged = math.mergeDecision(state, decision, cfg.historyLimit);
    state.last = merged.last;
    state.history = merged.history;
    render();
    notify();
    return true;
  }

  async function load() {
    if (fetchInProgress) return false;
    fetchInProgress = true;
    const startRevision = wsRevision;
    try {
      const snapshot = await App.api.fetchStrategy();
      return applySnapshot(snapshot, "rest", startRevision);
    } catch (error) {
      lastError = error && error.message ? error.message : "strategy snapshot unavailable";
      console.warn("Strategy snapshot unavailable:", error);
      if (!state.last && mounted) render();
      return false;
    } finally {
      fetchInProgress = false;
    }
  }

  // ---------- Rendering ----------

  function actionBadge(decision) {
    const action = math.actionOf(decision);
    return `<span class="action-badge is-${action}">${html(action.toUpperCase())}</span>`;
  }

  function scoreHtml(decision) {
    const bar = math.scoreBar(decision.score);
    const label = bar.sign === "bull" ? "bullish" : bar.sign === "bear" ? "bearish" : "neutral";
    const style = bar.sign === "zero"
      ? ""
      : `width:${bar.widthPct.toFixed(2)}%;left:${(bar.sign === "bull" ? 50 : 50 - bar.widthPct).toFixed(2)}%`;
    return `
      <div class="score-block">
        <div class="score-row">
          <span class="muted">Score</span>
          <b class="score-value is-${bar.sign}">${html(format.signedNumber(bar.value, 2))}</b>
        </div>
        <div class="score-track" role="img" aria-label="Score ${html(format.signedNumber(bar.value, 2))}, ${label}">
          <span class="score-zero" aria-hidden="true"></span>
          ${bar.sign === "zero" ? "" : `<span class="score-fill is-${bar.sign}" style="${style}"></span>`}
        </div>
        <div class="score-scale" aria-hidden="true"><span>\u22121</span><span>0</span><span>+1</span></div>
      </div>`;
  }

  function confidenceHtml(decision) {
    const pct = math.confidencePct(decision.confidence);
    const threshold = math.confidencePct(state.params.min_confidence);
    const safePct = pct === null ? 0 : pct;
    const safeThreshold = threshold === null ? null : threshold;
    return `
      <div class="gauge-block">
        <div class="meter" role="meter" aria-label="Confidence" aria-valuemin="0" aria-valuemax="100"
             aria-valuenow="${safePct.toFixed(1)}" aria-valuetext="${html(format.confidence(decision.confidence))}">
          <span class="meter-fill" style="width:${safePct.toFixed(2)}%"></span>
          ${safeThreshold === null ? "" : `<span class="meter-tick" style="left:${math.clamp(safeThreshold, 0, 100).toFixed(2)}%" title="Minimum confidence ${html(format.percent(safeThreshold))}"></span>`}
        </div>
        <div class="meter-legend">
          <span>Confidence <b>${html(format.confidence(decision.confidence))}</b></span>
          <span class="muted">min ${html(safeThreshold === null ? format.DASH : format.percent(safeThreshold))}</span>
        </div>
      </div>`;
  }

  function chipsHtml(decision) {
    const regime = String(decision.regime || "");
    const volatility = String(decision.volatility || "");
    return `
      <div class="strat-chips">
        <span class="chip-static regime-${html(regime)}"><span class="chip-k">Regime</span>${html(math.regimeLabel(regime))}</span>
        <span class="chip-static vol-${html(volatility)}"><span class="chip-k">Vol</span>${html(math.volatilityLabel(volatility))}</span>
      </div>`;
  }

  function reasonsHtml(decision) {
    const reasons = math.sortReasons(decision.reasons);
    if (!reasons.length) {
      return `<div class="subhead">Reasons</div><div class="empty inline">No factor breakdown in this decision.</div>`;
    }
    const rows = reasons.map((reason) => {
      const value = finite(reason.value) ?? 0;
      const clamped = math.clamp(value, -1, 1);
      const width = Math.abs(clamped) * 50;
      const left = clamped >= 0 ? 50 : 50 - width;
      const weight = finite(reason.weight);
      const contribution = finite(reason.contribution);
      const weightText = weight === null ? format.DASH : format.percent(weight * 100, 1);
      const valueText = format.signedNumber(value, 2);
      const title = `${math.factorLabel(reason.name)}: value ${valueText}, weight ${weightText}, contribution ${format.signedNumber(contribution, 2)}` +
        (reason.detail ? ` \u2014 ${reason.detail}` : "");
      return `
        <div class="reason" title="${html(title)}">
          <div class="reason-head">
            <span class="reason-name">${html(math.factorLabel(reason.name))}</span>
            <span class="reason-val is-${clamped > 0 ? "bull" : clamped < 0 ? "bear" : "flat"}">${html(valueText)}</span>
            <span class="reason-weight">${html(weightText)}</span>
          </div>
          <div class="reason-bar" role="img" aria-label="${html(title)}">
            <span class="reason-zero" aria-hidden="true"></span>
            <span class="reason-fill is-${clamped > 0 ? "bull" : clamped < 0 ? "bear" : "flat"}"
                  style="width:${width.toFixed(2)}%;left:${left.toFixed(2)}%"></span>
          </div>
          ${reason.detail ? `<div class="reason-detail">${html(reason.detail)}</div>` : ""}
        </div>`;
    });
    return `<div class="subhead">Reasons <span class="muted">by contribution</span></div><div class="reasons">${rows.join("")}</div>`;
  }

  function eventText(ts) {
    const remaining = finite(ts) - Date.now();
    if (remaining <= 0) return "";
    const minutes = Math.ceil(remaining / 60000);
    return `event in ${minutes} min \u2014 standing aside`;
  }

  function notesHtml(decision) {
    const flat = math.actionOf(decision) === "flat";
    const notes = decision.notes.slice();
    const nextTs = finite(decision.next_event_ts);
    const event = nextTs !== null && nextTs > Date.now() ? eventText(nextTs) : "";
    if (!notes.length && !event) return "";
    return `
      <div class="notes${flat ? " is-flat" : ""}" aria-label="Decision notes">
        ${notes.map((note) => `<div class="note">${html(note)}</div>`).join("")}
        ${event ? `<div class="note" id="strat-event" data-ts="${nextTs}">${html(event)}</div>` : ""}
      </div>`;
  }

  function historyHtml() {
    const items = state.history.slice(-cfg.stripSize);
    if (!items.length) {
      return `<div class="subhead">Recent decisions</div><div class="empty inline">No decisions yet.</div>`;
    }
    const width = 300;
    const height = 42;
    const slot = width / cfg.stripSize;
    // Newest decision sits at the right edge, so a short history grows leftward
    // instead of being stranded in the first pixels of the strip.
    const offset = width - items.length * slot;
    const bars = [];
    const points = [];
    items.forEach((decision, index) => {
      const action = math.actionOf(decision);
      const ratio = math.confidencePct(decision.confidence);
      const conf = ratio === null ? 0 : ratio / 100;
      const barHeight = Math.max(3, conf * 26);
      const x = offset + index * slot + 1.5;
      const y = height - 4 - barHeight;
      bars.push(
        `<rect class="hist-${action}" x="${x.toFixed(2)}" y="${y.toFixed(2)}" width="${Math.max(2, slot - 3).toFixed(2)}" height="${barHeight.toFixed(2)}">` +
        `<title>${html(`${format.time(decision.ts)} \u00b7 ${action.toUpperCase()} \u00b7 ${format.confidence(decision.confidence)}`)}</title></rect>`
      );
      points.push(`${(offset + index * slot + slot / 2).toFixed(2)},${y.toFixed(2)}`);
    });
    return `
      <div class="subhead">Recent decisions <span class="muted">last ${items.length}</span></div>
      <svg class="history-svg" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none"
           role="img" aria-label="${items.length} recent decisions: ${html(items.map((d) => math.actionOf(d)).join(", "))}">
        ${bars.join("")}
        <polyline class="hist-line" points="${points.join(" ")}" fill="none"></polyline>
      </svg>
      <div class="hist-legend">
        <span><i class="hist-dot is-buy"></i>Buy</span>
        <span><i class="hist-dot is-sell"></i>Sell</span>
        <span><i class="hist-dot is-flat"></i>Flat</span>
        <span class="muted">line = confidence</span>
      </div>`;
  }

  function skeletonHtml(message) {
    return `
      <div class="strat-skeleton" aria-busy="true">
        <div class="skeleton-line w40"></div>
        <div class="skeleton-line w70"></div>
        <div class="skeleton-line w55"></div>
      </div>
      <div class="empty">${html(message)}</div>`;
  }

  function render() {
    const body = $("strategy-body");
    if (!body) return;
    mounted = true;

    if (!state.last) {
      const failed = lastError && !fetchInProgress;
      body.innerHTML = skeletonHtml(failed ? "Strategy snapshot unavailable \u2014 waiting for the stream\u2026" : EMPTY_TEXT);
      paintStatus();
      return;
    }

    const decision = state.last;
    const action = math.actionOf(decision);
    const lots = finite(decision.lots);
    const risk = finite(decision.risk_usd);
    const engine = state.enabled === true
      ? `<span class="engine-chip is-on" title="Built-in strategy evaluator enabled">ENGINE ON</span>`
      : state.enabled === false
        ? `<span class="engine-chip is-off" title="Built-in strategy evaluator disabled">ENGINE OFF</span>`
        : `<span class="engine-chip" title="Evaluator state not reported yet">ENGINE ?</span>`;

    body.innerHTML = `
      <div class="strat-top">
        ${actionBadge(decision)}
        <span class="strat-symbol">${html(decision.symbol || state.symbol)}</span>
        ${engine}
        <span class="strat-age" id="strat-age" title="Decision timestamp">${html(format.age(decision.ts))}</span>
      </div>
      ${chipsHtml(decision)}
      ${confidenceHtml(decision)}
      ${scoreHtml(decision)}
      <dl class="kv-grid">
        <div><dt>Price</dt><dd class="tnum">${html(format.price(decision.price))}</dd></div>
        <div><dt>ATR</dt><dd class="tnum">${html(format.price(decision.atr))}</dd></div>
        <div><dt>Stop</dt><dd class="tnum">${html(format.price(decision.stop_loss))}</dd></div>
        <div><dt>Target</dt><dd class="tnum">${html(format.price(decision.take_profit))}</dd></div>
        <div class="kv-wide"><dt>Size</dt><dd class="tnum">${lots !== null && lots > 0
          ? `${html(format.lots(lots))} lots <span class="muted">\u00b7 ${html(format.money(risk))} at risk</span>`
          : `<span class="muted">no position \u2014 flat</span>`}</dd></div>
      </dl>
      ${reasonsHtml(decision)}
      ${notesHtml(decision)}
      ${historyHtml()}
      <div class="strat-conn" id="strat-conn" hidden></div>`;

    paintStatus();
    tick();
  }

  function socketLabel() {
    const socketState = App.socket && App.socket.getState ? App.socket.getState() : null;
    const polling = !!(App.runtime && App.runtime.polling);
    if (polling) return { text: "DELAYED", tone: "warn" };
    if (!socketState) return { text: "OFFLINE", tone: "bad" };
    if (socketState.status === "live") return { text: "LIVE", tone: "live" };
    if (socketState.status === "stale") return { text: "STALE", tone: "warn" };
    if (socketState.status === "connecting") return { text: "CONNECTING", tone: "warn" };
    if (socketState.status === "not-configured") return { text: "NOT CONFIGURED", tone: "bad" };
    return { text: "RECONNECTING", tone: "warn" };
  }

  /** Panel header state + stale dimming. Cheap enough to run every second. */
  function paintStatus() {
    const panel = $("panel-strategy");
    const stateEl = $("strategy-state");
    const label = socketLabel();
    if (stateEl) {
      stateEl.textContent = label.text;
      stateEl.dataset.tone = label.tone;
    }
    const decisionTs = finite(state.last && state.last.ts);
    const stale = decisionTs !== null && math.isStale(decisionTs, Date.now(), cfg.staleAfterMs);
    if (panel && panel.classList) panel.classList.toggle("is-stale", stale);

    const conn = $("strat-conn");
    if (!conn) return;
    let message = "";
    if (App.runtime && App.runtime.polling) {
      message = "Socket down \u2014 polling /strategy every 5 s (delayed).";
    } else if (stale && label.text !== "LIVE") {
      message = `No evaluation for ${format.age(decisionTs).replace(" ago", "")} \u00b7 stream ${label.text.toLowerCase()}.`;
    } else if (label.tone === "bad") {
      message = `Stream ${label.text.toLowerCase()} \u2014 showing the last received decision.`;
    }
    conn.hidden = !message;
    conn.textContent = message;
  }

  /** Per-second light updates: age, staleness, connection line, countdown. */
  function tick() {
    const ageEl = $("strat-age");
    const decisionTs = finite(state.last && state.last.ts);
    if (ageEl) {
      ageEl.textContent = decisionTs === null ? format.DASH : format.age(decisionTs);
      if (ageEl.classList) ageEl.classList.toggle("is-stale", decisionTs !== null && math.isStale(decisionTs, Date.now(), cfg.staleAfterMs));
    }
    const eventEl = $("strat-event");
    if (eventEl) {
      const nextTs = finite(eventEl.dataset && eventEl.dataset.ts);
      if (nextTs !== null) {
        const text = eventText(nextTs);
        if (!text) {
          eventEl.hidden = true;
          const container = eventEl.parentNode;
          if (container && container.querySelectorAll) {
            const otherNotes = Array.from(container.querySelectorAll(".note"))
              .some((node) => node !== eventEl && !node.hidden && node.textContent.trim());
            if (!otherNotes) container.hidden = true;
          }
        } else {
          eventEl.textContent = text;
        }
      }
    }
    paintStatus();
  }

  function start() {
    if (started) return;
    started = true;
    render();
    load();
    setInterval(tick, 1000);
  }

  App.strategy = {
    start,
    load,
    poll: load,
    applySnapshot,
    applyDecision,
    render,
    tick,
    getState,
    onUpdate(listener) {
      if (typeof listener !== "function") return () => {};
      updateListeners.add(listener);
      return () => updateListeners.delete(listener);
    },
  };
})((window.App = window.App || {}));
