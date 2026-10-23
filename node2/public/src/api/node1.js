// node1.js — Node 1 market-data REST resources (all GET, read-only).
// Contract: FRONTEND_NEXT.md §3. "bubbles" is deleted — not requested.

import { fetchJson } from "./rest.js";

export function node1Url(base, path) {
  return `${base}${path}`;
}

export async function fetchNode1Resource(base, path) {
  return fetchJson(node1Url(base, path));
}

// Seed all Node 1 resources in parallel. Individual 404s (e.g. the deployed
// engine lacks /strategy or /demo) are tolerated and reported through
// onEndpointError so the UI can show "unavailable" instead of failing the
// whole bootstrap.
export async function seedNode1(base, { onEndpointError, clearEndpointError } = {}) {
  const results = await Promise.all([
    fetchNode1Resource(base, "/status"),
    fetchNode1Resource(base, "/levels"),
    fetchNode1Resource(base, "/candles"),
    fetchNode1Resource(base, "/tick-volume"),
    fetchNode1Resource(base, "/calendar"),
    fetchNode1Resource(base, "/strategy"),
    fetchNode1Resource(base, "/demo"),
  ]);
  const [status, levels, candles, tickVolume, calendar, strategy, demo] = results;
  const payload = {};
  const endpoints = [
    ["status", status, "strategy/status"],
    ["levels", levels, "levels"],
    ["candles", candles, "candles"],
    ["tick_volume", tickVolume, "tick-volume"],
    ["calendar", calendar, "calendar"],
    ["strategy", strategy, "strategy"],
    ["demo", demo, "demo"],
  ];
  for (const [key, res, label] of endpoints) {
    if (res.ok && res.data != null) {
      payload[key] = res.data;
      if (clearEndpointError) clearEndpointError(`node1:${label}`);
    } else {
      if (onEndpointError) {
        onEndpointError(`node1:${label}`, res.status === 404
          ? "not available on this engine"
          : `HTTP ${res.status || "network error"}`);
      }
    }
  }
  return payload;
}
