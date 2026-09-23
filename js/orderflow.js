/* ============================================================
 * XAUUSD Terminal — order-flow bubbles (chart + panel)
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, fmtPrice, fmtClock, toMs, escapeHtml } = App.utils;
  const cfg = App.config.bubbles;

  const EVENT_STYLE = window.OrderFlowBubblesPrimitive
    ? window.OrderFlowBubblesPrimitive.EVENT_STYLE
    : {
        BUY_BUBBLE:  { color: "#26A69A", label: "BUY"   },
        SELL_BUBBLE: { color: "#EF5350", label: "SELL"  },
        ABS_BUY:     { color: "#F0B90B", label: "ABS-B" },
        ABS_SELL:    { color: "#F0B90B", label: "ABS-S" },
      };

  let primitive;
  if (window.OrderFlowBubblesPrimitive) {
    primitive = window.OrderFlowBubblesPrimitive.create();
    App.chart.series.attachPrimitive(primitive);
  } else {
    console.error("bubbles.js failed to load — chart bubbles disabled");
    primitive = { updateData() {}, maxStrength: cfg.scaleFloor };
  }

  const events = [];   // rolling window drawn on chart
  const rows = [];     // rolling window shown in panel
  let maxStrength = cfg.scaleFloor;

  // Decay the scale reference slowly so it recovers after a spike.
  setInterval(() => {
    if (maxStrength > cfg.scaleFloor) {
      maxStrength = Math.max(cfg.scaleFloor, maxStrength * cfg.decayFactor);
      primitive.maxStrength = maxStrength;
    }
  }, cfg.decayEveryMs);

  function renderRows() {
    const list = $("bubbles-list");
    $("bubbles-count").textContent = rows.length;
    if (!rows.length) {
      list.innerHTML = '<div class="empty">No bubbles detected</div>';
      return;
    }
    list.innerHTML = rows
      .map((b) => `
        <div class="bubble-row">
          <span class="bubble-dir" style="color:${b.style.color}">${b.style.label}</span>
          <span class="bubble-price">${fmtPrice(b.level)}</span>
          <span class="bubble-meta">${b.time} · <em>${escapeHtml(b.src)}</em></span>
        </div>`)
      .join("");
  }

  function add(ev) {
    if (!ev) return;
    events.push(ev);
    while (events.length > cfg.maxOnChart) events.shift();

    if (ev.strength > maxStrength) maxStrength = Math.min(cfg.scaleCap, ev.strength);
    primitive.maxStrength = maxStrength;
    primitive.updateData(events);
    App.chart.repaint();

    const ms = toMs(ev.timestamp);
    rows.unshift({
      level: ev.level,
      style: EVENT_STYLE[ev.kind] || EVENT_STYLE.BUY_BUBBLE,
      time: ms ? fmtClock(ms, true) : "—",
      src: (ev.exchange || "binance").toUpperCase(),
    });
    if (rows.length > cfg.maxRows) rows.pop();
    renderRows();
  }

  function clear() {
    events.length = 0;
    rows.length = 0;
    primitive.updateData([]);
    renderRows();
    App.chart.repaint();
  }

  App.orderflow = { add, clear };
})((window.App = window.App || {}));
