// topics.js — subscribe payloads per the published contracts.
//
// Node 1 (FRONTEND_NEXT.md): subscribe topics; the server replays all
// levels, the last 15 candles, a tick_volume snapshot, calendar, strategy
// last + history, the demo snapshot, then status. "bubbles" is deleted
// deliberately — it is NOT subscribed. Heartbeat (30s) is always forwarded.
//
// Node 4 (FRONTEND_RESOURCES.md §1.7): subscribe triggers 7 initial frames
// (diagnostics, open_trades, deriv_account, mt5_account, mt5_positions,
// mt5_history, bridge_status). The documented subscribe list includes
// "diagnostics" — the server treats it as a wildcard, so including it
// subscribes to every forwarded topic. Bridge-only frames are never
// forwarded to browsers. Heartbeat 25s is always forwarded.

export const NODE1_TOPICS = [
  "levels",
  "candle",
  "tick_volume",
  "strategy",
  "demo",
  "calendar",
  "status",
];

export const NODE4_TOPICS = [
  "mt5_account",
  "mt5_positions",
  "mt5_history",
  "bridge_status",
  "bridge_event",
  "open_trades",
  "diagnostics",
];

export function node1SubscribeMessage() {
  return { type: "subscribe", topics: [...NODE1_TOPICS] };
}

export function node4SubscribeMessage() {
  return { type: "subscribe", topics: [...NODE4_TOPICS] };
}
