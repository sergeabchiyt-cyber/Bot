/* ============================================================
 * XAUUSD Terminal — Order Flow Bubbles Primitive
 * lightweight-charts v4.2.0 ISeriesPrimitive
 *
 * Renders BUY/SELL/ABS bubbles as filled circles at trade price.
 * Radius scales with event.strength / primitive.maxStrength.
 * ============================================================ */

(function () {
  "use strict";

  const EVENT_STYLE = {
    BUY_BUBBLE:  { color: "#26A69A", label: "BUY"   },
    SELL_BUBBLE: { color: "#EF5350", label: "SELL"  },
    ABS_BUY:     { color: "#F0B90B", label: "ABS-B" },
    ABS_SELL:    { color: "#F0B90B", label: "ABS-S" },
  };

  function eventTimeToSeconds(ev) {
    const t = ev && ev.timestamp;
    if (typeof t !== "number") return null;
    // Heuristic: values > 1e12 are milliseconds.
    return t > 1e12 ? Math.floor(t / 1000) : t;
  }

  class BubblesRenderer {
    constructor(source) {
      this._source = source;
    }

    draw(target) {
      const src = this._source;
      const chart = src._chart;
      const series = src._series;
      const data = src._data;
      if (!chart || !series || !data || data.length === 0) return;

      target.useMediaCoordinateSpace((scope) => {
        const ctx = scope.context;
        const ts = chart.timeScale();
        const maxS = Math.max(src._maxStrength || 100, 1);

        for (let i = 0; i < data.length; i++) {
          const ev = data[i];
          const timeSec = eventTimeToSeconds(ev);
          if (timeSec === null) continue;

          const x = ts.timeToCoordinate(timeSec);
          const y = series.priceToCoordinate(ev.level);
          if (x === null || y === null) continue;

          const ratio = Math.max(0, Math.min(1, (ev.strength || 0) / maxS));
          const r = 3 + ratio * 12;
          const style = EVENT_STYLE[ev.kind] || EVENT_STYLE.BUY_BUBBLE;

          // Filled disc with a soft alpha + solid outline.
          ctx.beginPath();
          ctx.arc(x, y, r, 0, Math.PI * 2);
          ctx.fillStyle = style.color + "40";
          ctx.fill();

          ctx.beginPath();
          ctx.arc(x, y, r, 0, Math.PI * 2);
          ctx.strokeStyle = style.color;
          ctx.lineWidth = 1.5;
          ctx.stroke();
        }
      });
    }
  }

  class BubblesPaneView {
    constructor(source) {
      this._source = source;
      this._renderer = new BubblesRenderer(source);
    }
    renderer() {
      return this._renderer;
    }
    zOrder() {
      return "top";
    }
  }

  class OrderFlowBubblesPrimitive {
    constructor() {
      this._chart = null;
      this._series = null;
      this._data = [];
      this._maxStrength = 100;
      this._views = [new BubblesPaneView(this)];
    }

    attached({ chart, series }) {
      this._chart = chart;
      this._series = series;
    }

    detached() {
      this._chart = null;
      this._series = null;
    }

    // Called by lightweight-charts on every repaint. We read live
    // coordinates in the renderer, so there is nothing cached to
    // refresh — but this method MUST exist or the pane view never
    // invalidates and bubbles fail to render.
    updateAllViews() {
      // intentionally empty
    }

    paneViews() {
      return this._views;
    }

    updateData(events) {
      this._data = events || [];
    }

    get maxStrength() {
      return this._maxStrength;
    }

    set maxStrength(v) {
      this._maxStrength = (typeof v === "number" && v > 0) ? v : 100;
    }
  }

  window.OrderFlowBubblesPrimitive = {
    EVENT_STYLE,
    create() {
      return new OrderFlowBubblesPrimitive();
    },
  };
})();