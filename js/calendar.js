/* ============================================================
 * XAUUSD Terminal — economic calendar (USD only)
 *
 * Backend payload (/calendar REST and "calendar" WS frame):
 *   {
 *     count, source, updated,
 *     events: [{
 *       currency: "USD", event: "Unemployment Claims",
 *       impact: "High" | "Medium" | "Low" | "Holiday",
 *       actual, forecast, previous,   // string | null  e.g. "201K", "4.5%"
 *       gold_relevant: bool,
 *       time: ISO-8601 string, timestamp: epoch ms
 *     }]
 *   }
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml, toMs, fmtClock, dayKey, fmtDayLabel, fmtCountdown } = App.utils;
  const cfg = App.config.calendar;

  let allEvents = [];            // normalised, USD only, sorted
  let minImpact = "low";         // filter chip: low | medium | high
  let updatedAt = null;

  // ---------- Normalisation ----------
  function normalise(raw) {
    const ms = toMs(raw.timestamp) ?? Date.parse(raw.time);
    if (!isFinite(ms)) return null;
    const impact = String(raw.impact || "low").toLowerCase();
    return {
      ms,
      name: raw.event || "",
      currency: String(raw.currency || "").toUpperCase(),
      impact,
      rank: cfg.impactRank[impact] ?? 1,
      actual: raw.actual,
      forecast: raw.forecast,
      previous: raw.previous,
      gold: !!raw.gold_relevant,
    };
  }

  function extract(payload) {
    if (Array.isArray(payload)) return payload;
    if (payload && Array.isArray(payload.events)) return payload.events;
    return [];
  }

  function ingest(payload) {
    if (payload && payload.updated) updatedAt = toMs(payload.updated);
    allEvents = extract(payload)
      .filter((e) => String(e.currency || "").toUpperCase() === cfg.currency)
      .map(normalise)
      .filter(Boolean)
      .sort((a, b) => a.ms - b.ms);
    render();
  }

  // ---------- Filtering ----------
  function visible() {
    const minRank = cfg.impactRank[minImpact] ?? 1;
    const cutoff = Date.now() - cfg.keepPastMs;
    return allEvents.filter((e) => e.rank >= minRank && e.ms >= cutoff);
  }

  // ---------- Rendering ----------
  function valueCell(label, v, cls = "") {
    const has = v !== null && v !== undefined && v !== "";
    return `<span class="ev-val ${cls}${has ? "" : " is-empty"}">
      <em>${label}</em>${has ? escapeHtml(v) : "—"}</span>`;
  }

  /** Colour actual above/below forecast (direction only — not good/bad). */
  function actualClass(e) {
    if (e.actual == null || e.forecast == null) return "";
    const a = parseFloat(e.actual), f = parseFloat(e.forecast);
    if (!isFinite(a) || !isFinite(f) || a === f) return "";
    return a > f ? "is-above" : "is-below";
  }

  function eventRow(e, isNext) {
    const now = Date.now();
    const past = e.ms <= now;
    const hasValues = e.actual != null || e.forecast != null || e.previous != null;
    const classes = [
      "event-row",
      `impact-${e.impact}`,
      past ? "is-past" : "",
      isNext ? "is-next" : "",
      e.gold ? "is-gold" : "",
    ].join(" ");

    return `
      <div class="${classes}">
        <div class="ev-time">
          <span class="ev-clock">${fmtClock(e.ms)}</span>
          ${isNext ? `<span class="ev-countdown">${fmtCountdown(e.ms)}</span>` : ""}
        </div>
        <div class="ev-main">
          <div class="ev-title">
            <i class="impact-bar" title="${e.impact}"></i>
            <span class="ev-name">${escapeHtml(e.name)}</span>
            ${e.gold ? '<span class="ev-gold" title="Gold relevant">XAU</span>' : ""}
          </div>
          ${hasValues ? `
          <div class="ev-values">
            ${valueCell("A", e.actual, actualClass(e))}
            ${valueCell("F", e.forecast)}
            ${valueCell("P", e.previous)}
          </div>` : ""}
        </div>
      </div>`;
  }

  function render() {
    const list = $("calendar-list");
    const count = $("calendar-count");
    const events = visible();
    count.textContent = events.length;

    const nextEvt = allEvents.find((e) => e.ms > Date.now() && e.rank >= 1);
    renderNextBanner(nextEvt);

    if (!events.length) {
      list.innerHTML = `<div class="empty">No ${cfg.currency} events${
        minImpact !== "low" ? ` at ${minImpact}+ impact` : ""}</div>`;
      return;
    }

    const nextVisible = events.find((e) => e.ms > Date.now());
    let html = "";
    let currentDay = null;
    for (const e of events) {
      const k = dayKey(e.ms);
      if (k !== currentDay) {
        currentDay = k;
        html += `<div class="day-head">${fmtDayLabel(e.ms)}</div>`;
      }
      html += eventRow(e, e === nextVisible);
    }
    if (updatedAt) {
      html += `<div class="cal-foot">Updated ${fmtClock(updatedAt)}</div>`;
    }
    list.innerHTML = html;
  }

  /** Compact "next USD event" ticker in the top bar. */
  function renderNextBanner(e) {
    const el = $("next-event");
    if (!el) return;
    if (!e) {
      el.hidden = true;
      return;
    }
    el.hidden = false;
    el.className = `next-event impact-${e.impact}`;
    el.innerHTML = `<i class="impact-bar"></i>
      <span class="ne-name">${escapeHtml(e.name)}</span>
      <span class="ne-when">${fmtCountdown(e.ms)}</span>`;
  }

  // ---------- Impact filter chips ----------
  function bindFilters() {
    document.querySelectorAll("#calendar-filter .chip").forEach((btn) => {
      btn.addEventListener("click", () => {
        minImpact = btn.dataset.impact;
        document
          .querySelectorAll("#calendar-filter .chip")
          .forEach((b) => b.classList.toggle("active", b === btn));
        render();
      });
    });
  }

  // ---------- REST ----------
  async function fetchAll() {
    try {
      const res = await fetch(App.config.endpoints.calendar);
      if (!res.ok) throw new Error(`calendar ${res.status}`);
      ingest(await res.json());
    } catch (e) {
      console.error("Calendar fetch failed:", e);
    }
  }

  function start() {
    bindFilters();
    fetchAll();
    setInterval(fetchAll, cfg.refreshMs);
    setInterval(render, 30000); // keep countdowns / past-dimming fresh
  }

  App.calendar = { ingest, fetchAll, start };
})((window.App = window.App || {}));
