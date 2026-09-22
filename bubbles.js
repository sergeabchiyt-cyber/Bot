/* ============================================================
 * OrderFlowBubbles — lightweight-charts v4 Series Primitive
 * Renders volume-weighted bubbles at the price where each
 * order-flow event occurred. Size and opacity scale by strength.
 * ============================================================ */

(function (global) {
  const EVENT_COLORS = {
    BUY_BUBBLE:  { r: 38,  g: 166, b: 154 },  // teal
    SELL_BUBBLE: { r: 239, g: 83,  b: 80  },  // coral
    ABS_BUY:     { r: 240, g: 185, b: 11  },  // gold
    ABS_SELL:    { r: 240, g: 185, b: 11  },  // gold
  };

  const MAX_RADIUS = 28;      // px at full strength
  const MIN_RADIUS = 5;       // px at minimum visible strength
  const MIN_ALPHA  = 0.25;
  const MAX_ALPHA  = 0.85;

  class OrderFlowBubblesRenderer {
    constructor() {
      this._data = [];
      this._chart = null;
      this._series = null;
      this._paneViews = [new OrderFlowBubblesPaneView(this)];
      // Scale reference — updated by the primitive owner when new events arrive
      this.maxStrength = 100;
    }

    attached(param) {
      this._chart = param.chart;
      this._series = param.series;
    }

    detached() {
      this._chart = null;
      this._series = null;
    }

    updateData(data) {
      this._data = data;
    }

    paneViews() {
      return this._paneViews;
    }

    priceToCoordinate(price) {
      return this._series.priceToCoordinate(price);
    }

    timeToCoordinate(time) {
      return this._chart.timeScale().timeToCoordinate(time);
    }
  }

  class OrderFlowBubblesPaneView {
    constructor(source) {
      this._source = source;
    }

    renderer() {
      return new OrderFlowBubblesRenderer2(this._source);
    }

    zOrder() {
      return "top";
    }
  }

  class OrderFlowBubblesRenderer2 {
    constructor(source) {
      this._source = source;
    }

    draw(target) {
      const chart = this._source._chart;
      const series = this._source._series;
      if (!chart || !series) return;

      target.useBitmapCoordinateSpace((scope) => {
        const ctx = scope.context;
        const dpr = scope.horizontalPixelRatio;

        for (const ev of this._source._data) {
          const x = this._source.timeToCoordinate(Math.floor(ev.timestamp / 1000));
          const y = this._source.priceToCoordinate(ev.level);
          if (x === null || y === null) continue;

          const color = EVENT_COLORS[ev.kind] || EVENT_COLORS.BUY_BUBBLE;
          const norm = Math.min(1, Math.max(0, ev.strength / this._source.maxStrength));
          const radius = (MIN_RADIUS + (MAX_RADIUS - MIN_RADIUS) * norm) * dpr;
          const alpha = MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * norm;
          const cx = x * dpr;
          const cy = y * dpr;

          // Outer glow
          const grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius * 1.6);
          grad.addColorStop(0, `rgba(${color.r},${color.g},${color.b},${alpha})`);
          grad.addColorStop(0.6, `rgba(${color.r},${color.g},${color.b},${alpha * 0.35})`);
          grad.addColorStop(1, `rgba(${color.r},${color.g},${color.b},0)`);
          ctx.fillStyle = grad;
          ctx.beginPath();
          ctx.arc(cx, cy, radius * 1.6, 0, Math.PI * 2);
          ctx.fill();

          // Inner core
          const coreGrad = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius);
          coreGrad.addColorStop(0, `rgba(${color.r},${color.g},${color.b},${Math.min(1, alpha + 0.15)})`);
          coreGrad.addColorStop(1, `rgba(${color.r},${color.g},${color.b},${alpha * 0.5})`);
          ctx.fillStyle = coreGrad;
          ctx.beginPath();
          ctx.arc(cx, cy, radius, 0, Math.PI * 2);
          ctx.fill();
        }
      });
    }
  }

  global.OrderFlowBubblesPrimitive = {
    create() {
      return new OrderFlowBubblesRenderer();
    },
    EVENT_COLORS,
  };
})(window);