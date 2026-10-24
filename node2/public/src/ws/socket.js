// socket.js — reconnecting WebSocket client for Node 1 and Node 4 feeds.
//
// Contract:
//   - On open, immediately send the service subscribe frame.
//   - On reconnect, re-subscribe (the server replays all snapshots — panels
//     are overwritten, not appended).
//   - Track lastMsgTs for heartbeat liveness (see lib/freshness.js).
//   - Exponential backoff with jitter on close/error; retries forever — the
//     dashboard's job is to stay informed about a remote service.
//
// The WebSocket constructor, sleep and clock are injectable so tests can
// drive connect/reconnect/heartbeat scenarios deterministically.

export function defaultSleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function createReconnectingSocket({
  url,
  subscribeMessage,
  onMessage,
  onStateChange,
  WebSocketImpl,
  sleep = defaultSleep,
  now = () => Date.now(),
  heartbeatMs = 30000,
  baseDelayMs = 1000,
  maxDelayMs = 30000,
}) {
  if (typeof WebSocketImpl !== "function") {
    throw new Error("createReconnectingSocket: WebSocketImpl is required");
  }

  let ws = null;
  let closedByUser = false;
  let attempt = 0;
  let lastMsgTs = 0;
  let connState = "connecting"; // connecting | open | reconnecting | closed
  let timer = null;

  function setState(next) {
    if (connState !== next) {
      connState = next;
      if (typeof onStateChange === "function") onStateChange(next);
    }
  }

  function scheduleReconnect() {
    if (closedByUser) return;
    const base = Math.min(maxDelayMs, baseDelayMs * 2 ** attempt);
    const jitter = Math.floor(Math.random() * Math.max(1, Math.min(base, 4000) * 0.25));
    const delay = Math.min(maxDelayMs, base + jitter);
    attempt += 1;
    setState("reconnecting");
    timer = setTimeout(() => {
      timer = null;
      connect();
    }, delay);
  }

  function connect() {
    if (closedByUser) return;
    setState(attempt === 0 ? "connecting" : "reconnecting");
    try {
      ws = new WebSocketImpl(url);
    } catch (err) {
      scheduleReconnect();
      return;
    }
    ws.onopen = () => {
      attempt = 0;
      lastMsgTs = now();
      setState("open");
      try {
        ws.send(JSON.stringify(subscribeMessage));
      } catch (err) {
        scheduleReconnect();
      }
    };
    ws.onmessage = (event) => {
      lastMsgTs = now();
      if (typeof onMessage === "function") onMessage(event && event.data);
    };
    ws.onclose = () => {
      if (closedByUser) return;
      scheduleReconnect();
    };
    ws.onerror = () => {
      // The close event that follows will trigger the reconnect; do not
      // schedule here to avoid double-scheduling.
    };
  }

  function close() {
    closedByUser = true;
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (ws && typeof ws.close === "function") {
      try {
        ws.close();
      } catch (err) {
        /* ignore */
      }
    }
    setState("closed");
  }

  connect();

  return {
    close,
    get state() {
      return { state: connState, lastMsgTs, attempt };
    },
    // Exposed for tests / debugging.
    _getLastMsgTs: () => lastMsgTs,
    _getAttempt: () => attempt,
    _forceReconnect: () => {
      if (ws && typeof ws.close === "function") ws.close();
    },
  };
}
