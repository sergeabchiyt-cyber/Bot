/* ============================================================
 * Independent read-only WebSocket lifecycle for each service.
 * Each instance owns its socket, backoff, heartbeat, freshness timer, and
 * visibility recovery. REST snapshots remain service-module responsibilities.
 * ============================================================ */
(function (App) {
  "use strict";

  function createServiceStream(options) {
    const opts = options || {};
    const service = opts.service || "market";
    const endpoint = String(opts.endpoint || "").trim();
    const minDelay = Math.max(250, Number(opts.reconnectMinMs) || 1000);
    const maxDelay = Math.max(minDelay, Number(opts.reconnectMaxMs) || 30000);
    const heartbeatEvery = Math.max(5000, Number(opts.heartbeatIntervalMs) || 20000);
    const heartbeatTimeout = Math.max(heartbeatEvery * 2, Number(opts.heartbeatTimeoutMs) || 75000);
    const staleAfter = Math.max(heartbeatEvery, Number(opts.staleAfterMs) || 45000);

    let socket = null;
    let started = false;
    let connection = endpoint ? "offline" : "not-configured";
    let transportOpen = false;
    let reconnectDelay = minDelay;
    let reconnectTimer = null;
    let heartbeatTimer = null;
    let freshnessTimer = null;
    let visibilityHandler = null;
    let snapshotRefresh = typeof opts.refreshSnapshots === "function" ? opts.refreshSnapshots : null;
    let snapshotRefreshPromise = null;
    let lastMessageAt = null;
    let lastHeartbeatAt = null;
    let lastDataAt = null;
    let openedAt = null;
    let connectingSinceAt = null;
    let lastError = "";
    let receivedFrames = 0;

    function statusValue() {
      if (!endpoint) return "not-configured";
      if (transportOpen) {
        const reference = lastDataAt || openedAt;
        return reference && Date.now() - reference > staleAfter ? "stale" : "live";
      }
      if (connection === "connecting" || connection === "reconnecting") return "reconnecting";
      if (lastDataAt && Date.now() - lastDataAt > staleAfter) return "stale";
      return "offline";
    }

    function getState() {
      return {
        service,
        endpointConfigured: !!endpoint,
        connection,
        status: statusValue(),
        connected: transportOpen,
        lastMessageAt,
        lastHeartbeatAt,
        lastDataAt,
        lastError,
        receivedFrames,
      };
    }

    function publishState() {
      const current = getState();
      if (App.utils && App.utils.updateServiceStatus) {
        App.utils.updateServiceStatus(service, current.status, current);
      }
      if (typeof opts.onState === "function") {
        try { opts.onState(current); } catch (error) { console.error(`${service} state handler failed:`, error); }
      }
    }

    function refreshSnapshots() {
      if (typeof snapshotRefresh !== "function" || snapshotRefreshPromise) return snapshotRefreshPromise;
      try {
        snapshotRefreshPromise = Promise.resolve(snapshotRefresh())
          .catch((error) => console.warn(`${service} REST snapshot refresh failed:`, error))
          .finally(() => { snapshotRefreshPromise = null; });
      } catch (error) {
        console.warn(`${service} REST snapshot refresh failed:`, error);
        snapshotRefreshPromise = null;
      }
      return snapshotRefreshPromise;
    }

    function sendHeartbeat(target) {
      if (typeof WebSocket === "undefined" || !target || target.readyState !== WebSocket.OPEN) return;
      try {
        target.send(JSON.stringify({ type: "heartbeat" }));
      } catch (error) {
        lastError = `heartbeat send failed: ${error && error.message ? error.message : "unknown error"}`;
      }
    }

    function clearHeartbeat() {
      if (heartbeatTimer !== null) {
        clearInterval(heartbeatTimer);
        heartbeatTimer = null;
      }
    }

    function scheduleReconnect() {
      if (!started || !endpoint || reconnectTimer !== null) return;
      connection = "reconnecting";
      transportOpen = false;
      publishState();
      refreshSnapshots();
      reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        connect();
      }, reconnectDelay);
      reconnectDelay = Math.min(reconnectDelay * 2, maxDelay);
    }

    function forceReconnect(reason) {
      if (!started || !endpoint) return;
      if (reason) lastError = reason;
      transportOpen = false;
      connection = "reconnecting";
      publishState();
      if (reconnectTimer !== null) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      if (typeof WebSocket !== "undefined" && socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
        try { socket.close(); } catch {}
      }
      scheduleReconnect();
    }

    function connect() {
      if (!started) return;
      if (!endpoint) {
        connection = "not-configured";
        transportOpen = false;
        publishState();
        return;
      }
      if (typeof WebSocket === "undefined") {
        connection = "offline";
        transportOpen = false;
        lastError = "WebSocket is not supported in this browser";
        publishState();
        return;
      }
      if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) return;

      connection = openedAt === null && reconnectTimer === null ? "connecting" : "reconnecting";
      transportOpen = false;
      connectingSinceAt = Date.now();
      publishState();
      let candidate;
      try {
        candidate = new WebSocket(endpoint);
      } catch (error) {
        lastError = error && error.message ? error.message : "WebSocket construction failed";
        connection = "reconnecting";
        publishState();
        scheduleReconnect();
        return;
      }
      socket = candidate;

      candidate.onopen = () => {
        if (socket !== candidate) return;
        transportOpen = true;
        connection = "live";
        openedAt = Date.now();
        connectingSinceAt = null;
        lastMessageAt = null;
        lastError = "";
        reconnectDelay = minDelay;
        if (reconnectTimer !== null) {
          clearTimeout(reconnectTimer);
          reconnectTimer = null;
        }
        publishState();

        if (Array.isArray(opts.topics) && opts.topics.length) {
          try {
            candidate.send(JSON.stringify({ type: "subscribe", topics: opts.topics }));
          } catch (error) {
            lastError = `subscribe failed: ${error && error.message ? error.message : "unknown error"}`;
            publishState();
          }
        }
        clearHeartbeat();
        heartbeatTimer = setInterval(() => {
          if (socket !== candidate || candidate.readyState !== WebSocket.OPEN) return;
          const lastActivityAt = lastMessageAt || openedAt;
          if (lastActivityAt && Date.now() - lastActivityAt > heartbeatTimeout) {
            forceReconnect("WebSocket heartbeat timed out");
            return;
          }
          sendHeartbeat(candidate);
        }, heartbeatEvery);
        refreshSnapshots();
      };

      candidate.onmessage = (event) => {
        if (socket !== candidate) return;
        let frame;
        try {
          frame = JSON.parse(event.data);
        } catch {
          lastError = "received malformed WebSocket JSON";
          publishState();
          return;
        }
        if (!frame || typeof frame !== "object") return;
        lastMessageAt = Date.now();
        receivedFrames += 1;
        if (frame.type === "heartbeat") {
          lastHeartbeatAt = lastMessageAt;
        } else {
          lastDataAt = lastMessageAt;
          lastError = "";
        }
        publishState();
        if (typeof opts.onFrame === "function") {
          try { opts.onFrame(frame); } catch (error) { console.error(`${service} frame handler failed:`, error); }
        }
      };

      candidate.onerror = () => {
        if (socket !== candidate) return;
        lastError = "WebSocket connection error";
        connection = "reconnecting";
        transportOpen = false;
        publishState();
        scheduleReconnect();
        try { candidate.close(); } catch {}
      };

      candidate.onclose = () => {
        if (socket !== candidate) return;
        clearHeartbeat();
        transportOpen = false;
        connectingSinceAt = null;
        connection = "offline";
        socket = null;
        publishState();
        scheduleReconnect();
      };
    }

    function handleVisibility() {
      if (typeof document === "undefined" || document.visibilityState !== "visible" || !started) return;
      refreshSnapshots();
      if (!endpoint || typeof WebSocket === "undefined") {
        publishState();
        return;
      }
      if (!socket || (socket.readyState !== WebSocket.OPEN && socket.readyState !== WebSocket.CONNECTING)) {
        if (reconnectTimer !== null) {
          clearTimeout(reconnectTimer);
          reconnectTimer = null;
        }
        reconnectDelay = minDelay;
        connect();
        return;
      }
      const lastActivityAt = lastMessageAt || openedAt;
      if (transportOpen && lastActivityAt && Date.now() - lastActivityAt > heartbeatTimeout) {
        forceReconnect("WebSocket was silent while the page was hidden");
      } else if (socket.readyState === WebSocket.CONNECTING && connectingSinceAt && Date.now() - connectingSinceAt > heartbeatTimeout) {
        forceReconnect("WebSocket connection stalled while the page was hidden");
      }
    }

    function start() {
      if (started) return;
      started = true;
      publishState();
      refreshSnapshots();
      connect();
      freshnessTimer = setInterval(publishState, 5000);
      if (typeof document !== "undefined" && document.addEventListener) {
        visibilityHandler = handleVisibility;
        document.addEventListener("visibilitychange", visibilityHandler);
      }
    }

    function stop() {
      started = false;
      clearHeartbeat();
      if (freshnessTimer !== null) {
        clearInterval(freshnessTimer);
        freshnessTimer = null;
      }
      if (reconnectTimer !== null) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      if (visibilityHandler && typeof document !== "undefined" && document.removeEventListener) {
        document.removeEventListener("visibilitychange", visibilityHandler);
      }
      visibilityHandler = null;
      if (socket) {
        const previous = socket;
        socket = null;
        try { previous.close(); } catch {}
      }
      transportOpen = false;
      connection = endpoint ? "offline" : "not-configured";
      publishState();
    }

    return {
      start,
      connect,
      stop,
      getState,
      refreshSnapshots,
      setSnapshotRefresh(callback) {
        snapshotRefresh = typeof callback === "function" ? callback : null;
      },
    };
  }

  App.createServiceStream = createServiceStream;
})((window.App = window.App || {}));
