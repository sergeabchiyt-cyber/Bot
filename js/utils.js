/* ============================================================
 * XAUUSD Terminal — shared browser helpers
 * ============================================================ */
(function (App) {
  "use strict";

  const $ = (id) => document.getElementById(id);
  const normalise = App.normalize;

  function escapeHtml(value) {
    return normalise.escapeHtml(value);
  }

  function toMs(value) {
    return normalise.timestampMs(value);
  }

  function toSec(value) {
    const ms = toMs(value);
    return ms === null ? null : Math.floor(ms / 1000);
  }

  function numberOrNull(value) {
    return normalise.numberOrNull(value);
  }

  function fmtPrice(value, digits = 2) {
    const number = numberOrNull(value);
    return number === null ? "—" : number.toFixed(digits);
  }

  function fmtNumber(value, digits = 0) {
    const number = numberOrNull(value);
    if (number === null) return "—";
    return number.toLocaleString("en-US", {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
  }

  function fmtClock(value, withSeconds = false) {
    const ms = toMs(value);
    if (ms === null) return "—";
    return new Date(ms).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      ...(withSeconds ? { second: "2-digit" } : {}),
      hour12: false,
    });
  }

  function fmtDateTime(value, withSeconds = true) {
    const ms = toMs(value);
    if (ms === null) return "—";
    return new Date(ms).toLocaleString([], {
      year: "numeric",
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      ...(withSeconds ? { second: "2-digit" } : {}),
      hour12: false,
    });
  }

  function dayKey(value) {
    const ms = toMs(value);
    if (ms === null) return "";
    const date = new Date(ms);
    return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
  }

  function fmtDayLabel(value) {
    const ms = toMs(value);
    if (ms === null) return "Unknown date";
    const today = dayKey(Date.now());
    const tomorrow = dayKey(Date.now() + 86400000);
    const key = dayKey(ms);
    const base = new Date(ms).toLocaleDateString([], {
      weekday: "short",
      day: "numeric",
      month: "short",
    });
    if (key === today) return `Today · ${base}`;
    if (key === tomorrow) return `Tomorrow · ${base}`;
    return base;
  }

  function fmtCountdown(value) {
    const ms = toMs(value);
    if (ms === null) return "—";
    const diff = ms - Date.now();
    if (diff <= 0) return "now";
    const minutes = Math.ceil(diff / 60000);
    if (minutes < 60) return `in ${minutes}m`;
    const hours = Math.floor(minutes / 60);
    if (hours < 48) return `in ${hours}h ${String(minutes % 60).padStart(2, "0")}m`;
    return `in ${Math.floor(hours / 24)}d`;
  }

  const SERVICE_IDS = {
    market: "market-service-badge",
    strategy: "strategy-service-badge",
    execution: "execution-service-badge",
  };
  const SERVICE_LABELS = {
    market: "MARKET / Node 1",
    strategy: "STRATEGY / Node 3",
    execution: "EXECUTION / Node 4",
  };
  const BADGE_TEXT = {
    live: "LIVE",
    reconnecting: "RECONNECTING",
    stale: "STALE",
    offline: "OFFLINE",
    "not-configured": "NOT CONFIGURED",
  };

  async function fetchPublicJson(url, timeoutMs = 12000) {
    if (!url) throw new Error("public snapshot endpoint is not configured");
    if (typeof fetch !== "function") throw new Error("fetch is not supported in this browser");
    const timeout = Math.max(1000, Number(timeoutMs) || 12000);
    const controller = typeof AbortController === "function" ? new AbortController() : null;
    const timer = controller ? setTimeout(() => controller.abort(), timeout) : null;
    try {
      const response = await fetch(url, {
        method: "GET",
        cache: "no-store",
        headers: { Accept: "application/json" },
        ...(controller ? { signal: controller.signal } : {}),
      });
      if (!response.ok) throw new Error(`public snapshot request failed (${response.status})`);
      return await response.json();
    } catch (error) {
      if (error && error.name === "AbortError") {
        throw new Error(`public snapshot request timed out after ${Math.ceil(timeout / 1000)}s`);
      }
      throw error;
    } finally {
      if (timer !== null) clearTimeout(timer);
    }
  }

  function updateServiceStatus(service, state, details = {}) {
    const badge = $(SERVICE_IDS[service] || "");
    if (!badge) return;
    const status = BADGE_TEXT[state] ? state : "offline";
    badge.dataset.state = status;
    const value = badge.querySelector && badge.querySelector(".service-status-value");
    if (value) value.textContent = BADGE_TEXT[status];

    const lastData = toMs(details.lastDataAt);
    const lastMessage = toMs(details.lastMessageAt);
    const parts = [SERVICE_LABELS[service] || String(service), BADGE_TEXT[status]];
    if (state === "not-configured") parts.push("service endpoint is not configured");
    parts.push(lastData === null ? "no data received yet" : `last data update ${fmtDateTime(lastData)}`);
    if (lastMessage !== null) parts.push(`last WebSocket message ${fmtDateTime(lastMessage)}`);
    if (details.lastError) parts.push(`last error: ${String(details.lastError)}`);
    const label = parts.join(". ");
    badge.title = label;
    if (badge.setAttribute) badge.setAttribute("aria-label", label);
  }

  // Kept as a hidden live region for chart/history diagnostics. The visible
  // connection truth is shown by the three independent service badges.
  function setStatus(text, state) {
    const el = $("status");
    if (!el) return;
    el.textContent = String(text == null ? "" : text);
    el.dataset.state = state || "";
  }

  App.utils = {
    $, escapeHtml, toMs, toSec, numberOrNull,
    fmtPrice, fmtNumber, fmtClock, fmtDateTime,
    dayKey, fmtDayLabel, fmtCountdown,
    fetchPublicJson, updateServiceStatus, setStatus,
  };
})((window.App = window.App || {}));
