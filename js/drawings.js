/* ============================================================
 * XAUUSD Terminal — Node 3 drawing overlay.
 *
 * A lightweight-charts primitive renders the published geometry directly on
 * the existing candlestick pane. Identity is id + timeframe, so the same level
 * on 1H and daily can both be drawn. Colours and line styles come only from
 * /drawings; rows are rendered, never used to derive levels or setups.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory(root && root.App);
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.drawings = api;
  }
})(typeof window !== "undefined" ? window : null, function (App) {
  "use strict";

  App = App || {};
  const chartApi = App.chart || null;
  const series = chartApi && chartApi.series ? chartApi.series : null;
  const axis = chartApi && chartApi.axis ? chartApi.axis : null;
  const utils = App.utils || {};

  const state = {
    timestamp: null,
    items: [],
    keys: [],
    renderedKeys: [],
    pending: [],
    attached: false,
  };

  function safeColor(value) {
    const text = String(value || "").trim();
    if (/^#[\da-f]{3,8}$/i.test(text)) return text;
    if (/^(rgb|hsl)a?\(\s*[\d.,%\s/-]+\s*\)$/i.test(text)) return text;
    if (/^[A-Za-z]{3,20}$/.test(text)) return text;
    return "#8B949E";
  }

  function dashFor(style) {
    if (style === "dotted") return [2, 4];
    if (style === "dashed") return [7, 4];
    return [];
  }

  function coordinateForTime(scope, timeMs) {
    const attachedChart = primitiveState.chart;
    if (!attachedChart || !axis || timeMs === null || timeMs === undefined) return null;
    const realSec = Math.floor(Number(timeMs) / 1000);
    if (!Number.isFinite(realSec)) return null;
    const displaySec = typeof axis.displayForReal === "function"
      ? axis.displayForReal(realSec)
      : typeof axis.displayFor === "function" ? axis.displayFor(realSec) : realSec;
    if (displaySec === null || displaySec === undefined) return null;
    try {
      const coordinate = attachedChart.timeScale().timeToCoordinate(displaySec);
      return Number.isFinite(coordinate) ? coordinate : null;
    } catch {
      return null;
    }
  }

  const primitiveState = {
    chart: null,
    series: null,
    requestUpdate: null,
    items: [],
  };

  function drawLevelLine(ctx, scope, item) {
    const price = Number(item.price);
    const y = primitiveState.series.priceToCoordinate(price);
    if (!Number.isFinite(y)) return false;
    const width = scope.mediaSize.width;
    let x = 0;
    if (item.timeframe === "daily") {
      x = coordinateForTime(scope, item.ray_from_time ?? item.start);
      if (x === null) return false;
      x = Math.max(0, Math.min(width, x));
    }
    const color = safeColor(item.color);
    ctx.save();
    ctx.globalAlpha = item.timeframe === "daily" ? 0.9 : 0.62;
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = item.timeframe === "daily" ? 1.4 : 1;
    ctx.setLineDash(dashFor(item.line_style));
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(width, y);
    ctx.stroke();
    if (item.label) {
      ctx.setLineDash([]);
      ctx.font = "9px Inter, sans-serif";
      ctx.textBaseline = "bottom";
      ctx.fillText(item.label, Math.min(width - 60, x + 4), y - 2);
    }
    ctx.restore();
    return true;
  }

  function drawFrvp(ctx, scope, item) {
    const rows = Array.isArray(item.rows) ? item.rows : [];
    if (!rows.length) return;
    const width = scope.mediaSize.width;
    let xStart = coordinateForTime(scope, item.start);
    let xEnd = coordinateForTime(scope, item.end);
    if (xStart === null) xStart = 0;
    if (xEnd === null || xEnd <= xStart) xEnd = Math.min(width, xStart + Math.max(60, width * 0.42));
    const span = Math.max(2, xEnd - xStart);
    const maxVolume = rows.reduce((max, row) => Math.max(max, Number(row.volume) || 0), 0) || 1;
    const color = safeColor(item.color);
    const rectangle = item.line_style === "rectangle";
    ctx.save();
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.font = "9px Inter, sans-serif";
    ctx.setLineDash(dashFor(item.line_style));
    for (const row of rows) {
      const yHigh = primitiveState.series.priceToCoordinate(Number(row.high));
      const yLow = primitiveState.series.priceToCoordinate(Number(row.low));
      if (!Number.isFinite(yHigh) || !Number.isFinite(yLow)) continue;
      const top = Math.min(yHigh, yLow);
      const height = Math.max(1, Math.abs(yLow - yHigh));
      const barWidth = Math.max(2, span * ((Number(row.volume) || 0) / maxVolume));
      const up = Math.max(0, Number(row.up_volume) || 0);
      const down = Math.max(0, Number(row.down_volume) || 0);
      const upRatio = up + down > 0 ? up / (up + down) : 0.5;
      const upWidth = barWidth * upRatio;
      if (rectangle) {
        // Same published colour, two opacities: up and down volume split.
        ctx.globalAlpha = 0.28;
        ctx.fillRect(xStart, top, upWidth, height);
        ctx.globalAlpha = 0.13;
        ctx.fillRect(xStart + upWidth, top, barWidth - upWidth, height);
      } else {
        ctx.globalAlpha = 0.55;
        ctx.strokeRect(xStart + 0.5, top + 0.5, Math.max(1, upWidth - 1), height - 1);
        ctx.globalAlpha = 0.28;
        ctx.strokeRect(xStart + upWidth + 0.5, top + 0.5, Math.max(1, barWidth - upWidth - 1), height - 1);
      }
      ctx.globalAlpha = 0.45;
      ctx.strokeRect(xStart + 0.5, top + 0.5, barWidth - 1, height - 1);
    }
    ctx.globalAlpha = 0.9;
    ctx.fillText(item.label, xStart + 4, Math.max(10, scope.mediaSize.height * 0.08));
    ctx.restore();
  }

  const renderer = {
    draw(target) {
      if (!primitiveState.chart || !primitiveState.series) return;
      target.useMediaCoordinateSpace((scope) => {
        const ctx = scope.context;
        state.renderedKeys = [];
        for (const item of primitiveState.items) {
          if (item.kind === "frvp") {
            drawFrvp(ctx, scope, item);
            state.renderedKeys.push(item.key);
          } else if (drawLevelLine(ctx, scope, item)) {
            state.renderedKeys.push(item.key);
          }
        }
      });
    },
  };
  const paneView = { renderer: () => renderer, zOrder: () => "bottom" };

  const primitive = {
    attached({ chart, series: attachedSeries, requestUpdate }) {
      primitiveState.chart = chart;
      primitiveState.series = attachedSeries;
      primitiveState.requestUpdate = requestUpdate;
      state.attached = true;
    },
    detached() {
      primitiveState.chart = null;
      primitiveState.series = null;
      primitiveState.requestUpdate = null;
      state.attached = false;
    },
    updateAllViews() {},
    paneViews() { return [paneView]; },
    update(items, timestamp) {
      primitiveState.items = items || [];
      state.timestamp = timestamp ?? null;
      if (typeof primitiveState.requestUpdate === "function") primitiveState.requestUpdate();
    },
  };

  if (series && typeof series.attachPrimitive === "function") {
    try { series.attachPrimitive(primitive); } catch (error) { console.error("drawing primitive unavailable:", error); }
  }
  if (chartApi && typeof chartApi.onReset === "function") {
    chartApi.onReset(() => primitive.update(state.items, state.timestamp));
  }

  function normalise(payload) {
    if (App.client && typeof App.client.parseDrawings === "function") {
      return App.client.parseDrawings(payload);
    }
    const source = payload && typeof payload === "object" ? payload : {};
    const items = Array.isArray(source.items) ? source.items : [];
    return { timestamp: source.timestamp ?? null, items, keys: items.map((item) => `${item.id}::${item.timeframe}`), pending: [] };
  }

  function apply(payload) {
    const parsed = normalise(payload);
    state.timestamp = parsed.timestamp;
    state.items = parsed.items || [];
    state.keys = parsed.keys || state.items.map((item) => item.key);
    state.pending = parsed.pending || [];
    state.renderedKeys = state.keys.slice();
    primitive.update(state.items, state.timestamp);
    if (App.panels && typeof App.panels.renderDrawingsLegend === "function") {
      App.panels.renderDrawingsLegend();
    }
    return parsed;
  }

  function requestUpdate() {
    primitive.update(state.items, state.timestamp);
  }

  return {
    apply,
    requestUpdate,
    getState() { return { ...state, items: state.items.slice(), keys: state.keys.slice() }; },
    keys() { return state.keys.slice(); },
    primitive,
  };
});
