/* ============================================================
 * XAUUSD Terminal — Node 4 execution snapshot (read-only).
 *
 * Renders the two additive sections of `ExecutionSnapshot`:
 *
 *   market_feed — Node 1 consumer health (`trading_ready` is the gate)
 *   strategy    — the local `vp_break_retest_v1` runner (local / monitor)
 *
 * plus the venue this service executes on, the Node 3 intent link and the
 * feed-integrity counters. All of it is read-only: there is no control, no
 * order action and no strategy maths in this file.
 *
 * Transports (identical payloads, identical rendering):
 *   GET  /diagnostics -> bare ExecutionSnapshot
 *   WS   /ws          -> {"type":"diagnostics","data":ExecutionSnapshot}
 *
 * Every diagnostics frame is a COMPLETE snapshot and replaces the previous
 * one wholesale. Nothing here merges, because a merged view would hide a
 * reset (reconnect, resync, cleared counters) from the operator.
 *
 * Colour rules, pinned by the Node 4 handoff:
 *   - readiness and colour follow `market_feed.trading_ready`, never `state`
 *     (`state` can read "live" while the gate is closed);
 *   - `not_ready_reason` is rendered verbatim — the backend's own words;
 *   - `strategy.mode !== "local"` labels both cards "monitor only";
 *   - in local mode the Node 3 link is "not used", never an error.
 * ============================================================ */
(function (App) {
  "use strict";

  const normalise = App.normalize;
  const format = App.format;
  const { $, escapeHtml } = App.utils;
  const cfg = App.config.execution;

  const html = (value) => escapeHtml(value);
  const own = normalise.own;
  const isRecord = normalise.isRecord;
  const DASH = format.DASH;

  const str = (value) => (typeof value === "string" && value.trim() ? value : null);
  const bool = (value) => (value === true ? true : value === false ? false : null);
  const num = (value) => normalise.numberOrNull(value);
  const whole = (value) => {
    const parsed = num(value);
    return parsed === null ? null : Math.trunc(parsed);
  };
  const stamp = (value) => normalise.timestampMs(value);

  // ---------------------------------------------------------------
  // Small formatting helpers (every displayed value stays here or in format)
  // ---------------------------------------------------------------

  /** Integer when it is one, up to two decimals when it is not. */
  function compact(value) {
    const parsed = num(value);
    if (parsed === null) return DASH;
    return Number.isInteger(parsed) ? format.number(parsed, 0) : format.number(parsed, 2);
  }

  function countText(value) {
    const parsed = whole(value);
    return parsed === null ? DASH : format.count(parsed);
  }

  /** 900 -> "0.9 s", 94000 -> "94 s", 300000 -> "5.0 min". */
  function duration(value) {
    const parsed = num(value);
    if (parsed === null || parsed < 0) return DASH;
    if (parsed < 1000) return `${Math.round(parsed)} ms`;
    if (parsed < 10000) return `${(parsed / 1000).toFixed(1)} s`;
    if (parsed < 60000) return `${Math.round(parsed / 1000)} s`;
    if (parsed < 3600000) return `${(parsed / 60000).toFixed(1)} min`;
    return `${(parsed / 3600000).toFixed(1)} h`;
  }

  function yesNo(value) {
    if (value === true) return "yes";
    if (value === false) return "no";
    return DASH;
  }

  function relative(value) {
    const at = stamp(value);
    return at === null ? DASH : format.age(at);
  }

  function band(min, max, unit, digits = null) {
    const low = num(min);
    const high = num(max);
    if (low === null && high === null) return DASH;
    const show = (value) => (digits === null ? compact(value) : format.number(value, digits));
    const suffix = unit ? ` ${unit}` : "";
    if (low !== null && high !== null && low === high) return `${show(low)}${suffix}`;
    return `${low === null ? DASH : show(low)}\u2013${high === null ? DASH : show(high)}${suffix}`;
  }

  // ---------------------------------------------------------------
  // Normalisation — a bounded, replace-only view model
  // ---------------------------------------------------------------

  function normaliseFeed(raw) {
    if (!isRecord(raw)) return null;
    return {
      enabled: bool(raw.enabled),
      state: str(raw.state),
      trading_ready: bool(raw.trading_ready),
      not_ready_reason: str(raw.not_ready_reason),
      url: str(raw.url),
      auth: str(raw.auth),
      contract_version: str(raw.contract_version),
      contract_required: str(raw.contract_required),
      symbol: str(raw.symbol),
      interval: str(raw.interval),
      connected: bool(raw.connected),
      last_frame_ms: stamp(raw.last_frame_ms),
      last_status_ms: stamp(raw.last_status_ms),
      last_candle_time: stamp(raw.last_candle_time),
      last_closed_candle_time: stamp(raw.last_closed_candle_time),
      last_levels_ts: stamp(raw.last_levels_ts),
      last_seq: whole(raw.last_seq),
      session_closed_candles: whole(raw.session_closed_candles),
      sync_complete: bool(raw.sync_complete),
      contract_ok: bool(raw.contract_ok),
      freshness_state: str(raw.freshness_state),
      freshness_age_ms: num(raw.freshness_age_ms),
      market_open: bool(raw.market_open),
      reconnect_count: whole(raw.reconnect_count),
      frames_received: whole(raw.frames_received),
      candles_received: whole(raw.candles_received),
      closed_candles: whole(raw.closed_candles),
      levels_received: whole(raw.levels_received),
      status_frames: whole(raw.status_frames),
      duplicates: whole(raw.duplicates),
      out_of_order: whole(raw.out_of_order),
      gaps: whole(raw.gaps),
      contract_violations: whole(raw.contract_violations),
      resyncs: whole(raw.resyncs),
      last_error: str(raw.last_error),
      last_event_ms: stamp(raw.last_event_ms),
    };
  }

  function normaliseStrategy(raw) {
    if (!isRecord(raw)) return null;
    return {
      enabled: bool(raw.enabled),
      mode: str(raw.mode),
      strategy_version: str(raw.strategy_version),
      symbol: str(raw.symbol),
      interval: str(raw.interval),
      volume_threshold: num(raw.volume_threshold),
      sl_min_pips: num(raw.sl_min_pips),
      sl_max_pips: num(raw.sl_max_pips),
      tp_min_pips: num(raw.tp_min_pips),
      tp_max_pips: num(raw.tp_max_pips),
      rr_min: num(raw.rr_min),
      rr_max: num(raw.rr_max),
      ttl_secs: whole(raw.ttl_secs),
      levels_tracked: whole(raw.levels_tracked),
      candles_observed: whole(raw.candles_observed),
      last_candle_time: stamp(raw.last_candle_time),
      atr: num(raw.atr),
      last_decision_at_ms: stamp(raw.last_decision_at_ms),
      last_action: str(raw.last_action),
      last_trigger_level: str(raw.last_trigger_level),
      last_intent_id: str(raw.last_intent_id),
      intents_emitted: whole(raw.intents_emitted),
      intents_suppressed: whole(raw.intents_suppressed),
      last_suppression_reason: str(raw.last_suppression_reason),
      last_error: str(raw.last_error),
      updated_at: stamp(raw.updated_at),
      breaks_detected: whole(raw.breaks_detected),
      breaks_invalidated: whole(raw.breaks_invalidated),
      retests_rejected_low_volume: whole(raw.retests_rejected_low_volume),
      signals_confirmed: whole(raw.signals_confirmed),
    };
  }

  function normaliseWork(raw) {
    if (!isRecord(raw)) return null;
    return {
      service: str(raw.service),
      version: str(raw.version),
      role: str(raw.role),
      venue: str(raw.venue),
      started_at: stamp(raw.started_at),
      uptime_secs: whole(raw.uptime_secs),
      intents_received: whole(raw.intents_received),
      intents_accepted: whole(raw.intents_accepted),
      intents_rejected: whole(raw.intents_rejected),
      intents_expired: whole(raw.intents_expired),
      intents_duplicate: whole(raw.intents_duplicate),
      orders_placed: whole(raw.orders_placed),
      orders_filled: whole(raw.orders_filled),
      orders_rejected: whole(raw.orders_rejected),
      orders_unknown: whole(raw.orders_unknown),
      last_intent_ts: stamp(raw.last_intent_ts),
      last_error: str(raw.last_error),
    };
  }

  function normaliseNode3(raw) {
    if (!isRecord(raw)) return null;
    return {
      url: str(raw.url),
      token_configured: bool(raw.token_configured),
      protocol_version: whole(raw.protocol_version),
      connected: bool(raw.connected),
      authenticated: bool(raw.authenticated),
      state: str(raw.state),
      session_age_secs: whole(raw.session_age_secs),
      last_message_ts: stamp(raw.last_message_ts),
      last_hello_ack_ts: stamp(raw.last_hello_ack_ts),
      last_report_ts: stamp(raw.last_report_ts),
      reconnect_count: whole(raw.reconnect_count),
      frames_received: whole(raw.frames_received),
      reports_sent: whole(raw.reports_sent),
      reports_acked: whole(raw.reports_acked),
      last_error: str(raw.last_error),
    };
  }

  /**
   * `{type:"diagnostics",data:{…}}` frames, `{data:{…}}` wrappers and bare
   * REST bodies all normalise to the same shape. Anything else is ignored.
   * There is deliberately no merge with the previous snapshot.
   *
   * A Node 4 snapshot is recognised by the sections only Node 4 sends, not by
   * a bare `timestamp`: a different service answering `/diagnostics` must not
   * look like a build that forgot `market_feed`.
   *
   * `sections` records which additive keys this particular response carried.
   * Node 4 >= 7f6f071 always sends both, so a `false` here is a build or
   * transport fact about the service, not a reason to guess at the data.
   */
  function normaliseSnapshot(raw) {
    const record = normalise.unwrapFrame(raw);
    if (!isRecord(record)) return null;
    const identifying = ["market_feed", "strategy", "work", "node3", "open_trades", "mt5", "recent_events", "deriv_account"];
    if (!identifying.some((key) => own(record, key))) return null;
    return {
      timestamp: stamp(record.timestamp),
      sections: {
        market_feed: own(record, "market_feed"),
        strategy: own(record, "strategy"),
      },
      market_feed: normaliseFeed(record.market_feed),
      strategy: normaliseStrategy(record.strategy),
      work: normaliseWork(record.work),
      node3: normaliseNode3(record.node3),
    };
  }

  // ---------------------------------------------------------------
  // Derived state (pure decisions — no indicators, no strategy maths)
  // ---------------------------------------------------------------

  function modeOf(strategy) {
    return strategy ? strategy.mode : null;
  }

  function isLocal(strategy) {
    return modeOf(strategy) === "local";
  }

  function modeLabel(strategy) {
    const mode = modeOf(strategy);
    if (!mode) return null;
    if (mode === "local") return "LOCAL";
    return "MONITOR ONLY";
  }

  /**
   * Readiness comes from `trading_ready` alone. `state` is explanatory and is
   * only consulted to escalate a feed that is actively violating the contract
   * (`contract_mismatch` / `contract_violation`), never to declare health.
   */
  function describeFeed(feed) {
    if (!feed) {
      return { available: false, tone: "muted", headline: "Market data unavailable", ready: null, reason: null };
    }
    if (feed.enabled === false) {
      return {
        available: true,
        tone: "muted",
        headline: "Feed disabled",
        ready: false,
        reason: feed.not_ready_reason,
      };
    }
    if (feed.trading_ready === true) {
      return { available: true, tone: "live", headline: "Market data ready", ready: true, reason: null };
    }
    const contractProblem = feed.state === "contract_mismatch" || feed.state === "contract_violation";
    const down = feed.connected !== true || contractProblem;
    return {
      available: true,
      tone: down ? "bad" : "warn",
      headline: "Market data not ready",
      ready: false,
      reason: feed.not_ready_reason,
    };
  }

  function describeLink(node3, strategy) {
    if (isLocal(strategy)) {
      return {
        tone: "muted",
        headline: "Not used \u2014 local strategy",
        detail: "Node 4 is not dialling Node 3 in local mode. No intent is expected from that link.",
      };
    }
    if (!node3) {
      return {
        tone: "muted",
        headline: "Node 3 link unavailable",
        detail: "This Node 4 snapshot does not include the node3 block.",
      };
    }
    const suffix = node3.last_error ? ` Last error: ${node3.last_error}` : "";
    if (node3.connected === true) {
      return {
        tone: "live",
        headline: node3.authenticated === false ? "Connected \u2014 not authenticated" : "Connected",
        detail: `Intent link ${node3.state || "connected"}.${suffix}`,
      };
    }
    if (modeOf(strategy) === "node3") {
      return {
        tone: "bad",
        headline: "Not connected",
        detail: `Node 3 is the intent source and the link is ${node3.state || "down"}.${suffix}`,
      };
    }
    return {
      tone: "muted",
      headline: `Link ${node3.state || "idle"}`,
      detail: `Node 3 is not the intent source.${suffix}`,
    };
  }

  // ---------------------------------------------------------------
  // Store
  // ---------------------------------------------------------------

  const state = {
    snapshot: null,   // {timestamp, market_feed, strategy, work, node3}
    receivedAt: null,
    source: null,     // "rest" | "ws"
    lastRestError: null,
    lastStreamError: null,
  };

  function getState() {
    return {
      snapshot: state.snapshot,
      sections: state.snapshot ? state.snapshot.sections : null,
      receivedAt: state.receivedAt,
      source: state.source,
      phase,
      stream: streamView(),
      lastRestError: state.lastRestError,
      lastStreamError: state.lastStreamError,
    };
  }

  /**
   * Replace the snapshot. An older REST seed never overwrites a newer frame;
   * two frames with the same timestamp are still a full replacement.
   */
  function apply(raw, source = "ws") {
    const next = normaliseSnapshot(raw);
    if (!next) return false;
    const current = state.snapshot;
    if (source === "rest" && current && current.timestamp !== null && next.timestamp !== null &&
        next.timestamp < current.timestamp) {
      return false;
    }
    state.snapshot = next;
    state.receivedAt = Date.now();
    state.source = source;
    if (source === "ws") state.lastStreamError = null;
    render();
    return true;
  }

  // ---------------------------------------------------------------
  // Rendering
  // ---------------------------------------------------------------

  const painted = new Map();

  function paint(id, markup) {
    const el = $(id);
    if (!el) return;
    if (painted.get(id) === markup) return;
    painted.set(id, markup);
    el.innerHTML = markup;
  }

  function paintState(id, label, tone) {
    const el = $(id);
    if (!el) return;
    const key = `state:${id}`;
    const signature = `${label}|${tone || ""}`;
    if (painted.get(key) === signature) return;
    painted.set(key, signature);
    el.textContent = label;
    if (el.dataset) el.dataset.tone = tone || "";
  }

  function statusRow(tone, headline, extra = "") {
    return `
      <div class="exec-status" data-tone="${html(tone)}">
        <span class="exec-dot" aria-hidden="true"></span>
        <b>${html(headline)}</b>${extra}
      </div>`;
  }

  /**
   * "monitor only" is asserted from a reported mode, never from its absence:
   * a snapshot without `strategy` gets an "unavailable" card, not a guess.
   */
  function monitorFlag(strategy) {
    if (!strategy || modeOf(strategy) === null) return "";
    return isLocal(strategy) ? "" : `<span class="exec-flag">monitor only</span>`;
  }

  function cell(label, value, tone = "", cls = "") {
    const text = value === null || value === undefined || value === "" ? DASH : String(value);
    return `<div${cls ? ` class="${html(cls)}"` : ""}><dt>${html(label)}</dt><dd class="tnum ${tone}">${html(text)}</dd></div>`;
  }

  function note(text, tone = "") {
    return `<p class="exec-note ${tone}">${html(text)}</p>`;
  }

  function unavailable(headline, detail) {
    return `${statusRow("muted", headline)}${note(detail)}`;
  }

  function streamShort() {
    if (!cfg || !cfg.diagnostics) return "not configured";
    if (phase === "live") return "stream live";
    if (phase === "connecting") return "connecting";
    if (phase === "reconnecting") return "polling (reconnecting)";
    return "polling";
  }

  function streamText() {
    if (!cfg || !cfg.diagnostics) return "Node 4 is not configured. Set window.XAUUSD_CONFIG.NODE4_HOST to read /diagnostics.";
    if (phase === "live") return "Diagnostics stream connected.";
    if (phase === "connecting") return "Connecting to the Node 4 diagnostics stream\u2026";
    if (phase === "reconnecting") return "Diagnostics stream reconnecting \u2014 /diagnostics is polled every 5 s meanwhile.";
    return "Waiting for Node 4 diagnostics \u2014 /diagnostics is polled every 5 s.";
  }

  /**
   * Before the first snapshot every card says the same calm thing. A failed
   * request is named; a pending one is not dressed up as an error.
   */
  /** The endpoint an operator can curl to see the same bytes. */
  function endpointText() {
    return (cfg && cfg.diagnostics) || "GET /diagnostics";
  }

  function buildLine(snapshot) {
    const work = snapshot ? snapshot.work : null;
    if (!work) return "";
    const parts = [work.service, work.version, work.role].filter(Boolean);
    if (!parts.length) return "";
    const sections = snapshot.sections
      ? [
        `market_feed ${snapshot.sections.market_feed ? "present" : "missing"}`,
        `strategy ${snapshot.sections.strategy ? "present" : "missing"}`,
      ].join(" \u00b7 ")
      : "";
    return `<div class="exec-meta">Node 4 build ${html(parts.join(" \u00b7 "))}${sections ? ` \u00b7 ${html(sections)}` : ""}</div>`;
  }

  /**
   * A snapshot without one of the additive sections is never a data problem
   * the UI can paper over. Node 4 always serialises both, so the card explains
   * which layer is wrong instead of showing an empty panel.
   */
  function missingSection(snapshot, which) {
    const other = which === "market_feed" ? "strategy" : "market_feed";
    const label = which === "market_feed" ? "market_feed" : "strategy";
    const title = which === "market_feed" ? "Market feed state unavailable" : "Local strategy state unavailable";
    const endpoint = `<code class="exec-id">${html(endpointText())}</code>`;
    const work = snapshot ? snapshot.work : null;
    const build = work && (work.service || work.version)
      ? ` Reported build: ${html([work.service, work.version, work.role && `role ${work.role}`].filter(Boolean).join(" \u00b7 "))}.`
      : "";
    const bothMissing = !snapshot || !snapshot.sections || !snapshot.sections[other];
    const detail = bothMissing
      ? `${endpoint} returned a snapshot with neither market_feed nor strategy. Node 4 from `
        + "7f6f071 (PR #44) always serialises both, even in node3 mode, so this response comes from "
        + `an older build \u2014 redeploy Node 4 from the Node-4 branch.${build}`
      : `This snapshot has ${html(other)} but no ${html(label)}. Node 4 serialises both sections `
        + "together, so either the build is older than 7f6f071 (PR #44) or something between the "
        + `browser and ${endpoint} is dropping fields.${build}`;
    return `${statusRow("warn", title)}<p class="exec-note">${detail}</p>`;
  }

  function noSnapshot() {
    if (!cfg || !cfg.diagnostics) {
      return unavailable("Node 4 not configured",
        "Set window.XAUUSD_CONFIG.NODE4_HOST to the execution service origin to read /diagnostics.");
    }
    if (state.lastRestError) {
      return unavailable("Node 4 diagnostics unavailable", `${state.lastRestError}. ${streamText()}`);
    }
    return `${statusRow("muted", "Waiting for Node 4 diagnostics")}${note(streamText())}`;
  }

  function integrityChips(feed) {
    const items = [
      { key: "duplicates", label: "duplicates", value: feed.duplicates, title: "Duplicate frames dropped." },
      { key: "gaps", label: "gaps", value: feed.gaps, title: "Sequence gaps / gap frames." },
      { key: "out_of_order", label: "out of order", value: feed.out_of_order, title: "Frames older than the accepted sequence." },
      { key: "contract_violations", label: "contract violations", value: feed.contract_violations, title: "Malformed or forbidden frames \u2014 the draft feed contract is being violated.", alarm: true },
      { key: "resyncs", label: "resyncs", value: feed.resyncs, title: "Forced resyncs (reconnects)." },
    ];
    if (items.every((item) => item.value === null)) {
      return note("Feed-integrity counters are not reported in this snapshot.");
    }
    return `<div class="integrity" aria-label="Feed integrity since start">${items.map((item) => {
      const bad = item.alarm === true && item.value !== null && item.value > 0;
      return `<span class="integrity-chip${bad ? " is-bad" : ""}" title="${html(item.title)}">${html(item.label)} <b>${html(countText(item.value))}</b></span>`;
    }).join("")}</div>`;
  }

  function feedMarkup(feed, strategy, snapshot) {
    if (!feed) return missingSection(snapshot, "market_feed");
    const view = describeFeed(feed);
    const reason = view.ready === false
      ? (view.reason
        ? `<p class="exec-reason" data-tone="${html(view.tone)}">${html(view.reason)}</p>`
        : note("Node 4 reported no not_ready_reason.", "is-warn"))
      : "";
    const contract = feed.contract_version === null
      ? `\u2014 (requires ${feed.contract_required || DASH})`
      : `${feed.contract_version} (requires ${feed.contract_required || DASH})`;
    const freshness = feed.freshness_state === null
      ? DASH
      : `${feed.freshness_state}${feed.freshness_age_ms === null ? "" : ` \u00b7 ${duration(feed.freshness_age_ms)}`}`;
    const contractTone = feed.contract_ok === true ? "is-live" : feed.contract_ok === false ? "is-bad" : "";
    const sync = feed.sync_complete === true ? "complete" : feed.sync_complete === false ? "in progress" : DASH;
    const meta = [
      feed.symbol || DASH,
      feed.interval || DASH,
      feed.url || null,
      feed.auth ? `auth ${feed.auth}` : null,
    ].filter(Boolean).map((part) => html(part)).join(" \u00b7 ");

    return `
      ${statusRow(view.tone, view.headline, monitorFlag(strategy))}
      ${reason}
      <dl class="kv-grid">
        ${cell("Transport", feed.state ? feed.state : DASH)}
        ${cell("Connected", yesNo(feed.connected))}
        ${cell("Contract", contract, contractTone)}
        ${cell("Freshness", freshness)}
        ${cell("Market open", yesNo(feed.market_open))}
        ${cell("Sync", sync)}
        ${cell("Last closed bar", format.dateTime(feed.last_closed_candle_time))}
        ${cell("Last frame", relative(feed.last_frame_ms))}
        ${cell("Session closed candles", countText(feed.session_closed_candles))}
        ${cell("Last seq", countText(feed.last_seq))}
      </dl>
      <div class="exec-meta" title="Node 4's outbound feed URL \u2014 the browser never connects to it.">${meta}</div>
      <div class="subhead"><span>Feed integrity</span><span class="muted">since start</span></div>
      ${integrityChips(feed)}
      ${feed.last_error ? note(`Last feed error: ${feed.last_error}`, "is-bad") : ""}`;
  }

  function actionChip(action) {
    const value = action || null;
    if (value === "buy" || value === "sell" || value === "flat") {
      return `<span class="action-badge is-${value}">${html(value.toUpperCase())}</span>`;
    }
    return `<span class="action-badge is-unknown">${html(value ? value.toUpperCase() : DASH)}</span>`;
  }

  function strategyMarkup(strategy, snapshot) {
    if (!strategy) return missingSection(snapshot, "strategy");
    const counters = [
      ["breaks detected", strategy.breaks_detected, "Breakouts detected by the state machine."],
      ["breaks invalidated", strategy.breaks_invalidated, "Breaks invalidated before a retest."],
      ["low-volume retests", strategy.retests_rejected_low_volume, "Retests rejected below the tick-volume threshold."],
      ["signals confirmed", strategy.signals_confirmed, "Completed break-retest signals."],
    ];
    const counterHtml = counters.every(([, value]) => value === null)
      ? note("State-machine counters are not reported in this snapshot.")
      : `<div class="integrity" aria-label="Strategy state machine since start">${counters.map(([label, value, title]) =>
        `<span class="integrity-chip" title="${html(title)}">${html(label)} <b>${html(countText(value))}</b></span>`).join("")}</div>`;
    const suppressed = strategy.intents_suppressed;
    const suppressionNote = strategy.last_suppression_reason
      ? note(`Last signal suppressed: ${strategy.last_suppression_reason}`, "is-warn")
      : "";
    const identity = [
      strategy.last_trigger_level ? `<div class="exec-meta"><span class="muted">Trigger level</span> ${html(strategy.last_trigger_level)}</div>` : "",
      strategy.last_intent_id ? `<div class="exec-meta"><span class="muted">Last intent</span> <code class="exec-id">${html(strategy.last_intent_id)}</code></div>` : "",
    ].join("");

    return `
      ${statusRow(strategy.enabled === true ? "live" : "muted",
        strategy.enabled === true ? "Strategy enabled" : "Strategy disabled",
        monitorFlag(strategy))}
      <div class="exec-head">
        ${actionChip(strategy.last_action)}
        <span class="exec-symbol">${html(strategy.symbol || DASH)} \u00b7 ${html(strategy.interval || DASH)}</span>
        <span class="exec-version">${html(strategy.strategy_version || DASH)}</span>
      </div>
      <dl class="kv-grid">
        ${cell("Last decision", relative(strategy.last_decision_at_ms))}
        ${cell("Updated", relative(strategy.updated_at))}
        ${cell("ATR", format.number(strategy.atr, 2))}
        ${cell("Levels tracked", countText(strategy.levels_tracked))}
        ${cell("Volume threshold", compact(strategy.volume_threshold))}
        ${cell("Candles observed", countText(strategy.candles_observed))}
        ${cell("SL band", band(strategy.sl_min_pips, strategy.sl_max_pips, "pips"))}
        ${cell("TP band", band(strategy.tp_min_pips, strategy.tp_max_pips, "pips"))}
        ${cell("RR band", band(strategy.rr_min, strategy.rr_max, "", 1))}
        ${cell("Intent TTL", strategy.ttl_secs === null ? DASH : `${countText(strategy.ttl_secs)} s`)}
        ${cell("Last bar", format.dateTime(strategy.last_candle_time), "", "kv-wide")}
      </dl>
      <div class="subhead"><span>State machine</span><span class="muted">since start</span></div>
      ${counterHtml}
      <div class="exec-intents">
        <div class="exec-intent"><span class="exec-intent-key">Intents emitted</span><b class="tnum">${html(countText(strategy.intents_emitted))}</b></div>
        <div class="exec-intent"><span class="exec-intent-key">Intents suppressed</span><b class="tnum ${suppressed ? "is-warn" : ""}">${html(countText(suppressed))}</b></div>
      </div>
      ${suppressionNote}
      ${identity}
      ${strategy.last_error ? note(`Last strategy error: ${strategy.last_error}`, "is-bad") : ""}`;
  }

  function venueMarkup(work, snapshot) {
    if (!work) {
      return unavailable("Execution telemetry unavailable",
        `This snapshot has no work block, so \`GET ${endpointText()}\` is not a Node 4 ExecutionSnapshot.`);
    }
    const dryRun = work.venue === "none";
    const known = work.venue !== null;
    const venueLabel = !known ? DASH : dryRun ? "none (dry run)" : work.venue;
    const duplicates = work.intents_duplicate;
    return `
      ${statusRow(!known ? "muted" : dryRun ? "warn" : "live",
        !known ? "Venue not reported"
          : dryRun ? "Dry run \u2014 no broker order is placed" : `Venue ${work.venue}`)}
      ${dryRun ? note("Intents are recorded and executed against no venue. No position exists because of this page.") : ""}
      <dl class="kv-grid">
        ${cell("Venue", venueLabel)}
        ${cell("Snapshot", streamShort())}
        ${cell("Orders placed", countText(work.orders_placed))}
        ${cell("Intents accepted", countText(work.intents_accepted))}
        ${cell("Duplicates rejected", countText(duplicates))}
        ${cell("Orders unknown", countText(work.orders_unknown))}
      </dl>
      ${buildLine(snapshot)}
      ${duplicates ? note("Replayed intents are dropped by design; duplicates rejected is the expected count after a reconnect, not an error.") : ""}
      ${work.last_error ? note(`Last execution error: ${work.last_error}`, "is-bad") : ""}`;
  }

  function node3Markup(node3, strategy) {
    const view = describeLink(node3, strategy);
    if (isLocal(strategy)) {
      return `${statusRow(view.tone, view.headline)}${note(view.detail)}`;
    }
    if (!node3) {
      return `${statusRow(view.tone, view.headline)}${note(view.detail)}`;
    }
    return `
      ${statusRow(view.tone, view.headline)}
      ${note(view.detail)}
      <dl class="kv-grid">
        ${cell("Link state", node3.state || DASH)}
        ${cell("Connected", yesNo(node3.connected))}
        ${cell("Authenticated", yesNo(node3.authenticated))}
        ${cell("Last message", relative(node3.last_message_ts))}
        ${cell("Reports sent", countText(node3.reports_sent))}
        ${cell("Reports acked", countText(node3.reports_acked))}
      </dl>
      ${node3.url ? `<div class="exec-meta" title="Node 4's outbound link to Node 3 \u2014 the browser never connects to it.">${html(node3.url)}</div>` : ""}
      ${node3.last_error ? note(`Last link error: ${node3.last_error}`, "is-bad") : ""}`;
  }

  function paintBanner(work) {
    const banner = $("dry-run-banner");
    if (!banner) return;
    const dryRun = !!work && work.venue === "none";
    banner.hidden = !dryRun;
    if (!dryRun) return;
    const text = $("dry-run-text");
    const message = "Venue \u201cnone\u201d \u2014 intents are recorded but no broker order is placed. There is no open position.";
    if (text && text.textContent !== message) text.textContent = message;
  }

  function render() {
    const snapshot = state.snapshot;
    const strategy = snapshot ? snapshot.strategy : null;
    const feed = snapshot ? snapshot.market_feed : null;

    const mode = modeLabel(strategy);
    paintState("exec-feed-mode", mode || DASH, mode === "MONITOR ONLY" ? "warn" : "");
    paintState("exec-strategy-mode", mode || DASH, mode === "MONITOR ONLY" ? "warn" : "");

    const venue = snapshot ? snapshot.work : null;
    if (venue && venue.venue === "none") paintState("exec-venue-mode", "DRY RUN", "warn");
    else if (venue && venue.venue) paintState("exec-venue-mode", venue.venue.toUpperCase(), "live");
    else paintState("exec-venue-mode", DASH, "");

    if (isLocal(strategy)) paintState("exec-node3-mode", "NOT USED", "");
    else if (snapshot && snapshot.node3) {
      paintState("exec-node3-mode", (snapshot.node3.state || DASH).toUpperCase(),
        snapshot.node3.connected === true ? "live" : "warn");
    } else paintState("exec-node3-mode", DASH, "");

    paint("exec-feed-body", snapshot ? feedMarkup(feed, strategy, snapshot) : noSnapshot());
    paint("exec-strategy-body", snapshot ? strategyMarkup(strategy, snapshot) : noSnapshot());
    paint("exec-venue-body", snapshot ? venueMarkup(snapshot.work, snapshot) : noSnapshot());
    paint("exec-node3-body", snapshot ? node3Markup(snapshot.node3, strategy) : noSnapshot());

    paintBanner(snapshot ? snapshot.work : null);
  }

  // ---------------------------------------------------------------
  // Transport: GET /diagnostics seed + a dedicated Node 4 socket
  //
  // Deliberately separate from js/socket.js: a dead execution service must
  // never disturb the Node 1 market stream, and vice versa.
  // ---------------------------------------------------------------

  const MIN_DELAY = Math.max(250, Number(cfg && cfg.reconnectMinMs) || 1000);
  const MAX_DELAY = Math.max(MIN_DELAY, Number(cfg && cfg.reconnectMaxMs) || 30000);
  const HEARTBEAT_EVERY = Math.max(5000, Number(cfg && cfg.heartbeatEveryMs) || 30000);
  const HEARTBEAT_TIMEOUT = Math.max(HEARTBEAT_EVERY, Number(cfg && cfg.heartbeatTimeoutMs) || 75000);
  const WATCHDOG_EVERY = 5000;

  let started = false;
  let socket = null;
  let phase = "idle";             // idle | disabled | connecting | live | reconnecting
  let attempted = false;
  let reconnectDelay = MIN_DELAY;
  let reconnectTimer = null;
  let heartbeatTimer = null;
  let watchdogTimer = null;
  let pollTimer = null;
  let pollGraceTimer = null;
  let openedAt = null;
  let lastMessageAt = null;
  let fetching = false;
  let visibilityHandler = null;

  function isOpen(candidate) {
    return !!(typeof WebSocket !== "undefined" && candidate && candidate.readyState === WebSocket.OPEN);
  }

  function streamView() {
    const configured = !!(cfg && cfg.ws);
    return {
      configured,
      phase,
      connected: isOpen(socket),
      polling: pollTimer !== null,
      lastMessageAt,
      lastError: state.lastStreamError,
    };
  }

  function clearReconnect() {
    if (reconnectTimer !== null) { clearTimeout(reconnectTimer); reconnectTimer = null; }
  }

  function clearHeartbeat() {
    if (heartbeatTimer !== null) { clearInterval(heartbeatTimer); heartbeatTimer = null; }
  }

  function stopPolling() {
    if (pollGraceTimer !== null) { clearTimeout(pollGraceTimer); pollGraceTimer = null; }
    if (pollTimer !== null) { clearInterval(pollTimer); pollTimer = null; }
  }

  function startPolling() {
    if (pollTimer !== null) return;
    if (!cfg || !cfg.diagnostics) return;
    pollTimer = setInterval(() => { load(); }, Math.max(1000, Number(cfg.pollFallbackMs) || 5000));
  }

  async function load() {
    if (fetching) return false;
    const endpoint = cfg && cfg.diagnostics;
    if (!endpoint) return false;
    fetching = true;
    try {
      const payload = App.api && typeof App.api.fetchExecutionDiagnostics === "function"
        ? await App.api.fetchExecutionDiagnostics()
        : await App.utils.fetchPublicJson(endpoint, 8000);
      const accepted = apply(payload, "rest");
      state.lastRestError = null;
      return accepted;
    } catch (error) {
      state.lastRestError = error && error.message ? error.message : "diagnostics request failed";
      if (!isOpen(socket)) render();
      return false;
    } finally {
      fetching = false;
    }
  }

  function scheduleReconnect() {
    if (!started || !cfg || !cfg.ws || reconnectTimer !== null) return;
    phase = "reconnecting";
    const delay = reconnectDelay;
    reconnectDelay = Math.min(reconnectDelay * 2, MAX_DELAY);
    reconnectTimer = setTimeout(() => { reconnectTimer = null; connect(); }, delay);
    render();
  }

  function forceReconnect(reason) {
    if (!started || !cfg || !cfg.ws) return;
    if (reason) state.lastStreamError = reason;
    phase = "reconnecting";
    clearHeartbeat();
    clearReconnect();
    const closing = socket;
    socket = null;
    if (closing) { try { closing.close(); } catch { /* already closed */ } }
    startPolling();
    scheduleReconnect();
    render();
  }

  function connect() {
    if (!started) return;
    if (!cfg || !cfg.ws) {
      phase = "disabled";
      render();
      return;
    }
    if (typeof WebSocket === "undefined") {
      phase = "disabled";
      state.lastStreamError = "WebSocket is not supported in this browser";
      startPolling();
      render();
      return;
    }
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) return;

    phase = attempted ? "reconnecting" : "connecting";
    attempted = true;
    render();

    let candidate;
    try {
      candidate = new WebSocket(cfg.ws);
    } catch (error) {
      state.lastStreamError = error && error.message ? error.message : "WebSocket construction failed";
      startPolling();
      scheduleReconnect();
      return;
    }
    socket = candidate;

    candidate.onopen = () => {
      if (socket !== candidate) return;
      openedAt = Date.now();
      lastMessageAt = Date.now();
      phase = "live";
      state.lastStreamError = null;
      reconnectDelay = MIN_DELAY;
      clearReconnect();
      stopPolling();
      try {
        candidate.send(JSON.stringify({ type: "subscribe", topics: (cfg.topics || ["diagnostics"]).slice() }));
      } catch (error) {
        state.lastStreamError = error && error.message ? error.message : "subscribe failed";
      }
      clearHeartbeat();
      heartbeatTimer = setInterval(() => {
        if (socket !== candidate || !isOpen(candidate)) return;
        const reference = lastMessageAt || openedAt;
        if (reference && Date.now() - reference > HEARTBEAT_TIMEOUT) {
          forceReconnect("diagnostics stream went silent");
          return;
        }
        try { candidate.send(JSON.stringify({ type: "heartbeat" })); } catch { /* closed */ }
      }, HEARTBEAT_EVERY);
      render();
    };

    candidate.onmessage = (event) => {
      if (socket !== candidate) return;
      lastMessageAt = Date.now();
      let frame;
      try {
        frame = JSON.parse(event.data);
      } catch {
        state.lastStreamError = "received malformed diagnostics JSON";
        render();
        return;
      }
      if (!frame || typeof frame !== "object") return;
      if (frame.type === "heartbeat") { render(); return; }
      if (frame.type === "diagnostics") {
        // A complete snapshot: replace, never merge.
        apply(own(frame, "data") ? frame.data : frame, "ws");
      }
    };

    candidate.onerror = () => {
      if (socket !== candidate) return;
      state.lastStreamError = "diagnostics stream error";
      phase = "reconnecting";
      try { candidate.close(); } catch { /* already closed */ }
      startPolling();
      scheduleReconnect();
    };

    candidate.onclose = () => {
      if (socket !== candidate) return;
      socket = null;
      clearHeartbeat();
      phase = "reconnecting";
      startPolling();
      scheduleReconnect();
      render();
    };
  }

  function watchdog() {
    if (isOpen(socket)) {
      const reference = lastMessageAt || openedAt;
      if (reference && Date.now() - reference > HEARTBEAT_TIMEOUT) {
        forceReconnect("diagnostics stream went silent");
        return;
      }
    }
    render(); // refreshes the relative ages without waiting for a frame
  }

  function handleVisibility() {
    if (!started || typeof document === "undefined" || document.visibilityState !== "visible") return;
    if (!cfg || !cfg.ws) return;
    if (!socket || (socket.readyState !== WebSocket.OPEN && socket.readyState !== WebSocket.CONNECTING)) {
      clearReconnect();
      reconnectDelay = MIN_DELAY;
      load();
      connect();
      return;
    }
    const reference = lastMessageAt || openedAt;
    if (isOpen(socket) && reference && Date.now() - reference > HEARTBEAT_TIMEOUT) {
      forceReconnect("diagnostics stream was silent while the page was hidden");
    }
  }

  function start() {
    if (started) return;
    started = true;
    render();
    if (!cfg || !cfg.diagnostics) {
      phase = "disabled";
      render();
      return;
    }
    load();
    connect();
    // A brief grace period so a normal first connect never flashes "polling".
    pollGraceTimer = setTimeout(() => {
      pollGraceTimer = null;
      if (!isOpen(socket)) startPolling();
    }, Math.max(0, Number(cfg.pollGraceMs) || 1500));
    watchdogTimer = setInterval(watchdog, WATCHDOG_EVERY);
    if (typeof document !== "undefined" && document.addEventListener) {
      visibilityHandler = handleVisibility;
      document.addEventListener("visibilitychange", visibilityHandler);
    }
  }

  function stop() {
    started = false;
    clearReconnect();
    clearHeartbeat();
    stopPolling();
    if (watchdogTimer !== null) { clearInterval(watchdogTimer); watchdogTimer = null; }
    if (visibilityHandler && typeof document !== "undefined" && document.removeEventListener) {
      document.removeEventListener("visibilitychange", visibilityHandler);
    }
    visibilityHandler = null;
    const closing = socket;
    socket = null;
    if (closing) { try { closing.close(); } catch { /* already closed */ } }
    phase = "idle";
  }

  App.execution = {
    start,
    stop,
    load,
    render,
    apply,
    getState,
    // Pure helpers, exported so the wire contract can be tested directly.
    normaliseSnapshot,
    describeFeed,
    describeLink,
    missingSection,
    modeLabel,
    isLocal,
  };
})((window.App = window.App || {}));
