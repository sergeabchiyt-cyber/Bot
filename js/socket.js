/* ============================================================
 * XAUUSD Terminal — the one Node 1 WebSocket.
 *
 * Owns the only socket on the page: subscribe (full topic list on every
 * open, so a reconnect replay reseeds the panels), 30 s heartbeat, silence
 * watchdog, bounded reconnect backoff, and visibility recovery.
 *
 * Frames are routed by their public `type`:
 *   levels · candle · tick_volume · strategy · demo · calendar · status
 * ============================================================ */
(function (App) {
  "use strict";

  const cfg = App.config.socket;
  const endpoint = App.config.client.websocketUrl("node1");
  const topics = cfg.topics.slice();
  const handlers = Object.create(null);
  const stateListeners = new Set();
  const format = App.format;

  const MIN_DELAY = Math.max(250, Number(cfg.reconnectMinMs) || 1000);
  const MAX_DELAY = Math.max(MIN_DELAY, Number(cfg.reconnectMaxMs) || 30000);
  const HEARTBEAT_EVERY = Math.max(5000, Number(cfg.heartbeatEveryMs) || 30000);
  const HEARTBEAT_TIMEOUT = Math.max(HEARTBEAT_EVERY, Number(cfg.heartbeatTimeoutMs) || 75000);
  const STALE_AFTER = Math.max(HEARTBEAT_EVERY, Number(cfg.staleAfterMs) || 90000);
  const WATCHDOG_EVERY = 5000;

  const BADGE_TEXT = {
    live: "LIVE",
    connecting: "CONNECTING",
    reconnecting: "RECONNECTING",
    stale: "STALE",
    offline: "OFFLINE",
    "not-configured": "NOT CONFIGURED",
  };

  let socket = null;
  let started = false;
  let phase = "offline";          // offline | connecting | live | reconnecting
  let reconnectDelay = MIN_DELAY;
  let reconnectTimer = null;
  let heartbeatTimer = null;
  let watchdogTimer = null;
  let visibilityHandler = null;
  let openedAt = null;
  let lastMessageAt = null;
  let lastDataAt = null;
  let lastError = "";
  let receivedFrames = 0;
  let lastSeq = null;

  function isOpen(candidate) {
    return !!(typeof WebSocket !== "undefined" && candidate &&
      candidate.readyState === WebSocket.OPEN);
  }

  function statusValue() {
    if (!endpoint) return "not-configured";
    if (isOpen(socket)) {
      const reference = lastDataAt || openedAt;
      return reference && Date.now() - reference > STALE_AFTER ? "stale" : "live";
    }
    if (phase === "connecting") return "connecting";
    if (phase === "reconnecting") return "reconnecting";
    return "offline";
  }

  function getState() {
    return {
      endpoint,
      endpointConfigured: !!endpoint,
      status: statusValue(),
      connected: isOpen(socket),
      lastMessageAt,
      lastDataAt,
      lastError,
      receivedFrames,
      topics: topics.slice(),
    };
  }

  function paintBadge(state) {
    if (typeof document === "undefined" || !document.getElementById) return;
    const badge = document.getElementById("socket-badge");
    if (!badge) return;
    const label = BADGE_TEXT[state.status] || "OFFLINE";
    badge.dataset.state = state.status;
    const value = badge.querySelector && badge.querySelector(".socket-status-value");
    if (value) value.textContent = label;
    const parts = ["Node 1 stream", label];
    if (!endpoint) parts.push("service endpoint is not configured");
    if (lastDataAt !== null) parts.push(`last data ${format.dateTime(lastDataAt)}`);
    if (lastError) parts.push(`last error: ${lastError}`);
    const title = parts.join(". ");
    badge.title = title;
    if (badge.setAttribute) badge.setAttribute("aria-label", title);
  }

  function publish() {
    const state = getState();
    paintBadge(state);
    for (const listener of stateListeners) {
      try { listener(state); } catch (error) { console.error("Socket state listener failed:", error); }
    }
  }

  function dispatch(frame) {
    if (!frame || typeof frame !== "object" || frame.type === "heartbeat") return;
    const handler = handlers[frame.type];
    if (typeof handler !== "function") return;
    const payload = Object.prototype.hasOwnProperty.call(frame, "data") ? frame.data : frame;
    try {
      handler(payload, frame);
    } catch (error) {
      console.error(`Socket handler "${frame.type}" failed:`, error);
    }
  }

  function clearHeartbeat() {
    if (heartbeatTimer !== null) {
      clearInterval(heartbeatTimer);
      heartbeatTimer = null;
    }
  }

  function sendHeartbeat() {
    if (!isOpen(socket)) return false;
    try {
      socket.send(JSON.stringify({ type: "heartbeat" }));
      return true;
    } catch (error) {
      lastError = `heartbeat send failed: ${error && error.message ? error.message : "unknown error"}`;
      return false;
    }
  }

  function subscribe(candidate) {
    if (!isOpen(candidate) || !topics.length) return;
    try {
      candidate.send(JSON.stringify({ type: "subscribe", topics }));
    } catch (error) {
      lastError = `subscribe failed: ${error && error.message ? error.message : "unknown error"}`;
      publish();
    }
  }

  function scheduleReconnect() {
    if (!started || !endpoint || reconnectTimer !== null) return;
    phase = "reconnecting";
    publish();
    const delay = reconnectDelay;
    reconnectDelay = Math.min(reconnectDelay * 2, MAX_DELAY);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);
  }

  function forceReconnect(reason) {
    if (!started || !endpoint) return;
    if (reason) lastError = reason;
    phase = "reconnecting";
    publish();
    clearHeartbeat();
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (socket && (isOpen(socket) || socket.readyState === WebSocket.CONNECTING)) {
      try { socket.close(); } catch { /* already closing */ }
    }
    socket = null;
    scheduleReconnect();
  }

  function connect() {
    if (!started) return;
    if (!endpoint) {
      phase = "offline";
      publish();
      return;
    }
    if (typeof WebSocket === "undefined") {
      phase = "offline";
      lastError = "WebSocket is not supported in this browser";
      publish();
      return;
    }
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) return;

    phase = openedAt === null ? "connecting" : "reconnecting";
    publish();

    let candidate;
    try {
      candidate = new WebSocket(endpoint);
    } catch (error) {
      lastError = error && error.message ? error.message : "WebSocket construction failed";
      phase = "reconnecting";
      publish();
      scheduleReconnect();
      return;
    }
    socket = candidate;

    candidate.onopen = () => {
      if (socket !== candidate) return;
      openedAt = Date.now();
      lastSeq = null;
      phase = "live";
      lastMessageAt = null;
      lastError = "";
      reconnectDelay = MIN_DELAY;
      if (reconnectTimer !== null) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      publish();
      // Every (re)connect re-sends the full topic list: the server replays the
      // latest strategy frame and a demo snapshot, which is what reseeds us.
      subscribe(candidate);
      clearHeartbeat();
      heartbeatTimer = setInterval(() => {
        if (socket !== candidate || !isOpen(candidate)) return;
        const reference = lastMessageAt || openedAt;
        if (reference && Date.now() - reference > HEARTBEAT_TIMEOUT) {
          forceReconnect("WebSocket heartbeat timed out");
          return;
        }
        sendHeartbeat();
      }, HEARTBEAT_EVERY);
    };

    candidate.onmessage = (event) => {
      if (socket !== candidate) return;
      let frame;
      try {
        frame = JSON.parse(event.data);
      } catch {
        lastError = "received malformed WebSocket JSON";
        publish();
        return;
      }
      if (!frame || typeof frame !== "object") return;
      lastMessageAt = Date.now();
      receivedFrames += 1;
      if (frame.type !== "heartbeat") lastDataAt = lastMessageAt;
      if (frame.replay !== true && Number.isFinite(Number(frame.seq))) {
        const sequence = Number(frame.seq);
        if (lastSeq !== null && sequence > lastSeq + 1) dispatch({ type: "gap", data: { expected: lastSeq + 1, received: sequence } });
        lastSeq = sequence;
      }
      publish();
      dispatch(frame);
    };

    candidate.onerror = () => {
      if (socket !== candidate) return;
      lastError = "WebSocket connection error";
      phase = "reconnecting";
      publish();
      try { candidate.close(); } catch { /* already closed */ }
    };

    candidate.onclose = () => {
      if (socket !== candidate) return;
      clearHeartbeat();
      socket = null;
      phase = "reconnecting";
      publish();
      scheduleReconnect();
    };
  }

  function handleVisibility() {
    if (typeof document === "undefined" || document.visibilityState !== "visible" || !started) return;
    if (!endpoint || typeof WebSocket === "undefined") {
      publish();
      return;
    }
    if (!socket || (socket.readyState !== WebSocket.OPEN && socket.readyState !== WebSocket.CONNECTING)) {
      if (reconnectTimer !== null) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      reconnectDelay = MIN_DELAY;
      connect();
      return;
    }
    const reference = lastMessageAt || openedAt;
    if (isOpen(socket) && reference && Date.now() - reference > HEARTBEAT_TIMEOUT) {
      forceReconnect("WebSocket was silent while the page was hidden");
    }
  }

  function start() {
    if (started) return;
    started = true;
    connect();
    watchdogTimer = setInterval(() => {
      if (isOpen(socket)) {
        const reference = lastMessageAt || openedAt;
        if (reference && Date.now() - reference > HEARTBEAT_TIMEOUT) {
          forceReconnect("WebSocket heartbeat timed out");
          return;
        }
      }
      publish(); // refreshes the stale/live badge without waiting for a frame
    }, WATCHDOG_EVERY);
    if (typeof document !== "undefined" && document.addEventListener) {
      visibilityHandler = handleVisibility;
      document.addEventListener("visibilitychange", visibilityHandler);
    }
  }

  function stop() {
    started = false;
    if (reconnectTimer !== null) { clearTimeout(reconnectTimer); reconnectTimer = null; }
    if (watchdogTimer !== null) { clearInterval(watchdogTimer); watchdogTimer = null; }
    clearHeartbeat();
    if (visibilityHandler && typeof document !== "undefined" && document.removeEventListener) {
      document.removeEventListener("visibilitychange", visibilityHandler);
      visibilityHandler = null;
    }
    const closing = socket;
    socket = null;
    if (closing) {
      try { closing.close(); } catch { /* already closed */ }
    }
    phase = "offline";
    publish();
  }

  App.socket = {
    on(type, handler) {
      if (typeof handler === "function") handlers[type] = handler;
    },
    onState(listener) {
      if (typeof listener !== "function") return () => {};
      stateListeners.add(listener);
      return () => stateListeners.delete(listener);
    },
    start,
    stop,
    getState,
    sendHeartbeat,
  };
})((window.App = window.App || {}));
