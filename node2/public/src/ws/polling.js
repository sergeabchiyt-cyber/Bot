// polling.js — REST polling fallback used while a WebSocket feed is
// disconnected (Node 1 contract: poll /strategy + /demo every 5s and mark
// "delayed" on the affected panels). It never issues writes.

export function createPollingFallback({ poll, intervalMs = 5000, now = () => Date.now() }) {
  if (typeof poll !== "function") {
    throw new Error("createPollingFallback: poll is required");
  }
  let timer = null;
  let delayed = false;
  let lastPollTs = 0;
  const listeners = new Set();

  function emit() {
    for (const l of listeners) {
      try {
        l();
      } catch (err) {
        console.error("polling listener error", err);
      }
    }
  }

  async function tick() {
    try {
      await poll();
      lastPollTs = now();
      emit();
    } catch (err) {
      // Polling failures are non-fatal; the next tick retries.
    }
  }

  function start() {
    if (timer) return;
    delayed = true;
    timer = setInterval(tick, intervalMs);
    emit();
  }

  function stop() {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    if (delayed) {
      delayed = false;
      emit();
    }
  }

  return {
    start,
    stop,
    isDelayed: () => delayed,
    lastPollTs: () => lastPollTs,
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}
