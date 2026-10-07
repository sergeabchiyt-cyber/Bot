/* ============================================================
 * Node 1 market WebSocket adapter.
 * Reuses the same independent stream lifecycle as Node 3 and Node 4.
 * ============================================================ */
(function (App) {
  "use strict";

  const handlers = Object.create(null);
  const settings = App.config.market;
  const stream = App.createServiceStream({
    service: "market",
    endpoint: App.config.endpoints.marketWs,
    topics: settings.topics,
    reconnectMinMs: settings.reconnectMinMs,
    reconnectMaxMs: settings.reconnectMaxMs,
    staleAfterMs: settings.staleAfterMs,
    heartbeatIntervalMs: settings.heartbeatIntervalMs,
    heartbeatTimeoutMs: settings.heartbeatTimeoutMs,
    onFrame(frame) {
      const handler = frame && handlers[frame.type];
      if (typeof handler !== "function") return;
      try {
        handler(Object.prototype.hasOwnProperty.call(frame, "data") ? frame.data : frame);
      } catch (error) {
        console.error(`Market handler "${frame.type}" failed:`, error);
      }
    },
  });

  App.socket = {
    on(type, handler) {
      if (typeof handler === "function") handlers[type] = handler;
    },
    connect: stream.start,
    start: stream.start,
    stop: stream.stop,
    state: stream.getState,
    refreshSnapshots: stream.refreshSnapshots,
    setSnapshotRefresh: stream.setSnapshotRefresh,
  };
})((window.App = window.App || {}));
