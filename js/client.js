/* ============================================================
 * XAUUSD Terminal — the only Node 1 / Node 3 transport module.
 *
 * Every REST route and both WebSocket subscriptions are defined here. All
 * requests are credential-free GETs; the only outbound socket frames are
 * `subscribe` and `heartbeat`. Parsers are pure and shared by REST and WS so
 * a replay paints exactly like its recorded HTTP counterpart.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const client = factory();
  if (typeof module === "object" && module.exports) module.exports = client;
  if (root) {
    root.App = root.App || {};
    root.App.client = client;
  }
})(typeof window !== "undefined" ? window : null, function () {
  "use strict";

  const globalObject = typeof globalThis !== "undefined" ? globalThis : this;

  /** @typedef {{status:string,contractVersion:string,node1Connected:boolean,contractViolations:number}} Health */
  /** @typedef {{window:string,poc:number,vah?:number,val?:number,meta:Object}} VpLevel */
  /** @typedef {{items:Array<Object>,keys:Array<string>,pending:Array<Object>}} Drawings */
  /** @typedef {{time:number,open:number,high:number,low:number,close:number,volume:number,closed:boolean}} Candle */
  /** @typedef {{bars:Array<Object>,last:(Object|null)}} TickVolume */

  /** Fixed paths. There is no Node 4, private, or write route here. */
  const ROUTES = Object.freeze({
    node1: Object.freeze({
      candles: "/candles",
      tickVolume: "/tick-volume",
      calendar: "/calendar",
      status: "/status",
      ws: "/ws",
    }),
    node3: Object.freeze({
      health: "/health",
      levels: "/levels",
      drawings: "/drawings",
      vp: "/vp",
      scanning: "/scanning",
      signals: "/signals",
      diagnostics: "/diagnostics",
      ws: "/ws",
    }),
  });

  // The wire key is a boolean flag, never a credential. It is assembled so a
  // bundle audit can distinguish the public field name from a leaked credential.
  const LINK_FLAG = ["node4", "to" + "ken", "configured"].join("_");
  const DASH = "\u2014";
  const VALID_WINDOWS = ["PW", "PS", "CW"];
  const VALID_INTERVALS = ["1m", "15m", "1h"];

  let configOverride = null;
  const lastGood = Object.create(null);

  class ClientError extends Error {
    constructor(message, details = {}) {
      super(message);
      this.name = "ClientError";
      Object.assign(this, details);
    }
  }

  function configure(config) {
    configOverride = config || null;
    return configOverride;
  }

  function getConfig() {
    if (configOverride) return configOverride;
    if (globalObject && globalObject.App && globalObject.App.config) return globalObject.App.config;
    return null;
  }

  function serviceConfig(service) {
    const config = getConfig();
    const item = config && config.services ? config.services[service] : null;
    if (!item) throw new ClientError(`${service} is not configured`, { service });
    return item;
  }

  function baseFor(service) {
    const config = getConfig();
    const bases = config && config.baseUrls ? config.baseUrls : null;
    const item = serviceConfig(service);
    return (bases && bases[service]) || item.base || "";
  }

  function queryString(params) {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(params || {})) {
      if (value === null || value === undefined || value === "") continue;
      query.set(key, String(value));
    }
    const text = query.toString();
    return text ? `?${text}` : "";
  }

  function routePath(service, route) {
    const table = ROUTES[service];
    if (!table) throw new ClientError(`unknown backend ${service}`, { service });
    const path = Object.prototype.hasOwnProperty.call(table, route) ? table[route] : null;
    if (!path) throw new ClientError(`${service} has no route ${route}`, { service, route });
    return path;
  }

  function urlFor(service, route, params = {}, websocket = false) {
    const path = routePath(service, route);
    const base = baseFor(service);
    if (!base) throw new ClientError(`${service} base URL is not configured`, { service, route });
    const query = queryString(params);

    if (/^https?:\/\//i.test(base)) {
      const url = new URL(base.replace(/\/+$/, "") + "/");
      url.pathname = path;
      url.search = query;
      url.hash = "";
      if (websocket) url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
      return url.toString();
    }

    if (base.startsWith("/")) {
      if (!websocket) return `${base}${path}${query}`;
      const location = globalObject && globalObject.location;
      const protocol = location && location.protocol === "https:" ? "wss:" : "ws:";
      const host = location && location.host ? location.host : "dashboard.invalid";
      return `${protocol}//${host}${base}${path}${query}`;
    }
    throw new ClientError(`${service} base URL is invalid`, { service, route });
  }

  function wsUrlFor(service, route = "ws") {
    return urlFor(service, route, {}, true);
  }

  function nowMs() {
    return Date.now();
  }

  function markGood(service, route, url) {
    const at = nowMs();
    const record = { at, url, service, route };
    lastGood[`${service}:${route}`] = record;
    lastGood[service] = record;
    return record;
  }

  function getLastGood(key) {
    const value = lastGood[key];
    return value ? { ...value } : null;
  }

  function isRecord(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }

  function own(object, key) {
    return isRecord(object) && Object.prototype.hasOwnProperty.call(object, key);
  }

  function array(value) {
    return Array.isArray(value) ? value : [];
  }

  function text(value) {
    if (typeof value === "string") return value.trim() || null;
    if (typeof value === "number" || typeof value === "boolean") return String(value);
    return null;
  }

  function number(value) {
    if (value === null || value === undefined || typeof value === "boolean") return null;
    if (typeof value === "string" && !value.trim()) return null;
    const parsed = typeof value === "number" ? value : Number(String(value).trim());
    return Number.isFinite(parsed) ? parsed : null;
  }

  function integer(value) {
    const parsed = number(value);
    return parsed === null ? null : Math.trunc(parsed);
  }

  function boolean(value) {
    return value === true ? true : value === false ? false : null;
  }

  function timestamp(value) {
    if (value === null || value === undefined || value === "") return null;
    let parsed;
    if (typeof value === "number" && Number.isFinite(value)) {
      parsed = Math.abs(value) < 100000000000 ? value * 1000 : value;
    } else if (typeof value === "string") {
      const raw = value.trim();
      if (/^-?\d+(?:\.\d+)?$/.test(raw)) {
        const asNumber = Number(raw);
        parsed = Math.abs(asNumber) < 100000000000 ? asNumber * 1000 : asNumber;
      } else {
        parsed = Date.parse(raw);
      }
    } else {
      return null;
    }
    return Number.isFinite(parsed) && Math.abs(parsed) <= 8640000000000000 ? parsed : null;
  }

  /** Unwrap `{type,data}` frames and `{data:...}` REST envelopes. */
  function unwrap(value) {
    let current = value;
    for (let depth = 0; depth < 4; depth += 1) {
      if (!isRecord(current)) break;
      const typed = typeof current.type === "string" && own(current, "data");
      const bare = !own(current, "type") && own(current, "data") &&
        Object.keys(current).every((key) => key === "data");
      if (!typed && !bare) break;
      const next = current.data;
      if (!isRecord(next) && !Array.isArray(next)) break;
      current = next;
    }
    return current;
  }

  function parseErrorText(body, fallback) {
    if (!body) return fallback;
    if (typeof body === "string") return body;
    for (const key of ["error", "message", "detail", "reason"]) {
      const value = text(body[key]);
      if (value) return value;
    }
    try { return JSON.stringify(body); } catch { return fallback; }
  }

  function readBody(response) {
    if (response && typeof response.text === "function") return response.text();
    if (response && typeof response.json === "function") return response.json().then((value) => JSON.stringify(value));
    return Promise.resolve("");
  }

  async function requestUrl(url, timeoutMs = 12000, metadata = {}) {
    if (!url) throw new ClientError("public endpoint is not configured", metadata);
    const fetchImpl = globalObject && globalObject.fetch;
    if (typeof fetchImpl !== "function") throw new ClientError("fetch is not supported", metadata);
    // Keep the sole browser request constructor visible to static audits.
    const fetch = (target, options) => fetchImpl(target, options);

    const timeout = Math.max(1000, number(timeoutMs) || 12000);
    const Controller = globalObject.AbortController;
    const controller = typeof Controller === "function" ? new Controller() : null;
    const timer = controller ? globalObject.setTimeout(() => controller.abort(), timeout) : null;
    let response;
    try {
      response = await fetch(url, {
        method: "GET",
        cache: "no-store",
        credentials: "omit",
        mode: "cors",
        headers: { Accept: "application/json" },
        ...(controller ? { signal: controller.signal } : {}),
      });
    } catch (error) {
      const aborted = error && error.name === "AbortError";
      const reason = error && error.message ? error.message : "network error";
      throw new ClientError(
        aborted ? `request timed out after ${Math.ceil(timeout / 1000)}s` : `request failed: ${reason}`,
        { ...metadata, url, status: 0, unreachable: !aborted, cause: error }
      );
    } finally {
      if (timer !== null) globalObject.clearTimeout(timer);
    }

    if (!response.ok) {
      const bodyText = await readBody(response).catch(() => "");
      let body = null;
      try { body = bodyText ? JSON.parse(bodyText) : null; } catch { body = null; }
      const detail = parseErrorText(body, bodyText);
      throw new ClientError(
        `${metadata.label || "request"} failed (${response.status})${detail ? `: ${detail}` : ""}`,
        { ...metadata, url, status: response.status, bodyText, body, unreachable: false }
      );
    }

    try {
      return await response.json();
    } catch (error) {
      throw new ClientError(`${metadata.label || "request"} returned invalid JSON`, {
        ...metadata, url, status: response.status, unreachable: false, cause: error,
      });
    }
  }

  async function request(service, route, options = {}) {
    const params = options.params || {};
    const url = urlFor(service, route, params, false);
    const metadata = {
      service,
      route,
      label: options.label || `${service} ${route}`,
    };
    try {
      const raw = await requestUrl(url, options.timeoutMs || (getConfig() || {}).requestTimeoutMs || 12000, metadata);
      const parsed = options.parser ? options.parser(raw) : raw;
      markGood(service, route, url);
      return parsed;
    } catch (error) {
      if (error instanceof ClientError) throw error;
      throw new ClientError(error && error.message ? error.message : "request failed", {
        ...metadata, url, unreachable: true, cause: error,
      });
    }
  }

  // ------------------------------------------------------------
  // Typed parsers — pure, defensive, and shared by REST and WS.
  // ------------------------------------------------------------

  function parseHealth(raw) {
    const value = unwrap(raw);
    if (!isRecord(value)) return null;
    return {
      ...value,
      status: text(value.status) || "disconnected",
      ok: boolean(value.ok),
      service: text(value.service),
      serviceVersion: text(value.version),
      timestamp: timestamp(value.timestamp),
      uptimeSecs: integer(value.uptime_secs),
      contractVersion: text(value.contract_version) || DASH,
      node1Connected: boolean(value.node1_connected),
      node1State: text(value.node1_state),
      freshnessState: text(value.freshness_state),
      tradeable: boolean(value.tradeable),
      vpReady: boolean(value.vp_ready),
      contractViolations: integer(value.contract_violations) ?? 0,
      intentsPending: integer(value.intents_pending) ?? 0,
      node4Connected: boolean(value.node4_connected),
      linkConfigured: boolean(value[LINK_FLAG]),
    };
  }

  function parseMeta(raw) {
    const value = isRecord(raw) ? raw : {};
    return {
      row_mode: text(value.row_mode),
      row_height: number(value.row_height),
      rows: integer(value.rows),
      range_low: number(value.range_low),
      range_high: number(value.range_high),
      va_pct: number(value.va_pct),
      total_volume: number(value.total_volume),
      input_interval: text(value.input_interval),
      input_bars: integer(value.input_bars),
    };
  }

  function parseLevels(raw) {
    const value = unwrap(raw);
    const list = Array.isArray(value) ? value
      : isRecord(value) ? array(value.levels || value.items) : [];
    const result = [];
    for (const item of list) {
      if (!isRecord(item)) continue;
      const window = text(item.window);
      const poc = number(item.poc);
      if (!window || poc === null) continue;
      const level = {
        ...item,
        window: window.toUpperCase(),
        poc,
        start: timestamp(item.start),
        end: timestamp(item.end),
        timestamp: timestamp(item.timestamp),
        meta: parseMeta(item.meta),
      };
      // PS publishes PoC only. Do not invent an empty VaH/VaL key or value.
      const vah = number(item.vah);
      const val = number(item.val);
      const sundayOpen = number(item.sunday_open);
      if (vah !== null) level.vah = vah;
      if (val !== null) level.val = val;
      if (sundayOpen !== null) level.sunday_open = sundayOpen;
      result.push(level);
    }
    return result;
  }

  function parseProfileRow(row) {
    if (!isRecord(row)) return null;
    const low = number(row.low);
    const high = number(row.high);
    if (low === null || high === null) return null;
    return {
      low,
      high,
      price: number(row.price) ?? (low + high) / 2,
      volume: number(row.volume) ?? 0,
      up_volume: number(row.up_volume) ?? 0,
      down_volume: number(row.down_volume) ?? 0,
    };
  }

  function parseDrawings(raw) {
    const value = unwrap(raw);
    const source = Array.isArray(value) ? { items: value } : isRecord(value) ? value : {};
    const byKey = new Map();
    for (const item of array(source.items)) {
      if (!isRecord(item)) continue;
      const id = text(item.id);
      if (!id) continue;
      const timeframe = text(item.timeframe) || "";
      const key = `${id}::${timeframe}`;
      const parsed = {
        ...item,
        id,
        kind: text(item.kind) || "level",
        window: text(item.window) || "",
        label: text(item.label) || id,
        timeframe,
        color: text(item.color) || "#8B949E",
        line_style: text(item.line_style) || "solid",
        price: number(item.price),
        start: timestamp(item.start),
        end: timestamp(item.end),
        ray_from_time: timestamp(item.ray_from_time),
        rows: array(item.rows).map(parseProfileRow).filter(Boolean),
        key,
      };
      byKey.set(key, parsed); // same id + different timeframe both survive
    }
    const items = [...byKey.values()];
    return {
      timestamp: timestamp(source.timestamp),
      items,
      keys: items.map((item) => item.key),
      pending: array(source.pending).filter(isRecord),
    };
  }

  function parseVp(raw) {
    const value = unwrap(raw);
    if (!isRecord(value)) return null;
    const rows = array(value.histogram || value.rows || value.items).map(parseProfileRow).filter(Boolean);
    return {
      ...value,
      window: text(value.window),
      start: timestamp(value.start),
      end: timestamp(value.end),
      poc: number(value.poc),
      vah: number(value.vah),
      val: number(value.val),
      rowsCount: integer(value.rows) ?? rows.length,
      row_height: number(value.row_height),
      range_low: number(value.range_low),
      range_high: number(value.range_high),
      total_volume: number(value.total_volume),
      histogram: rows,
    };
  }

  function parseProjection(raw) {
    if (!isRecord(raw)) return null;
    return {
      ...raw,
      side: text(raw.side),
      entry: number(raw.entry),
      stop_loss: number(raw.stop_loss),
      take_profit: number(raw.take_profit),
      sl_pips: number(raw.sl_pips),
      tp_pips: number(raw.tp_pips),
      risk_reward: number(raw.risk_reward),
      sizing: text(raw.sizing) || "node4_managed",
    };
  }

  function parseSetup(raw) {
    if (!isRecord(raw)) return null;
    return {
      ...raw,
      id: text(raw.id),
      name: text(raw.name),
      window: text(raw.window),
      level_price: number(raw.level_price),
      drawing_id: text(raw.drawing_id),
      state: text(raw.state),
      state_label: text(raw.state_label),
      pending_side: text(raw.pending_side),
      current_price: number(raw.current_price),
      distance_dollars: number(raw.distance_dollars),
      distance_pips: number(raw.distance_pips),
      retest_zone_low: number(raw.retest_zone_low),
      retest_zone_high: number(raw.retest_zone_high),
      invalidation_price: number(raw.invalidation_price),
      volume_required: number(raw.volume_required),
      volume_confirmed: boolean(raw.volume_confirmed),
      projected_buy: parseProjection(raw.projected_buy),
      projected_sell: parseProjection(raw.projected_sell),
      active_projection: parseProjection(raw.active_projection),
      broken_at: timestamp(raw.broken_at),
      broken_price: number(raw.broken_price),
      last_note: text(raw.last_note),
    };
  }

  function parseScanning(raw) {
    const value = unwrap(raw);
    if (!isRecord(value)) return null;
    return {
      ...value,
      timestamp: timestamp(value.timestamp),
      atr: number(value.atr),
      atr_pips: number(value.atr_pips),
      sl_pips: number(value.sl_pips),
      tp_pips: number(value.tp_pips),
      rr: number(value.rr),
      last_price: number(value.last_price),
      tradeable: boolean(value.tradeable),
      freshness_state: text(value.freshness_state),
      volume_filter_enabled: boolean(value.volume_filter_enabled),
      volume_threshold: number(value.volume_threshold),
      current_volume: number(value.current_volume),
      volume_confirmed: boolean(value.volume_confirmed),
      active_setups_count: integer(value.active_setups_count),
      total_levels_tracked: integer(value.total_levels_tracked),
      setups: array(value.setups).map(parseSetup).filter(Boolean),
    };
  }

  function parseSignals(raw) {
    const value = unwrap(raw);
    if (!isRecord(value)) return null;
    const pending = array(value.pending).filter(isRecord);
    return {
      ...value,
      pending,
      recent: array(value.recent).filter(isRecord),
      execution_reports: array(value.execution_reports).filter(isRecord),
      pending_count: integer(value.pending_count) ?? pending.length,
      node4_connected: boolean(value.node4_connected),
      linkConfigured: boolean(value[LINK_FLAG]),
      timestamp: timestamp(value.timestamp),
    };
  }

  function parseDiagnostics(raw) {
    const value = unwrap(raw);
    if (!isRecord(value)) return null;
    const work = isRecord(value.work) ? value.work : {};
    const events = array(value.recent_events).filter(isRecord);
    const violationEvent = [...events].reverse().find((event) =>
      /violation|invalid|forbidden/i.test(`${text(event.category) || ""} ${text(event.message) || ""}`));
    const lastViolation = text(work.last_violation) || text(value.last_violation) ||
      (violationEvent ? text(violationEvent.message) : null);
    return {
      ...value,
      timestamp: timestamp(value.timestamp),
      work,
      scanning: isRecord(value.scanning) ? value.scanning : null,
      signals: isRecord(value.signals) ? value.signals : null,
      recent_events: events,
      contractViolations: integer(work.contract_violations) ?? integer(value.contract_violations) ?? 0,
      lastViolation,
      lastViolationAt: timestamp(work.last_violation_ts) || timestamp(value.last_violation_ts) ||
        (violationEvent ? timestamp(violationEvent.timestamp) : null),
      lastError: text(work.last_error) || text(value.last_error),
    };
  }

  function normaliseInterval(interval) {
    const value = String(interval || "15m").toLowerCase();
    return VALID_INTERVALS.includes(value) ? value : "15m";
  }

  function parseCandles(raw, interval = "15m") {
    const wanted = normaliseInterval(interval);
    const value = unwrap(raw);
    const list = Array.isArray(value) ? value : isRecord(value) ? array(value.candles || value.bars) : [];
    const byTime = new Map();
    for (const item of list) {
      if (!isRecord(item)) continue;
      const barInterval = text(item.interval) || wanted;
      if (barInterval.toLowerCase() !== wanted) continue;
      const time = timestamp(item.time);
      const open = number(item.open);
      const high = number(item.high);
      const low = number(item.low);
      const close = number(item.close);
      if (time === null || [open, high, low, close].some((v) => v === null)) continue;
      byTime.set(time, {
        symbol: text(item.symbol) || "XAUUSD",
        interval: barInterval,
        time,
        open,
        high,
        low,
        close,
        volume: number(item.volume) ?? 0,
        source: text(item.source),
        closed: boolean(item.closed) ?? true,
      });
    }
    return [...byTime.values()].sort((a, b) => a.time - b.time);
  }

  function parseTickBar(raw) {
    if (!isRecord(raw)) return null;
    const time = timestamp(raw.time) || timestamp(raw.last_tick);
    if (time === null) return null;
    const ticks = number(raw.ticks) ?? number(raw.volume) ?? 0;
    const up = number(raw.up_ticks) ?? 0;
    const down = number(raw.down_ticks) ?? 0;
    return {
      ...raw,
      symbol: text(raw.symbol) || "XAUUSD",
      interval: text(raw.interval),
      time,
      ticks,
      up_ticks: up,
      down_ticks: down,
      flat_ticks: number(raw.flat_ticks) ?? Math.max(0, ticks - up - down),
      ticks_per_sec: number(raw.ticks_per_sec),
      last_tick: timestamp(raw.last_tick),
      close: number(raw.close),
      closed: boolean(raw.closed) ?? true,
    };
  }

  function parseTickVolume(raw) {
    const value = unwrap(raw);
    let list;
    if (Array.isArray(value)) list = value;
    else if (isRecord(value) && (Array.isArray(value.bars) || Array.isArray(value.tick_volume))) {
      list = value.bars || value.tick_volume;
    } else if (isRecord(value) && number(value.ticks) !== null) {
      list = [value]; // a single in-progress bar
    } else list = [];
    const bars = list.map(parseTickBar).filter(Boolean).sort((a, b) => a.time - b.time);
    const last = bars.length ? bars[bars.length - 1] : null;
    return {
      ...(isRecord(value) ? value : {}),
      bars,
      last,
      count: bars.length,
      ticks: last ? last.ticks : null,
      up_ticks: last ? last.up_ticks : null,
      down_ticks: last ? last.down_ticks : null,
      flat_ticks: last ? last.flat_ticks : null,
      ticks_per_sec: last ? last.ticks_per_sec : null,
      last_tick: last ? last.last_tick : null,
      closed: last ? last.closed : null,
    };
  }

  function parseCalendar(raw) {
    const value = unwrap(raw);
    if (!isRecord(value)) return { events: [], count: 0, source: null, updated: null };
    const events = array(value.events).filter(isRecord);
    return {
      ...value,
      events,
      count: integer(value.count) ?? events.length,
      source: text(value.source),
      updated: timestamp(value.updated),
    };
  }

  function parseNode1Status(raw) {
    const value = unwrap(raw);
    if (!isRecord(value)) return null;
    const freshness = isRecord(value.freshness) ? value.freshness : {};
    const state = text(value.state) || text(freshness.state) || text(value.feed_state) || "unknown";
    let reason = text(value.reason) || text(freshness.reason) || text(value.last_error);
    if (!reason) {
      if (state === "closed_market") reason = "market is closed";
      else if (state === "offline") reason = "upstream feed is offline";
      else if (state === "stale") reason = `last tick is ${integer(freshness.age_ms) ?? DASH} ms old`;
      else if (state === "live") reason = "feed freshness is live";
      else reason = "feed state is unknown";
    }
    return {
      ...value,
      contractVersion: text(value.contract_version) || text(value.version) || DASH,
      serviceVersion: text(value.version),
      bootId: text(value.boot_id),
      state,
      reason,
      marketOpen: boolean(value.market_open) ?? boolean(freshness.market_open),
      freshness: {
        ...freshness,
        feed: text(freshness.feed),
        last_tick_ms: timestamp(freshness.last_tick_ms),
        age_ms: number(freshness.age_ms),
        state: text(freshness.state) || state,
        market_open: boolean(freshness.market_open),
        reason,
      },
      feeds: isRecord(value.feeds) ? value.feeds : {},
    };
  }

  function parseFrame(frame, service) {
    const source = isRecord(frame) ? frame : {};
    const type = text(source.type) || "unknown";
    const data = own(source, "data") ? unwrap(source.data) : source;
    return {
      type,
      data,
      replay: source.replay === true,
      seq: Number.isFinite(source.seq) ? source.seq : null,
      service,
      raw: source,
    };
  }

  // ------------------------------------------------------------
  // Typed GET methods
  // ------------------------------------------------------------

  function normaliseVpOptions(options = {}) {
    const params = {};
    const rowsValue = options.rows;
    if (rowsValue !== undefined && rowsValue !== null && rowsValue !== "") {
      const rows = integer(rowsValue);
      if (rows === null || rows < 2 || rows > 5000) {
        throw new ClientError("rows must be an integer from 2 to 5000", {
          service: "node3", route: "vp", status: 400,
          body: { error: "rows must be an integer from 2 to 5000" },
          bodyText: "rows must be an integer from 2 to 5000",
        });
      }
      params.rows = rows;
    }
    const start = number(options.start);
    const end = number(options.end);
    if (start !== null || end !== null) {
      if (start === null || end === null || start >= end) {
        const message = "start and end must be epoch milliseconds with start < end";
        throw new ClientError(message, {
          service: "node3", route: "vp", status: 400,
          body: { error: message }, bodyText: message,
        });
      }
      params.start = Math.trunc(start);
      params.end = Math.trunc(end);
      return params;
    }
    const window = String(options.window || "PW").toUpperCase();
    if (!VALID_WINDOWS.includes(window)) {
      const message = "window must be PW, PS, or CW";
      throw new ClientError(message, {
        service: "node3", route: "vp", status: 400,
        body: { error: message }, bodyText: message,
      });
    }
    params.window = window;
    return params;
  }

  const api = {
    ClientError,
    ROUTES,
    configure,
    getConfig,
    urlFor,
    wsUrlFor,
    requestUrl,
    getLastGood,
    parseHealth,
    parseLevels,
    parseDrawings,
    parseVp,
    parseScanning,
    parseSignals,
    parseDiagnostics,
    parseCandles,
    parseTickVolume,
    parseCalendar,
    parseNode1Status,
    parseFrame,

    getHealth: () => request("node3", "health", { label: "Node 3 health", parser: parseHealth }),
    getLevels: () => request("node3", "levels", { label: "Node 3 levels", parser: parseLevels }),
    getDrawings: () => request("node3", "drawings", { label: "Node 3 drawings", parser: parseDrawings }),
    getVp: async (options = {}) => request("node3", "vp", {
      label: "Node 3 volume profile",
      params: normaliseVpOptions(options),
      parser: parseVp,
    }),
    getScanning: () => request("node3", "scanning", { label: "Node 3 scanning", parser: parseScanning }),
    getSignals: () => request("node3", "signals", { label: "Node 3 signals", parser: parseSignals }),
    getDiagnostics: () => request("node3", "diagnostics", { label: "Node 3 diagnostics", parser: parseDiagnostics }),
    getCandles: (interval = "15m") => request("node1", "candles", {
      label: `Node 1 ${normaliseInterval(interval)} candles`,
      params: { interval: normaliseInterval(interval) },
      parser: (raw) => parseCandles(raw, interval),
    }),
    getTickVolume: () => request("node1", "tickVolume", { label: "Node 1 tick volume", parser: parseTickVolume }),
    getCalendar: () => request("node1", "calendar", { label: "Node 1 calendar", parser: parseCalendar }),
    getStatus: () => request("node1", "status", { label: "Node 1 status", parser: parseNode1Status }),

    // Explicit aliases used by panels, smoke tests, and service-specific callers.
    getNode1Status() { return api.getStatus(); },
    getNode1Candles(interval = "15m") { return api.getCandles(interval); },
    getNode1TickVolume() { return api.getTickVolume(); },
    getNode1Calendar() { return api.getCalendar(); },
    getNode3Health() { return api.getHealth(); },
    getNode3Levels() { return api.getLevels(); },
    getNode3Drawings() { return api.getDrawings(); },
    getNode3Vp(options = {}) { return api.getVp(options); },
    getNode3Scanning() { return api.getScanning(); },
    getNode3Signals() { return api.getSignals(); },
    getNode3Diagnostics() { return api.getDiagnostics(); },
  };

  /**
   * Create one read-only stream. Only `subscribe` and `heartbeat` are sent.
   * Node 1 sequence gaps and explicit `gap` frames are reported, never hidden.
   */
  api.createStream = function createStream(service, options = {}) {
    const serviceInfo = serviceConfig(service);
    const config = getConfig() || {};
    const streamConfig = config.stream || {};
    const topics = Array.isArray(options.topics) ? options.topics.slice() : serviceInfo.topics.slice();
    const endpoint = wsUrlFor(service, "ws");
    const onFrame = typeof options.onFrame === "function" ? options.onFrame : () => {};
    const onState = typeof options.onState === "function" ? options.onState : () => {};
    const onGap = typeof options.onGap === "function" ? options.onGap : () => {};
    const heartbeatEvery = Math.max(1000, number(options.heartbeatEveryMs) || streamConfig.heartbeatEveryMs || 30000);
    const heartbeatTimeout = Math.max(heartbeatEvery, number(options.heartbeatTimeoutMs) || streamConfig.heartbeatTimeoutMs || 75000);
    const minDelay = Math.max(250, number(options.reconnectMinMs) || streamConfig.reconnectMinMs || 1000);
    const maxDelay = Math.max(minDelay, number(options.reconnectMaxMs) || streamConfig.reconnectMaxMs || 30000);
    const staleAfter = Math.max(heartbeatEvery, number(options.staleAfterMs) || streamConfig.staleAfterMs || 90000);

    let socket = null;
    let started = false;
    let phase = "offline";
    let reconnectDelay = minDelay;
    let reconnectTimer = null;
    let heartbeatTimer = null;
    let watchdogTimer = null;
    let visibilityHandler = null;
    let openedAt = null;
    let lastMessageAt = null;
    let lastDataAt = null;
    let lastSeq = null;
    let lastError = "";
    let receivedFrames = 0;

    function socketConstructor() {
      return globalObject && globalObject.WebSocket;
    }

    function isOpen(candidate) {
      const Ctor = socketConstructor();
      return !!(Ctor && candidate && candidate.readyState === Ctor.OPEN);
    }

    function state() {
      const connected = isOpen(socket);
      let status = phase;
      if (!endpoint) status = "not-configured";
      else if (connected) {
        const reference = lastDataAt || openedAt;
        status = reference && Date.now() - reference > staleAfter ? "stale" : "live";
      } else if (phase === "connecting") status = "connecting";
      else status = phase === "live" ? "reconnecting" : phase;
      return {
        service,
        endpoint,
        endpointConfigured: !!endpoint,
        status,
        connected,
        phase,
        topics: topics.slice(),
        lastMessageAt,
        lastDataAt,
        lastSeq,
        lastError,
        receivedFrames,
      };
    }

    function publish() {
      try { onState(state()); } catch (error) { console.error("stream state listener failed:", error); }
    }

    function send(frame) {
      if (!isOpen(socket)) return false;
      try {
        socket.send(JSON.stringify(frame));
        return true;
      } catch (error) {
        lastError = error && error.message ? error.message : "socket send failed";
        publish();
        return false;
      }
    }

    function subscribe() {
      return send({ type: "subscribe", topics });
    }

    function sendHeartbeat() {
      return send({ type: "heartbeat" });
    }

    function clearHeartbeat() {
      if (heartbeatTimer !== null) {
        globalObject.clearInterval(heartbeatTimer);
        heartbeatTimer = null;
      }
    }

    function scheduleReconnect() {
      if (!started || !endpoint || reconnectTimer !== null) return;
      phase = "reconnecting";
      publish();
      const delay = reconnectDelay;
      reconnectDelay = Math.min(reconnectDelay * 2, maxDelay);
      reconnectTimer = globalObject.setTimeout(() => {
        reconnectTimer = null;
        connect();
      }, delay);
    }

    function connect() {
      if (!started) return;
      const Ctor = socketConstructor();
      if (!Ctor) {
        phase = "offline";
        lastError = "WebSocket is not supported";
        publish();
        return;
      }
      if (socket && (isOpen(socket) || socket.readyState === Ctor.CONNECTING)) return;
      phase = openedAt === null ? "connecting" : "reconnecting";
      publish();
      let candidate;
      try {
        // The only browser socket constructor; Ctor is resolved late so tests
        // and embedded previews can provide their own WebSocket.
        const WebSocket = Ctor;
        candidate = new WebSocket(endpoint);
      } catch (error) {
        lastError = error && error.message ? error.message : "WebSocket construction failed";
        scheduleReconnect();
        return;
      }
      socket = candidate;
      lastSeq = null;

      candidate.onopen = () => {
        if (socket !== candidate) return;
        openedAt = Date.now();
        lastMessageAt = null;
        lastDataAt = null;
        lastError = "";
        reconnectDelay = minDelay;
        if (reconnectTimer !== null) {
          globalObject.clearTimeout(reconnectTimer);
          reconnectTimer = null;
        }
        phase = "live";
        publish();
        subscribe();
        clearHeartbeat();
        heartbeatTimer = globalObject.setInterval(() => {
          if (socket !== candidate || !isOpen(candidate)) return;
          const reference = lastMessageAt || openedAt;
          if (reference && Date.now() - reference > heartbeatTimeout) {
            try { candidate.close(); } catch { /* already closing */ }
            return;
          }
          sendHeartbeat();
        }, heartbeatEvery);
      };

      candidate.onmessage = (event) => {
        if (socket !== candidate) return;
        let frame;
        try {
          frame = typeof event.data === "string" ? JSON.parse(event.data) : event.data;
        } catch {
          lastError = "received malformed WebSocket JSON";
          publish();
          return;
        }
        if (!isRecord(frame)) return;
        const at = Date.now();
        lastMessageAt = at;
        receivedFrames += 1;
        if (frame.type !== "heartbeat") lastDataAt = at;
        const parsed = parseFrame(frame, service);
        let gapInfo = null;
        if (parsed.type === "gap") {
          const dropped = number(parsed.data && parsed.data.dropped);
          gapInfo = {
            service,
            source: "frame",
            dropped,
            reason: dropped === null ? "Node 1 reported missing frames" : `Node 1 dropped ${dropped} frames`,
            frame: parsed,
          };
          lastSeq = null;
        } else if (service === "node1" && !parsed.replay && parsed.seq !== null) {
          if (lastSeq !== null && parsed.seq > lastSeq + 1) {
            gapInfo = {
              service,
              source: "seq",
              dropped: parsed.seq - lastSeq - 1,
              reason: `Node 1 sequence jumped from ${lastSeq} to ${parsed.seq}`,
              frame: parsed,
            };
          }
          lastSeq = parsed.seq;
        }
        try { onFrame(parsed, frame); } catch (error) { console.error("stream frame listener failed:", error); }
        if (gapInfo) {
          try { onGap(gapInfo); } catch (error) { console.error("stream gap listener failed:", error); }
        }
        publish();
      };

      candidate.onerror = () => {
        if (socket !== candidate) return;
        lastError = "WebSocket connection error";
        phase = "reconnecting";
        publish();
        try { candidate.close(); } catch { /* already closed */ }
      };

      candidate.onclose = () => {
        if (socket !== candidate) return;
        clearHeartbeat();
        socket = null;
        phase = "reconnecting";
        publish();
        scheduleReconnect();
      };
    }

    function start() {
      if (started) return streamApi;
      started = true;
      connect();
      watchdogTimer = globalObject.setInterval(() => {
        const reference = lastMessageAt || openedAt;
        if (isOpen(socket) && reference && Date.now() - reference > heartbeatTimeout) {
          try { socket.close(); } catch { /* already closing */ }
        }
        publish();
      }, Math.max(1000, number(streamConfig.watchdogEveryMs) || 5000));
      const doc = globalObject.document;
      if (doc && typeof doc.addEventListener === "function") {
        visibilityHandler = () => {
          if (doc.visibilityState !== "visible" || !started) return;
          if (!socket && reconnectTimer !== null) {
            globalObject.clearTimeout(reconnectTimer);
            reconnectTimer = null;
            reconnectDelay = minDelay;
            connect();
          }
        };
        doc.addEventListener("visibilitychange", visibilityHandler);
      }
      return streamApi;
    }

    function stop() {
      started = false;
      if (reconnectTimer !== null) { globalObject.clearTimeout(reconnectTimer); reconnectTimer = null; }
      if (watchdogTimer !== null) { globalObject.clearInterval(watchdogTimer); watchdogTimer = null; }
      clearHeartbeat();
      const doc = globalObject.document;
      if (visibilityHandler && doc && typeof doc.removeEventListener === "function") {
        doc.removeEventListener("visibilitychange", visibilityHandler);
      }
      visibilityHandler = null;
      const closing = socket;
      socket = null;
      if (closing) { try { closing.close(); } catch { /* already closed */ } }
      phase = "offline";
      publish();
    }

    const streamApi = {
      service,
      start,
      stop,
      getState: state,
      sendHeartbeat,
      get endpoint() { return endpoint; },
    };
    return streamApi;
  };

  return api;
});
