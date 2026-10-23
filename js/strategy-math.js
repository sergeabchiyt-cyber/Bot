/* ============================================================
 * XAUUSD Terminal — pure strategy/demo math.
 *
 * No DOM, no sockets: this module is the testable core behind the
 * strategy panel, the position progress bar and the decision history
 * reducer. State updates are keyed on decision.ts, so a reconnect replay
 * replaces a decision instead of duplicating it.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.strategyMath = api;
  }
})(typeof window !== "undefined" ? window : null, function () {
  "use strict";

  const ACTIONS = ["buy", "sell", "flat"];
  const FACTOR_LABELS = {
    trend: "Trend",
    value_location: "Value location",
    level_reaction: "Level reaction",
    momentum: "Momentum",
    tick_pressure: "Tick pressure",
    swing_structure: "Swing structure",
  };
  const REGIME_LABELS = {
    trend_up: "Trend up",
    trend_down: "Trend down",
    balance: "Balance",
  };
  const VOLATILITY_LABELS = {
    expanding: "Expanding",
    normal: "Normal",
    contracting: "Contracting",
  };

  function finite(value) {
    if (value === null || value === undefined || typeof value === "boolean") return null;
    if (typeof value === "string" && !value.trim()) return null;
    const number = typeof value === "number" ? value : Number(String(value).trim());
    return Number.isFinite(number) ? number : null;
  }

  function clamp(value, low, high) {
    const number = finite(value);
    if (number === null) return low;
    return Math.min(high, Math.max(low, number));
  }

  function actionOf(decision) {
    const action = decision && typeof decision.action === "string"
      ? decision.action.toLowerCase()
      : "";
    return ACTIONS.includes(action) ? action : "flat";
  }

  function regimeLabel(regime) {
    if (regime && REGIME_LABELS[regime]) return REGIME_LABELS[regime];
    const text = String(regime || "").trim();
    if (!text) return "Unknown";
    return text.replace(/_/g, " ").replace(/^\w/, (c) => c.toUpperCase());
  }

  function volatilityLabel(volatility) {
    if (volatility && VOLATILITY_LABELS[volatility]) return VOLATILITY_LABELS[volatility];
    const text = String(volatility || "").trim();
    if (!text) return "Unknown";
    return text.replace(/_/g, " ").replace(/^\w/, (c) => c.toUpperCase());
  }

  function factorLabel(name) {
    const key = String(name || "").trim();
    if (FACTOR_LABELS[key]) return FACTOR_LABELS[key];
    if (!key) return "Factor";
    return key.replace(/[_-]+/g, " ").replace(/^\w/, (c) => c.toUpperCase());
  }

  /** 0..1 ratio to percent number (not a string). */
  function confidencePct(ratio) {
    const number = finite(ratio);
    return number === null ? null : clamp(number, 0, 1) * 100;
  }

  /** Reasons sorted by |contribution| descending; malformed rows sink. */
  function sortReasons(reasons) {
    const list = Array.isArray(reasons) ? reasons.slice() : [];
    return list.sort((a, b) => {
      const left = Math.abs(finite(a && a.contribution) ?? 0);
      const right = Math.abs(finite(b && b.contribution) ?? 0);
      return right - left;
    });
  }

  /** Geometry for the zero-centred score bar (−1…+1). */
  function scoreBar(score) {
    const value = finite(score);
    if (value === null) return { value: null, sign: "zero", widthPct: 0 };
    const clamped = clamp(value, -1, 1);
    return {
      value: clamped,
      sign: clamped > 0 ? "bull" : clamped < 0 ? "bear" : "zero",
      widthPct: Math.abs(clamped) * 50,
    };
  }

  /**
   * One reducer per topic, keyed on `ts`.
   *
   * - a repeated ts replaces its history entry (reconnect replay)
   * - history stays ascending and capped (oldest first)
   * - `last` only moves forward, so a late/older replay cannot rewind the panel
   */
  function mergeDecision(state, decision, limit = 120) {
    const current = state && typeof state === "object" ? state : {};
    const history = Array.isArray(current.history) ? current.history.slice() : [];
    const ts = finite(decision && decision.ts);
    if (ts === null) return { last: current.last || null, history, changed: false };

    const entry = decision;
    const index = history.findIndex((item) => finite(item && item.ts) === ts);
    if (index >= 0) history[index] = entry;
    else history.push(entry);
    history.sort((a, b) => (finite(a && a.ts) ?? 0) - (finite(b && b.ts) ?? 0));

    const cap = Math.max(1, Number(limit) || 120);
    while (history.length > cap) history.shift();

    const lastTs = finite(current.last && current.last.ts);
    const takeAsLast = lastTs === null || ts >= lastTs;
    return {
      last: takeAsLast ? entry : current.last,
      history,
      changed: true,
    };
  }

  /**
   * The decision where the current buy/sell run began, for the chart marker.
   * A current `flat` action has no arrow: a change to flat is not a
   * directional entry and must not keep an old arrow alive.
   */
  function lastDirectionalChange(history) {
    const list = Array.isArray(history) ? history : [];
    if (!list.length) return null;
    const latest = actionOf(list[list.length - 1]);
    if (latest !== "buy" && latest !== "sell") return null;
    for (let i = list.length - 1; i >= 0; i -= 1) {
      const action = actionOf(list[i]);
      if (action !== latest) continue;
      const previous = i > 0 ? actionOf(list[i - 1]) : null;
      if (previous !== action) return list[i];
    }
    return null;
  }

  /**
   * Position progress geometry for the entry/SL/TP bar.
   *
   * The track spans stop → target (profit side green), with entry in the
   * middle. When a stop is absent the track spans entry → target and the
   * marker still shows how far price has travelled.
   */
  function progressToTarget(input) {
    const opts = input && typeof input === "object" ? input : {};
    const side = String(opts.side || "").toLowerCase() === "sell" ? "sell" : "buy";
    const entry = finite(opts.entry);
    const stop = finite(opts.stop);
    const target = finite(opts.target);
    const price = finite(opts.price);

    if (entry === null || target === null || entry === target) {
      return { available: false, hasStop: stop !== null, inProfit: false };
    }

    const direction = side === "buy" ? 1 : -1;
    const profitSpan = (target - entry) * direction;
    const stopValid = stop !== null && (stop - entry) * direction < 0;
    const low = stopValid ? Math.min(stop, target, entry) : Math.min(entry, target);
    const high = stopValid ? Math.max(stop, target, entry) : Math.max(entry, target);
    const span = high - low;
    const place = (value) => {
      if (value === null || span <= 0) return null;
      return clamp(((value - low) / span) * 100, 0, 100);
    };

    const current = price === null ? null : price;
    const rawProgress = profitSpan > 0 && price !== null ? ((price - entry) * direction) / profitSpan : null;
    const inProfit = price === null ? false : (price - entry) * direction > 0;

    return {
      available: true,
      hasStop: stopValid,
      side,
      low,
      high,
      entryPct: place(entry),
      stopPct: stopValid ? place(stop) : null,
      targetPct: place(target),
      pricePct: place(current),
      profitPct: profitSpan > 0 ? clamp((profitSpan / span) * 100, 0, 100) : 0,
      progressPct: rawProgress === null ? null : clamp(rawProgress * 100, 0, 100),
      inProfit,
      overshoot: rawProgress === null ? false : rawProgress > 1 || rawProgress < 0,
    };
  }

  function isStale(ts, nowMs = Date.now(), limitMs = 60000) {
    const at = finite(ts);
    if (at === null) return true;
    return nowMs - at > limitMs;
  }

  return {
    finite,
    clamp,
    actionOf,
    regimeLabel,
    volatilityLabel,
    factorLabel,
    confidencePct,
    sortReasons,
    scoreBar,
    mergeDecision,
    lastDirectionalChange,
    progressToTarget,
    isStale,
  };
});
