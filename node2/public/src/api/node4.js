// node4.js — Node 4 read-only execution REST resources (all GET).
// Contract: FRONTEND_RESOURCES.md §1.6. Only the public read-only resources
// listed there are requested; operator-only endpoints are never fetched by
// browser code (see README security constraints).

import { fetchJson } from "./rest.js";

export function node4Url(base, path) {
  return `${base}${path}`;
}

export async function fetchNode4Resource(base, path) {
  return fetchJson(node4Url(base, path));
}

// Seed all Node 4 public resources in parallel. Individual failures are
// tolerated and reported through onEndpointError.
export async function seedNode4(base, { onEndpointError, clearEndpointError } = {}) {
  const results = await Promise.all([
    fetchNode4Resource(base, "/diagnostics"),
    fetchNode4Resource(base, "/open-trades"),
    fetchNode4Resource(base, "/mt5/account"),
    fetchNode4Resource(base, "/mt5/positions"),
    fetchNode4Resource(base, "/mt5/history"),
    fetchNode4Resource(base, "/mt5/status"),
  ]);
  const [diagnostics, openTrades, mt5Account, mt5Positions, mt5History, mt5Status] = results;
  const payload = {};
  const endpoints = [
    ["diagnostics", diagnostics, "diagnostics"],
    ["open_trades", openTrades, "open-trades"],
    ["mt5_account", mt5Account, "mt5/account"],
    ["mt5_positions", mt5Positions, "mt5/positions"],
    ["mt5_history", mt5History, "mt5/history"],
    ["mt5_status", mt5Status, "mt5/status"],
  ];
  for (const [key, res, label] of endpoints) {
    if (res.ok && res.data != null) {
      payload[key] = res.data;
      if (clearEndpointError) clearEndpointError(`node4:${label}`);
    } else {
      if (onEndpointError) {
        onEndpointError(`node4:${label}`, res.status === 404
          ? "not available"
          : `HTTP ${res.status || "network error"}`);
      }
    }
  }
  return payload;
}
