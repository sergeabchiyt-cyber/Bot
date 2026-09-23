/* ============================================================
 * XAUUSD Terminal — chart (lightweight-charts v4.2.0)
 * Candles, historical load, price lines.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, setStatus, fmtPrice, toSec } = App.utils;
  const cfg = App.config;

  const el = $("chart");

  if (typeof LightweightCharts === "undefined") {
    console.error("lightweight-charts failed to load");
    setStatus("chart lib error", "error");
    return;
  }

  const size = () => ({
    width: Math.max(1, el.clientWidth),
    height: Math.max(1, el.clientHeight),
  });

  const isNarrow = () => window.matchMedia("(max-width: 720px)").matches;

  const chart = LightweightCharts.createChart(el, {
    ...size(),
    layout: {
      background: { color: "#0B0E11" },
      textColor: "#7D8590",
      fontFamily: "'Inter', -apple-system, sans-serif",
      fontSize: isNarrow() ? 10 : 11,
    },
    grid: {
      vertLines: { color: "rgba(255,255,255,0.03)" },
      horzLines: { color: "rgba(255,255,255,0.03)" },
    },
    crosshair: {
      mode: LightweightCharts.CrosshairMode.Normal,
      vertLine: { color: "rgba(255,255,255,0.15)", width: 1, style: 2 },
      horzLine: { color: "rgba(255,255,255,0.15)", width: 1, style: 2 },
    },
    rightPriceScale: { borderColor: "rgba(255,255,255,0.06)" },
    timeScale: {
      borderColor: "rgba(255,255,255,0.06)",
      timeVisible: true,
      secondsVisible: false,
      rightOffset: 4,
    },
    handleScale: { axisPressedMouseMove: true, pinch: true },
    handleScroll: { horzTouchDrag: true, vertTouchDrag: false },
  });

  const series = chart.addCandlestickSeries({
    upColor: "#26A69A",
    downColor: "#EF5350",
    borderUpColor: "#26A69A",
    borderDownColor: "#EF5350",
    wickUpColor: "#26A69A",
    wickDownColor: "#EF5350",
  });

  // ---------- Top bar price ----------
  let lastClose = null;
  function updateLastPrice(close) {
    const priceEl = $("last-price");
    if (!priceEl) return;
    priceEl.textContent = fmtPrice(close);
    if (lastClose != null && close !== lastClose) {
      priceEl.dataset.dir = close > lastClose ? "up" : "down";
    }
    lastClose = close;
  }

  // ---------- History ----------
  async function loadHistory() {
    setStatus("loading", "busy");
    try {
      const res = await fetch(cfg.endpoints.klines);
      if (!res.ok) throw new Error(`Binance REST ${res.status}`);
      const raw = await res.json();
      const data = raw.map((k) => ({
        time: Math.floor(k[0] / 1000),
        open: parseFloat(k[1]),
        high: parseFloat(k[2]),
        low: parseFloat(k[3]),
        close: parseFloat(k[4]),
      }));
      series.setData(data);
      chart.timeScale().fitContent();
      if (data.length) updateLastPrice(data[data.length - 1].close);
    } catch (e) {
      console.error("History load failed:", e);
      setStatus("history error", "error");
    }
  }

  function updateCandle(c) {
    const time = toSec(c && c.time);
    if (time == null) return;
    series.update({ time, open: c.open, high: c.high, low: c.low, close: c.close });
    updateLastPrice(c.close);
  }

  // ---------- Price lines ----------
  const priceLines = {};

  function upsertPriceLine(key, price, style) {
    if (priceLines[key]) series.removePriceLine(priceLines[key]);
    priceLines[key] = series.createPriceLine({
      price,
      color: style.color,
      lineWidth: 1,
      lineStyle: style.dashed
        ? LightweightCharts.LineStyle.Dashed
        : LightweightCharts.LineStyle.Solid,
      axisLabelVisible: true,
      title: isNarrow() ? "" : style.title, // axis label only on mobile
    });
  }

  function clearPriceLines() {
    for (const k of Object.keys(priceLines)) {
      series.removePriceLine(priceLines[k]);
      delete priceLines[k];
    }
  }

  /** Force the primitive layer to repaint. */
  function repaint() {
    chart.applyOptions({});
    series.applyOptions({});
  }

  new ResizeObserver(() => chart.applyOptions(size())).observe(el);

  App.chart = {
    chart, series, loadHistory, updateCandle,
    upsertPriceLine, clearPriceLines, repaint,
  };
})((window.App = window.App || {}));
