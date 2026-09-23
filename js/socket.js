/* ============================================================
 * XAUUSD Terminal — backend WebSocket with backoff reconnect
 * Dispatches frames by `type` to registered handlers.
 * ============================================================ */
(function (App) {
  "use strict";

  const { setStatus } = App.utils;
  const cfg = App.config;

  let ws = null;
  let delay = cfg.ws.reconnectMinMs;
  let timer = null;
  const handlers = {};

  function on(type, fn) {
    handlers[type] = fn;
  }

  function scheduleReconnect() {
    if (timer) return;
    setStatus("reconnecting", "busy");
    timer = setTimeout(() => {
      timer = null;
      connect();
    }, delay);
    delay = Math.min(delay * 2, cfg.ws.reconnectMaxMs);
  }

  function connect() {
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
      return;
    }
    ws = new WebSocket(cfg.endpoints.ws);

    ws.onopen = () => {
      delay = cfg.ws.reconnectMinMs;
      setStatus("live", "live");
      ws.send(JSON.stringify({ type: "subscribe", topics: cfg.ws.topics }));
    };

    ws.onmessage = (ev) => {
      let frame;
      try {
        frame = JSON.parse(ev.data);
      } catch {
        return;
      }
      const fn = frame && handlers[frame.type];
      if (fn) {
        try {
          fn(frame.data);
        } catch (e) {
          console.error(`Handler "${frame.type}" failed:`, e);
        }
      }
    };

    ws.onclose = scheduleReconnect;
    ws.onerror = (e) => {
      setStatus("ws error", "error");
      console.error("Backend WS error:", e);
    };
  }

  // Reconnect promptly when a mobile tab returns to the foreground.
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && (!ws || ws.readyState === WebSocket.CLOSED)) {
      clearTimeout(timer);
      timer = null;
      delay = cfg.ws.reconnectMinMs;
      connect();
    }
  });

  App.socket = { on, connect };
})((window.App = window.App || {}));
