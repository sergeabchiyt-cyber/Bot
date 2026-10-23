/* ============================================================
 * XAUUSD Terminal — one place for every displayed value.
 *
 * Rules pinned here (see README):
 *   price       2 dp, grouped thousands        3,415.10
 *   lots        2 dp                           0.18
 *   money       2 dp, $, grouped               1,000.00  →  $1,000.00
 *   PnL / R     signed (+ / − U+2212)          +$12.40 · −1.00R
 *   percent     1 dp                           62.0%
 *   time        local HH:mm:ss                 14:03:22
 *   date        local DD MMM                   23 Oct
 *   confidence  0..1 ratio → 0..100%           62.0%
 *
 * A null / NaN / blank value always renders as an em dash, never as "null",
 * "NaN" or a raw float. Dependency-free UMD so Node's test runner can use the
 * exact browser helpers.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.format = api;
  }
})(typeof window !== "undefined" ? window : null, function () {
  "use strict";

  const DASH = "\u2014";            // em dash
  const MINUS = "\u2212";           // typographic minus
  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const pad2 = (n) => String(n).padStart(2, "0");

  /** Finite number or null. Preserves 0 and rejects "", true, NaN, Infinity. */
  function num(value) {
    if (value === null || value === undefined || typeof value === "boolean") return null;
    if (typeof value === "string" && !value.trim()) return null;
    const parsed = typeof value === "number" ? value : Number(String(value).trim());
    return Number.isFinite(parsed) ? parsed : null;
  }

  function ms(value) {
    if (value === null || value === undefined || value === "") return null;
    if (typeof value === "number" && Number.isFinite(value)) {
      return Math.abs(value) < 100000000000 ? value * 1000 : value;
    }
    if (typeof value === "string") {
      const text = value.trim();
      if (!text) return null;
      if (/^-?\d+(?:\.\d+)?$/.test(text)) {
        const number = Number(text);
        return Number.isFinite(number) ? (Math.abs(number) < 100000000000 ? number * 1000 : number) : null;
      }
      const parsed = Date.parse(text);
      return Number.isFinite(parsed) ? parsed : null;
    }
    return null;
  }

  /** Grouped fixed-decimal number, or the dash. */
  function number(value, digits = 0) {
    const parsed = num(value);
    if (parsed === null) return DASH;
    const places = Math.max(0, Math.min(8, Number(digits) || 0));
    return parsed.toLocaleString("en-US", {
      minimumFractionDigits: places,
      maximumFractionDigits: places,
    });
  }

  function price(value) {
    return number(value, 2);
  }

  function lots(value) {
    const parsed = num(value);
    return parsed === null ? DASH : parsed.toFixed(2);
  }

  function money(value) {
    const parsed = num(value);
    if (parsed === null) return DASH;
    const abs = Math.abs(parsed).toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
    if (parsed < 0) return `${MINUS}$${abs}`;
    return `$${abs}`;
  }

  /** Always carries a sign for non-zero values; zero stays neutral. */
  function signedMoney(value) {
    const parsed = num(value);
    if (parsed === null) return DASH;
    if (parsed === 0) return "$0.00";
    return `${parsed > 0 ? "+" : MINUS}${money(Math.abs(parsed))}`;
  }

  /** Signed two-decimal number for dimensionless values (score, factor value). */
  function signedNumber(value, digits = 2) {
    const parsed = num(value);
    if (parsed === null) return DASH;
    const places = Math.max(0, Math.min(8, Number(digits) || 0));
    if (parsed === 0) return (0).toFixed(places);
    return `${parsed > 0 ? "+" : MINUS}${Math.abs(parsed).toFixed(places)}`;
  }

  /** Percent units in, e.g. 62 → "62.0%", 15930.5 → "15,930.5%". */
  function percent(value, digits = 1) {
    const parsed = num(value);
    if (parsed === null) return DASH;
    const places = Math.max(0, Math.min(4, Number(digits) || 0));
    return `${parsed.toLocaleString("en-US", {
      minimumFractionDigits: places,
      maximumFractionDigits: places,
    })}%`;
  }

  /** 0..1 confidence ratio → "62.0%". */
  function confidence(ratio) {
    const parsed = num(ratio);
    return parsed === null ? DASH : percent(parsed * 100, 1);
  }

  /** Signed R multiple: +1.85R / −1.00R. */
  function rMultiple(value) {
    const parsed = num(value);
    if (parsed === null) return DASH;
    const sign = parsed > 0 ? "+" : parsed < 0 ? MINUS : "";
    return `${sign}${Math.abs(parsed).toFixed(2)}R`;
  }

  function count(value) {
    return number(value, 0);
  }

  /** Local HH:mm:ss. */
  function time(value) {
    const at = ms(value);
    if (at === null) return DASH;
    const d = new Date(at);
    if (!Number.isFinite(d.getTime())) return DASH;
    return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
  }

  /** Local HH:mm. */
  function clock(value) {
    const at = ms(value);
    if (at === null) return DASH;
    const d = new Date(at);
    if (!Number.isFinite(d.getTime())) return DASH;
    return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  }

  /** Local DD MMM. */
  function date(value) {
    const at = ms(value);
    if (at === null) return DASH;
    const d = new Date(at);
    if (!Number.isFinite(d.getTime())) return DASH;
    return `${pad2(d.getDate())} ${MONTHS[d.getMonth()]}`;
  }

  /** Local DD MMM HH:mm:ss. */
  function dateTime(value) {
    const at = ms(value);
    if (at === null) return DASH;
    return `${date(at)} ${time(at)}`;
  }

  /** Stable local day key used by the calendar's group headers. */
  function dayKey(value) {
    const at = ms(value);
    if (at === null) return "";
    const d = new Date(at);
    return `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;
  }

  function dayLabel(value) {
    const at = ms(value);
    if (at === null) return "Unknown date";
    const today = dayKey(Date.now());
    const tomorrow = dayKey(Date.now() + 86400000);
    const key = dayKey(at);
    const d = new Date(at);
    const base = `${pad2(d.getDate())} ${MONTHS[d.getMonth()]}`;
    if (key === today) return `Today \u00b7 ${base}`;
    if (key === tomorrow) return `Tomorrow \u00b7 ${base}`;
    return base;
  }

  /** Human age of an epoch-ms timestamp. */
  function age(value, nowMs = Date.now()) {
    const at = ms(value);
    if (at === null) return DASH;
    const diff = Number(nowMs) - at;
    if (!Number.isFinite(diff)) return DASH;
    if (diff < 0) return "just now";
    const seconds = Math.floor(diff / 1000);
    if (seconds < 1) return "just now";
    if (seconds < 60) return `${seconds} s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes} min ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} h ago`;
    return `${Math.floor(hours / 24)} d ago`;
  }

  /** "in 12m", "in 1h 05m", "now". */
  function countdown(value) {
    const at = ms(value);
    if (at === null) return DASH;
    const diff = at - Date.now();
    if (diff <= 0) return "now";
    const minutes = Math.ceil(diff / 60000);
    if (minutes < 60) return `in ${minutes}m`;
    const hours = Math.floor(minutes / 60);
    if (hours < 48) return `in ${hours}h ${pad2(minutes % 60)}m`;
    return `in ${Math.floor(hours / 24)}d`;
  }

  return {
    DASH,
    number, price, lots, money, signedMoney, signedNumber,
    percent, confidence, rMultiple, count,
    time, clock, date, dateTime,
    dayKey, dayLabel, age, countdown,
  };
});
