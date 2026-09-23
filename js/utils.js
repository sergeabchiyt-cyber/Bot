/* ============================================================
 * XAUUSD Terminal — shared helpers
 * ============================================================ */
(function (App) {
  "use strict";

  const $ = (id) => document.getElementById(id);

  const ESC = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
  function escapeHtml(v) {
    return String(v == null ? "" : v).replace(/[&<>"']/g, (c) => ESC[c]);
  }

  /** Normalise a numeric timestamp (s or ms) to milliseconds. */
  function toMs(t) {
    if (typeof t !== "number" || !isFinite(t)) return null;
    return t < 1e12 ? t * 1000 : t;
  }

  /** Normalise a numeric timestamp (s or ms) to whole seconds. */
  function toSec(t) {
    const ms = toMs(t);
    return ms == null ? null : Math.floor(ms / 1000);
  }

  function fmtPrice(p) {
    return Number(p).toFixed(2);
  }

  function fmtClock(ms, withSeconds = false) {
    return new Date(ms).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      ...(withSeconds ? { second: "2-digit" } : {}),
      hour12: false,
    });
  }

  /** Local-day key, e.g. "2026-09-23". */
  function dayKey(ms) {
    const d = new Date(ms);
    return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
  }

  function fmtDayLabel(ms) {
    const today = dayKey(Date.now());
    const tomorrow = dayKey(Date.now() + 86400000);
    const k = dayKey(ms);
    const base = new Date(ms).toLocaleDateString([], {
      weekday: "short",
      day: "numeric",
      month: "short",
    });
    if (k === today) return `Today · ${base}`;
    if (k === tomorrow) return `Tomorrow · ${base}`;
    return base;
  }

  /** "in 2h 05m", "in 12m", "now" */
  function fmtCountdown(ms) {
    const diff = ms - Date.now();
    if (diff <= 0) return "now";
    const m = Math.round(diff / 60000);
    if (m < 60) return `in ${m}m`;
    const h = Math.floor(m / 60);
    if (h < 48) return `in ${h}h ${String(m % 60).padStart(2, "0")}m`;
    return `in ${Math.floor(h / 24)}d`;
  }

  function setStatus(text, state) {
    const el = $("status");
    if (!el) return;
    el.textContent = text;
    el.dataset.state = state || "";
  }

  App.utils = {
    $, escapeHtml, toMs, toSec, fmtPrice, fmtClock,
    dayKey, fmtDayLabel, fmtCountdown, setStatus,
  };
})((window.App = window.App || {}));
