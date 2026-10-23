/* ============================================================
 * XAUUSD Terminal — shared browser helpers.
 *
 * Display formatting lives in js/format.js; this file keeps only DOM,
 * timestamp and public-read plumbing.
 * ============================================================ */
(function (App) {
  "use strict";

  const normalise = App.normalize;

  const $ = (id) => document.getElementById(id);

  const escapeHtml = (value) => normalise.escapeHtml(value);
  const toMs = (value) => normalise.timestampMs(value);
  const numberOrNull = (value) => normalise.numberOrNull(value);
  const toSec = (value) => {
    const ms = toMs(value);
    return ms === null ? null : Math.floor(ms / 1000);
  };

  /**
   * Bounded, credential-free public GET. Both REST snapshots (`/strategy`,
   * `/demo`) use it; it never sends a body, cache, or auth header.
   */
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

  /** Hidden aria-live line for chart/history diagnostics. */
  function setStatus(text, state) {
    const el = $("status");
    if (!el) return;
    el.textContent = String(text == null ? "" : text);
    el.dataset.state = state || "";
  }

  App.utils = { $, escapeHtml, toMs, toSec, numberOrNull, fetchPublicJson, setStatus };
})((window.App = window.App || {}));
