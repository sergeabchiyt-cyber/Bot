/* ============================================================
 * XAUUSD Terminal — chart overlays for the demo position.
 *
 * Draws only what Node 1 supplies:
 *   · entry / SL / TP price lines for the open demo position
 *   · a buy/sell marker on the candle where the current action began
 *   · an optional translucent band at decision price ± ATR
 *
 * Only the demo position and the strategy decision are drawn.
 * ============================================================ */
(function (App) {
  "use strict";

  if (!App.chart || !App.chart.series) {
    console.error("Chart unavailable — position overlays disabled");
    return;
  }

  const chart = App.chart;
  const format = App.format;
  const math = App.strategyMath;

  const COLOR_BUY = "#26A69A";
  const COLOR_SELL = "#EF5350";
  const COLOR_SL = "#EF5350";
  const COLOR_TP = "#22C55E";
  const LINE_KEYS = ["pos-entry", "pos-sl", "pos-tp"];

  let position = null;
  let decision = null;

  // ---------- Price lines ----------

  function applyPositionLines() {
    for (const key of LINE_KEYS) chart.removePriceLine(key);
    if (!position) return;
    const side = position.side === "sell" ? "sell" : "buy";
    const lots = format.lots(position.lots);
    const label = `${side.toUpperCase()} ${lots}`;
    chart.upsertPriceLine("pos-entry", position.entry, {
      color: side === "buy" ? COLOR_BUY : COLOR_SELL,
      title: `${label} \u00b7 entry ${format.price(position.entry)}`,
      compactTitle: label,
      dashed: false,
    });
    if (position.stop_loss !== null) {
      chart.upsertPriceLine("pos-sl", position.stop_loss, {
        color: COLOR_SL,
        title: `${label} \u00b7 SL ${format.price(position.stop_loss)}`,
        compactTitle: `${label} \u00b7 SL`,
        dashed: true,
      });
    }
    if (position.take_profit !== null) {
      chart.upsertPriceLine("pos-tp", position.take_profit, {
        color: COLOR_TP,
        title: `${label} \u00b7 TP ${format.price(position.take_profit)}`,
        compactTitle: `${label} \u00b7 TP`,
        dashed: true,
      });
    }
  }

  // ---------- Action marker ----------

  function applyMarker() {
    if (typeof chart.series.setMarkers !== "function") return;
    const run = math.lastDirectionalChange(App.strategy.getState().history);
    if (!run) {
      chart.series.setMarkers([]);
      return;
    }
    const action = math.actionOf(run);
    const seconds = Math.floor(run.ts / 1000);
    const display = chart.axis.displayForReal(seconds);
    if (display === null) {
      // The candle is not on the loaded chart (yet). The history-load reset
      // listener re-applies markers after /candles arrives.
      chart.series.setMarkers([]);
      return;
    }
    chart.series.setMarkers([{
      time: display,
      position: action === "buy" ? "belowBar" : "aboveBar",
      color: action === "buy" ? COLOR_BUY : COLOR_SELL,
      shape: action === "buy" ? "arrowUp" : "arrowDown",
      text: action.toUpperCase(),
    }]);
  }

  // ---------- ATR band ----------

  function createBandPrimitive() {
    const state = { chart: null, series: null, requestUpdate: null, low: null, high: null };
    const renderer = {
      draw(target) {
        if (!state.series || state.low === null || state.high === null) return;
        target.useMediaCoordinateSpace((scope) => {
          const ctx = scope.context;
          const width = scope.mediaSize.width;
          const yHigh = state.series.priceToCoordinate(state.high);
          const yLow = state.series.priceToCoordinate(state.low);
          if (yHigh === null || yLow === null) return;
          const top = Math.min(yHigh, yLow);
          const height = Math.abs(yLow - yHigh);
          if (height < 1) return;
          ctx.save();
          ctx.fillStyle = "rgba(240, 185, 11, 0.055)";
          ctx.fillRect(0, top, width, height);
          ctx.strokeStyle = "rgba(240, 185, 11, 0.22)";
          ctx.setLineDash([3, 4]);
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(0, top);
          ctx.lineTo(width, top);
          ctx.moveTo(0, top + height);
          ctx.lineTo(width, top + height);
          ctx.stroke();
          ctx.restore();
        });
      },
    };
    const view = { renderer: () => renderer, zOrder: () => "bottom" };
    return {
      attached({ chart: attachedChart, series, requestUpdate }) {
        state.chart = attachedChart;
        state.series = series;
        state.requestUpdate = requestUpdate;
      },
      detached() {
        state.chart = null;
        state.series = null;
        state.requestUpdate = null;
      },
      updateAllViews() {},
      paneViews() { return [view]; },
      update(low, high) {
        state.low = low;
        state.high = high;
        if (typeof state.requestUpdate === "function") state.requestUpdate();
      },
    };
  }

  let band = null;
  if (typeof chart.series.attachPrimitive === "function") {
    band = createBandPrimitive();
    chart.series.attachPrimitive(band);
  }

  function applyBand() {
    if (!band) return;
    const price = decision ? decision.price : null;
    const atr = decision ? decision.atr : null;
    if (price === null || atr === null || atr <= 0) {
      band.update(null, null);
    } else {
      band.update(price - atr, price + atr);
    }
  }

  // ---------- Wiring ----------

  function refresh() {
    applyPositionLines();
    applyMarker();
    applyBand();
  }

  App.demo.onUpdate((snapshot) => {
    position = snapshot && snapshot.position ? snapshot.position : null;
    applyPositionLines();
  });

  App.strategy.onUpdate((state) => {
    decision = state.last;
    applyMarker();
    applyBand();
  });

  if (typeof chart.onReset === "function") chart.onReset(refresh);
  if (typeof chart.onCandleUpdate === "function") chart.onCandleUpdate(applyMarker);
})((window.App = window.App || {}));
