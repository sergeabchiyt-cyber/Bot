/* ============================================================
 * XAUUSD Terminal — two independent read-only subscription routers.
 *
 * `App.client` owns both sockets. This module only routes frames: Node 1 is
 * market data and feed state; Node 3 is published levels, drawings, profiles,
 * setups, signals, and diagnostics. Node 3 frames are full replacements.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory(root && root.App);
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.streams = api;
  }
})(typeof window !== "undefined" ? window : null, function (App) {
  "use strict";

  App = App || {};
  const config = App.config || {};
  const utils = App.utils || {};
  const streams = { node1: null, node3: null };
  const states = { node1: null, node3: null };
  const handlers = { node1: new Map(), node3: new Map() };
  const stateListeners = new Set();

  function on(service, type, handler) {
    if (!handlers[service] || typeof handler !== "function") return () => {};
    handlers[service].set(type, handler);
    return () => handlers[service].delete(type);
  }

  function onState(listener) {
    if (typeof listener !== "function") return () => {};
    stateListeners.add(listener);
    return () => stateListeners.delete(listener);
  }

  function dispatch(service, frame) {
    const handler = handlers[service] && handlers[service].get(frame.type);
    if (!handler) return false;
    try { handler(frame.data, frame); } catch (error) { console.error(`${service} ${frame.type} handler failed:`, error); }
    return true;
  }

  function node1Frame(frame) {
    switch (frame.type) {
      case "candle":
        // A replay bar upserts by its known bucket time. Treating replay as
        // REST avoids incrementing the live revision and racing the full
        // /candles snapshot; only live frames advance that revision.
        if (App.chartControls && App.chartControls.acceptsCandle(frame.data) &&
            App.chart && typeof App.chart.updateCandle === "function") {
          App.chart.updateCandle(frame.data, frame.replay ? "rest" : "ws");
        }
        break;
      case "tick_volume":
        if (App.chart && typeof App.chart.updateTickVolume === "function") {
          App.chart.updateTickVolume(frame.data, frame.replay ? "rest" : "ws");
        }
        if (App.panels) App.panels.applyTopic("tick_volume", frame.data, frame.replay ? "replay" : "ws");
        break;
      case "calendar":
        if (App.panels) App.panels.applyTopic("calendar", frame.data, frame.replay ? "replay" : "ws");
        break;
      case "status":
        if (App.panels) App.panels.applyTopic("status", frame.data, frame.replay ? "replay" : "ws");
        break;
      case "gap":
        break; // client.createStream reports this through onGap
      default:
        break; // unknown types are ignored
    }
    dispatch("node1", frame);
  }

  function node3Frame(frame) {
    const topic = frame.type === "drawings" ? "drawing" : frame.type;
    if (["levels", "drawing", "vp", "scanning", "signals", "diagnostics"].includes(topic) && App.panels) {
      App.panels.applyTopic(topic, frame.data, frame.replay ? "replay" : "ws");
    }
    dispatch("node3", frame);
  }

  function paint(service, state) {
    states[service] = state;
    if (utils.setBadge) {
      const id = service === "node1" ? "socket-badge" : "node3-stream-badge";
      const prefix = service === "node1" ? "NODE 1 STREAM" : "NODE 3 STREAM";
      const status = String(state.status || "offline").toUpperCase();
      const stateValue = state.status === "live" ? "live"
        : ["stale", "connecting", "reconnecting"].includes(state.status) ? "stale" : "bad";
      utils.setBadge(id, `${prefix} ${status}`, stateValue,
        `${prefix}: ${status}${state.lastDataAt ? ` · last data ${App.format ? App.format.dateTime(state.lastDataAt) : ""}` : ""}`);
    }
    for (const listener of stateListeners) {
      try { listener(service, state, states); } catch (error) { console.error("stream state listener failed:", error); }
    }
  }

  function handleGap(info) {
    if (App.chartControls && typeof App.chartControls.handleGap === "function") {
      App.chartControls.handleGap(info);
      return;
    }
    if (App.panels && typeof App.panels.showGap === "function") App.panels.showGap(info);
  }

  function start() {
    if (!App.client || typeof App.client.createStream !== "function") return streams;
    if (!streams.node1) {
      streams.node1 = App.client.createStream("node1", {
        topics: config.services && config.services.node1 ? config.services.node1.topics : undefined,
        onFrame: node1Frame,
        onGap: handleGap,
        onState: (state) => paint("node1", state),
      });
      streams.node1.start();
    }
    if (!streams.node3) {
      streams.node3 = App.client.createStream("node3", {
        topics: config.services && config.services.node3 ? config.services.node3.topics : undefined,
        onFrame: node3Frame,
        onState: (state) => paint("node3", state),
      });
      streams.node3.start();
    }
    return streams;
  }

  function stop() {
    for (const service of ["node1", "node3"]) {
      if (streams[service]) streams[service].stop();
    }
  }

  function getState(service) {
    if (service) return states[service] ? { ...states[service] } : null;
    return {
      node1: states.node1 ? { ...states.node1 } : null,
      node3: states.node3 ? { ...states.node3 } : null,
    };
  }

  return { start, stop, on, onState, getState, handleGap };
});
