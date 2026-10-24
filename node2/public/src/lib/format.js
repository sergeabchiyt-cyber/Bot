// format.js — display formatting per Node 1 FRONTEND_NEXT.md §5.
// All formatters are pure; null/undefined/NaN render as "—" (never 0) so an
// absent field cannot masquerade as a confirmed value.

export const NA = "—";

function group(digits) {
  // 1234567.8 -> "1,234,567.8" (fraction kept as-is, already rounded by caller)
  const [int, frac] = digits.split(".");
  const withGroups = int.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return frac === undefined ? withGroups : `${withGroups}.${frac}`;
}

function round(value, decimals) {
  const f = 10 ** decimals;
  // Guard against NaN/Infinity before rounding.
  if (!Number.isFinite(value)) return null;
  return Math.round((value + Number.EPSILON) * f) / f;
}

// Price (XAUUSD): 2 decimals, thousands grouping.
export function fmtPrice(value) {
  const r = round(value, 2);
  return r === null ? NA : group(r.toFixed(2));
}

// Money (USD): 2 decimals, thousands grouping.
export function fmtMoney(value, { signed = false } = {}) {
  const r = round(value, 2);
  if (r === null) return NA;
  const s = group(r.toFixed(2));
  if (signed && r !== 0) return (r < 0 ? "−" : "+") + group(Math.abs(r).toFixed(2));
  return s;
}

// Lots: 4 decimals.
export function fmtLots(value) {
  const r = round(value, 4);
  return r === null ? NA : r.toFixed(4);
}

// Percent: 1 decimal, signed.
export function fmtPercent(value, { signed = false } = {}) {
  const r = round(value, 1);
  if (r === null) return NA;
  // toFixed keeps the minus sign for negatives in the unsigned form.
  const s = `${r.toFixed(1)}%`;
  if (signed && r !== 0) return (r < 0 ? "−" : "+") + Math.abs(r).toFixed(1) + "%";
  return s;
}

// Reward-to-risk: 1 decimal, signed.
export function fmtR(value) {
  const r = round(value, 1);
  return r === null ? NA : (r >= 0 ? "+" : "−") + Math.abs(r).toFixed(1);
}

// Confidence: 0-100 rendered as percent.
export function fmtConfidence(value) {
  const r = round(value, 1);
  return r === null ? NA : `${r.toFixed(1)}%`;
}

// Time: HH:MM:SS UTC. Seconds appended only when provided.
export function fmtTime(ts) {
  if (!ts || !Number.isFinite(ts)) return NA;
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return NA;
  const hh = String(d.getUTCHours()).padStart(2, "0");
  const mm = String(d.getUTCMinutes()).padStart(2, "0");
  const ss = String(d.getUTCSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss} UTC`;
}

// A short UTC time for tables (no seconds) and a tick label for charts.
export function fmtTimeShort(ts) {
  if (!ts || !Number.isFinite(ts)) return NA;
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return NA;
  const hh = String(d.getUTCHours()).padStart(2, "0");
  const mm = String(d.getUTCMinutes()).padStart(2, "0");
  return `${hh}:${mm} UTC`;
}

export function fmtDuration(ms) {
  if (!Number.isFinite(ms)) return NA;
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}m${String(r).padStart(2, "0")}s`;
}

export function fmtCount(value) {
  if (!Number.isFinite(value)) return NA;
  return String(Math.round(value));
}
