// chart.js — canvas candlestick chart.
//
// Pure geometry is separated from drawing: computeChartLayout() is deterministic
// and covered by tests; drawChart() only translates a layout onto a canvas.
//
// Visualization (Node 1 FRONTEND_NEXT.md §4):
//   - candles colored by direction (green up / red down)
//   - POC / VAH / VAL reference lines from the latest volume-profile levels
//   - entry / stop-loss / take-profit lines for the open demo position, with
//     an entry→TP progress bar and the stop
//   - ATR band around the latest close
//   - stale / disconnected overlays grey the chart and label the problem

export const PAD = { left: 8, right: 64, top: 16, bottom: 28 };

// Deterministic layout computation. bars: [{ time, open, high, low, close }]
// position: { side, entry, stopLoss, takeProfit } or null.
export function computeChartLayout(bars, {
  width,
  height,
  pad = PAD,
  position = null,
} = {}) {
  const plotW = Math.max(1, width - pad.left - pad.right);
  const plotH = Math.max(1, height - pad.top - pad.bottom);

  let min = Infinity;
  let max = -Infinity;
  for (const b of bars) {
    for (const v of [b.open, b.high, b.low, b.close]) {
      if (Number.isFinite(v)) {
        if (v < min) min = v;
        if (v > max) max = v;
      }
    }
  }

  // Include position lines in the price range so labels stay visible.
  if (position) {
    for (const v of [position.entry, position.stopLoss, position.takeProfit]) {
      if (Number.isFinite(v)) {
        if (v < min) min = v;
        if (v > max) max = v;
      }
    }
  }
  if (!Number.isFinite(min) || !Number.isFinite(max) || min === max) {
    min = 0;
    max = 1;
  }
  // Headroom so lines and candles do not touch the edges.
  const span = max - min;
  min -= span * 0.08;
  max += span * 0.08;

  const count = bars.length;
  const barW = count > 0 ? Math.max(1, (plotW / count) * 0.66) : 1;
  const stepX = count > 1 ? plotW / (count - 1) : plotW;

  const y = (price) => {
    const t = (price - min) / (max - min);
    return pad.top + (1 - t) * plotH;
  };
  const x = (i) => (count > 1 ? pad.left + i * stepX : pad.left + plotW / 2);

  // Position line geometry (y-coordinates + horizontal extents).
  const positionLines = position
    ? [
        {
          key: "entry",
          label: position.side === "sell" ? "Entry (sell)" : "Entry (buy)",
          price: position.entry,
          y: Number.isFinite(position.entry) ? y(position.entry) : null,
          tone: "entry",
        },
        {
          key: "sl",
          label: "Stop",
          price: position.stopLoss,
          y: Number.isFinite(position.stopLoss) ? y(position.stopLoss) : null,
          tone: "stop",
        },
        {
          key: "tp",
          label: "TP",
          price: position.takeProfit,
          y: Number.isFinite(position.takeProfit) ? y(position.takeProfit) : null,
          tone: "tp",
        },
      ]
    : [];

  // Entry→TP progress (fraction of the SL→TP span already traveled,
  // clamped conceptually to [0,1] by the progress bar rendering).
  let progress = null;
  if (position && Number.isFinite(position.entry)
    && Number.isFinite(position.stopLoss) && Number.isFinite(position.takeProfit)) {
    const span = position.takeProfit - position.stopLoss;
    if (span !== 0) {
      progress = position.side === "buy"
        ? (position.entry - position.stopLoss) / span
        : (position.takeProfit - position.entry) / span;
    }
  }

  return { plotW, plotH, pad, min, max, barW, stepX, count, x, y, positionLines, progress };
}

function fmtPriceShort(value) {
  if (!Number.isFinite(value)) return "—";
  return value.toFixed(2);
}

export function drawChart(canvas, { bars, position, levels, atr, freshnessState, interval, lastPrice, version }) {
  const ctx = canvas.getContext("2d");
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = Math.max(1, Math.floor(width * dpr));
  canvas.height = Math.max(1, Math.floor(height * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);

  const layout = computeChartLayout(bars, { width, height, position });
  const { pad, plotW, plotH, barW, x, y, min, max } = layout;

  // --- background --------------------------------------------------------
  ctx.fillStyle = "#0d1117";
  ctx.fillRect(0, 0, width, height);

  // --- grid -------------------------------------------------------------
  ctx.strokeStyle = "#21262d";
  ctx.lineWidth = 1;
  ctx.font = "11px ui-monospace, monospace";
  ctx.fillStyle = "#8b949e";
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  const gridLines = 5;
  for (let i = 0; i <= gridLines; i++) {
    const py = pad.top + (i * plotH) / gridLines;
    ctx.beginPath();
    ctx.moveTo(pad.left, py);
    ctx.lineTo(pad.left + plotW, py);
    ctx.stroke();
    const price = max - ((i * (max - min)) / gridLines);
    ctx.fillText(fmtPriceShort(price), width - pad.right + 6, py);
  }

  // --- candles ------------------------------------------------------------
  if (freshnessState === "stale" || freshnessState === "disconnected") {
    ctx.globalAlpha = 0.55;
  }
  bars.forEach((bar, i) => {
    const cx = x(i);
    const bodyTop = y(Math.max(bar.open, bar.close));
    const bodyBottom = y(Math.min(bar.open, bar.close));
    const up = bar.close >= bar.open;
    ctx.strokeStyle = up ? "#3fb950" : "#f85149";
    ctx.fillStyle = up ? "#3fb950" : "#f85149";
    // wick
    ctx.beginPath();
    ctx.moveTo(cx, y(bar.high));
    ctx.lineTo(cx, y(bar.low));
    ctx.stroke();
    // body
    const bodyH = Math.max(1, bodyBottom - bodyTop);
    ctx.fillRect(cx - barW / 2, bodyTop, barW, bodyH);
  });
  ctx.globalAlpha = 1;

  // --- position lines ----------------------------------------------------
  for (const line of layout.positionLines) {
    if (line.y == null) continue;
    ctx.strokeStyle = line.tone === "stop" ? "#f85149" : line.tone === "tp" ? "#3fb950" : "#d2a8ff";
    ctx.setLineDash(line.tone === "entry" ? [] : [6, 4]);
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    ctx.moveTo(pad.left, line.y);
    ctx.lineTo(pad.left + plotW, line.y);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = ctx.strokeStyle;
    ctx.fillText(line.label, pad.left + 4, line.y - 8);
  }

  // --- POC / VAH / VAL from the latest levels snapshot ---------------------
  if (Array.isArray(levels) && levels.length > 0) {
    const lv = levels[levels.length - 1];
    const refs = [
      { label: "POC", price: lv.poc, color: "#d2a8ff" },
      { label: "VAH", price: lv.vah, color: "#58a6ff" },
      { label: "VAL", price: lv.val, color: "#f0883e" },
    ];
    ctx.lineWidth = 1;
    ctx.setLineDash([2, 3]);
    for (const ref of refs) {
      if (!Number.isFinite(ref.price)) continue;
      const ry = y(ref.price);
      ctx.strokeStyle = ref.color;
      ctx.beginPath();
      ctx.moveTo(pad.left, ry);
      ctx.lineTo(pad.left + plotW, ry);
      ctx.stroke();
      ctx.fillStyle = ref.color;
      ctx.fillText(ref.label, pad.left + plotW - 34, ry - 8);
    }
    ctx.setLineDash([]);
  }

  // --- ATR band ----------------------------------------------------------
  if (Number.isFinite(atr) && Number.isFinite(lastPrice)) {
    const top = y(lastPrice + atr);
    const bottom = y(lastPrice - atr);
    ctx.fillStyle = "rgba(139,148,158,0.10)";
    ctx.fillRect(pad.left, top, plotW, Math.max(1, bottom - top));
    ctx.fillStyle = "#8b949e";
    ctx.fillText(`ATR ±${atr.toFixed(2)}`, pad.left + 4, top - 8);
  }

  // --- time axis ----------------------------------------------------------
  ctx.fillStyle = "#8b949e";
  ctx.textAlign = "center";
  ctx.textBaseline = "top";
  const labelCount = Math.min(6, bars.length);
  for (let i = 0; i < labelCount; i++) {
    const idx = labelCount === 1 ? 0 : Math.round((i * (bars.length - 1)) / (labelCount - 1));
    const bar = bars[idx];
    if (!bar || !Number.isFinite(bar.time)) continue;
    const d = new Date(bar.time * 1000);
    const label = `${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
    ctx.fillText(label, x(idx), height - pad.bottom + 6);
  }

  // --- freshness overlay --------------------------------------------------
  if (freshnessState === "stale" || freshnessState === "disconnected") {
    ctx.fillStyle = "rgba(13,17,23,0.72)";
    ctx.fillRect(0, 0, width, height);
    ctx.fillStyle = freshnessState === "stale" ? "#d29922" : "#f85149";
    ctx.font = "600 16px ui-sans-serif, system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(
      freshnessState === "stale" ? "Market data STALE — stream stalled" : "DISCONNECTED — reconnecting",
      width / 2,
      height / 2,
    );
  } else if (freshnessState === "unknown" && bars.length === 0) {
    ctx.fillStyle = "#8b949e";
    ctx.font = "13px ui-sans-serif, system-ui, sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText("Waiting for candles…", width / 2, height / 2);
  }
}
