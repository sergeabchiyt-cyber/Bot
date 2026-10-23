mod ai_cache;
mod calendar;
mod config;
mod demo_engine;
mod econ_monitor;
mod mcp_client;
mod sifting_rest;
mod sifting_ws;
mod status;
mod strategy;
mod tick_volume;
mod types;
mod volume_profile;
mod ws_server;

use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use ai_cache::AiCache;
use config::Config;
use demo_engine::{DemoEngine, DemoParams};
use status::FeedStatus;
use strategy::{StrategyEngine, StrategyInput, StrategyParams};
use tick_volume::TickVolumeStore;
use types::{VpCandle, WsFrame};
use volume_profile::VolumeProfileEngine;
use ws_server::AppState;

/// Deterministic 15m candle history covering the same fixed 2,000-bar span,
/// used only when `SEED_SYNTHETIC_CANDLES=1` and SiftingIO history failed.
/// Each session gets its own price band so the PS window is clearly distinct.
fn synthetic_history(now_ms: i64) -> Vec<VpCandle> {
    const FIFTEEN_MIN: i64 = 15 * 60 * 1000;
    let bars = sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT as i64;
    // The chart draws epoch-aligned 15m buckets, so the stand-in history is
    // aligned the same way — `ci/verify_candles.py` asserts the grid, and an
    // unaligned seed would let a real alignment regression slip through it.
    let end = now_ms - now_ms.rem_euclid(FIFTEEN_MIN);
    let start = end - bars * FIFTEEN_MIN;
    let mut out = Vec::with_capacity(bars as usize);
    for i in 0..bars {
        let t = start + i * FIFTEEN_MIN;
        // A slow drift plus an intraday wave -> realistic-looking profile.
        let day = (i / 96) as f64;
        let phase = (i % 96) as f64 / 96.0 * std::f64::consts::TAU;
        let base = 3300.0 + day * 7.5 + phase.sin() * 12.0;
        let low = base - 1.5;
        let high = base + 1.5;
        out.push(VpCandle {
            time: t,
            open: base,
            high,
            low,
            close: base + 0.25,
            volume: 100.0 + ((i % 17) as f64) * 3.0,
            source: "synthetic".into(),
        });
    }
    out
}

fn same_level_values(a: &types::VpLevels, b: &types::VpLevels) -> bool {
    a.window == b.window
        && a.poc == b.poc
        && a.vah == b.vah
        && a.val == b.val
        && a.start == b.start
        && a.end == b.end
        && a.direction == b.direction
        && a.swing_high == b.swing_high
        && a.swing_low == b.swing_low
        && a.sunday_open == b.sunday_open
        && a.meta == b.meta
}

fn level_changed(previous: &[types::VpLevels], next: &types::VpLevels) -> bool {
    previous
        .iter()
        .find(|old| old.window == next.window)
        .is_none_or(|old| !same_level_values(old, next))
}

/// Nearest high-impact USD event in the cached calendar snapshot, in epoch ms.
/// Feeds the strategy's news blackout so the engine stands aside either side of
/// a print instead of paying the spread into it.
fn nearest_high_impact_event(calendar: &serde_json::Value, now_ms: i64) -> Option<i64> {
    let events = calendar.get("events")?.as_array()?;
    events
        .iter()
        .filter(|event| {
            let flagged = event
                .get("gold_relevant")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let high_impact = event
                .get("impact")
                .and_then(|value| value.as_str())
                .is_some_and(|impact| impact.eq_ignore_ascii_case("high"));
            flagged || high_impact
        })
        .filter_map(|event| event.get("timestamp").and_then(|value| value.as_i64()))
        .min_by_key(|ts| (ts - now_ms).abs())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    info!(
        "Starting XAUUSD engine v{} on port {}",
        env!("CARGO_PKG_VERSION"),
        config.port
    );

    let (bc_tx, _) = broadcast::channel(4096);
    // SiftingIO is the sole live price, candle and tick-volume source, so the
    // chart, the profiles and the strategy all read one price series.
    let (sifting_candle_tx, mut sifting_candle_rx) = mpsc::channel::<VpCandle>(256);
    // Fine 1m bars feed PW/PS/CW. The chart and swing detector stay on 15m.
    let (sifting_profile_candle_tx, mut sifting_profile_candle_rx) =
        mpsc::channel::<VpCandle>(1024);

    let status = FeedStatus::new(bc_tx.clone());
    // TradingView-parity histogram model + profile input resolution.
    let vp_model = volume_profile::ProfileModel {
        row_mode: volume_profile::RowMode::parse(&config.vp_row_mode),
        rows: config.vp_rows,
        bin_size: config.vp_bin_size,
        tick_size: config.vp_tick_size,
        va_pct: config.vp_va_pct / 100.0,
    }
    .sanitized();
    let vp_lower_tf = volume_profile::LowerTf::parse(&config.vp_lower_tf);
    info!(
        "Volume profile model: row_mode={} rows={} bin_size={:.2} tick={:.4} value_area={:.0}% input={}",
        vp_model.row_mode.as_str(),
        vp_model.rows,
        vp_model.bin_size,
        vp_model.tick_size,
        vp_model.va_pct * 100.0,
        vp_lower_tf.as_str(),
    );
    let vp = Arc::new(RwLock::new(VolumeProfileEngine::with_settings(
        vp_model,
        vp_lower_tf,
    )));
    let cached_levels = Arc::new(RwLock::new(Vec::new()));
    let cached_candles = Arc::new(RwLock::new(Vec::<VpCandle>::new()));
    // Live tick-volume bars (closed + in-progress) for /tick-volume and WS
    // replay. Same span as the candle cache.
    let tick_volume = TickVolumeStore::new(sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT);
    let cached_calendar = Arc::new(RwLock::new(serde_json::json!({
        "source": "pending",
        "count": 0,
        "events": [],
    })));

    // ---------- Cold-start history ----------
    // The chart/swing seed stays exactly 2,000 x 15m bars. The profile windows
    // are fed the finest history that really covers the previous week: 1m
    // first, then `VP_FALLBACK_INTERVAL` (5m by default — the same resolution
    // TradingView picks for a weekly range), and only then the 15m chart seed.
    // Reconstructing a weekly distribution from 15m OHLC ranges smears each
    // bar's volume over much more price action than an intrabar profile does.
    let mut seeded = false;
    let mut profile_uses_fine_bars = false;
    if !config.sifting_api_key.is_empty() {
        match sifting_rest::fetch_sifting_klines_15m(
            &config.sifting_hist_url,
            &config.sifting_api_key,
            &config.sifting_symbol,
        )
        .await
        {
            Ok(mut candles_15m) => {
                debug_assert_eq!(
                    candles_15m.len(),
                    sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT
                );
                info!(
                    "Seeded exactly {} historical 15M chart/swing candles from SiftingIO",
                    candles_15m.len()
                );

                // SiftingIO's REST history can lag its own live stream, and the
                // live stream only ever appends from "now" — so a restart would
                // otherwise leave a hole between the seed's edge and the first
                // live close that nothing ever fills. Ask for the tail
                // explicitly; if the feed does not have it, say so out loud
                // instead of shipping a silently truncated chart.
                if let Some(edge) = candles_15m.last().map(|c| c.time) {
                    let lag_ms = chrono::Utc::now().timestamp_millis() - edge;
                    if lag_ms > 30 * 60_000 {
                        match sifting_rest::fetch_sifting_klines_15m_since(
                            &config.sifting_hist_url,
                            &config.sifting_api_key,
                            &config.sifting_symbol,
                            edge,
                        )
                        .await
                        {
                            Ok(tail) if !tail.is_empty() => {
                                let newest = tail.last().map(|c| c.time).unwrap_or(edge);
                                let mut by_time: BTreeMap<i64, VpCandle> = candles_15m
                                    .drain(..)
                                    .map(|candle| (candle.time, candle))
                                    .collect();
                                for candle in tail {
                                    by_time.insert(candle.time, candle);
                                }
                                candles_15m = by_time.into_values().collect();
                                info!(
                                    "SiftingIO history tail: filled {} gap bar(s), chart history now ends at {} ({} bars)",
                                    candles_15m.len() - sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT,
                                    newest,
                                    candles_15m.len()
                                );
                            }
                            Ok(_) => warn!(
                                "SiftingIO REST 15m history ends {} minutes behind the live stream; /status reports the gap until new buckets close",
                                lag_ms / 60_000
                            ),
                            Err(e) => {
                                warn!("SiftingIO 15m history tail fetch failed: {e}")
                            }
                        }
                    }
                }

                *cached_candles.write().await = candles_15m.clone();

                let now_ms = chrono::Utc::now().timestamp_millis();
                let (profile_start, profile_end) =
                    VolumeProfileEngine::previous_week_bounds_utc(now_ms);
                let profile_end_exclusive = now_ms - now_ms.rem_euclid(60_000);
                // Allow normal broker maintenance gaps around the NY
                // week-open/close, but reject a cursor/plan-limited page that
                // only covers a fragment of the prior week — that page would
                // silently smear the profile and move the POC.
                const BOUNDARY_TOLERANCE_MS: i64 = 2 * 60 * 60_000;
                let covers_previous_week = |candles: &[VpCandle], interval_ms: i64| {
                    sifting_rest::profile_history_covers(
                        candles,
                        profile_start,
                        profile_end,
                        interval_ms,
                        BOUNDARY_TOLERANCE_MS,
                    )
                };

                let mut intervals: Vec<String> = vec!["1m".to_string()];
                if config.vp_fallback_interval != "1m" {
                    intervals.push(config.vp_fallback_interval.clone());
                }
                let interval_ms =
                    |label: &str| volume_profile::parse_label(label).unwrap_or(60_000);
                let mut chosen: Option<(Vec<VpCandle>, String)> = None;
                for interval in &intervals {
                    match sifting_rest::fetch_sifting_profile_candles(
                        &config.sifting_hist_url,
                        &config.sifting_api_key,
                        &config.sifting_symbol,
                        profile_start,
                        profile_end_exclusive,
                        interval,
                    )
                    .await
                    {
                        Ok(candles) if covers_previous_week(&candles, interval_ms(interval)) => {
                            info!(
                                "Seeded {} historical {} profile candles from SiftingIO (PW window covered)",
                                candles.len(),
                                interval
                            );
                            chosen = Some((candles, interval.clone()));
                            break;
                        }
                        Ok(candles) => warn!(
                            "SiftingIO {} profile history has {} bars and does not cover the complete PW window; trying the next resolution",
                            interval,
                            candles.len()
                        ),
                        Err(e) => warn!(
                            "SiftingIO {} profile history fetch failed: {e}; trying the next resolution",
                            interval
                        ),
                    }
                }

                let (profile_seed, profile_interval) = match chosen {
                    Some((candles, interval)) => {
                        profile_uses_fine_bars = true;
                        (candles, interval)
                    }
                    None => {
                        warn!(
                            "No fine profile history covers the PW window — profiles fall back to the 15M chart seed and will read coarser than TradingView"
                        );
                        (candles_15m.clone(), "15m".to_string())
                    }
                };
                info!(
                    "Profile windows fed from {} bars (VP_LOWER_TF={})",
                    profile_interval, config.vp_lower_tf
                );

                let mut vp_w = vp.write().await;
                vp_w.ingest_history(profile_seed, candles_15m);
                let levels = vp_w.all_levels();
                drop(vp_w);
                *cached_levels.write().await = levels;
                seeded = true;
            }
            Err(e) => {
                warn!(
                    "SiftingIO REST history fetch failed; the chart/profile seed is unavailable: {e}"
                )
            }
        }
    } else {
        warn!("No SIFTING_API_KEY set — historical VP seed is unavailable");
    }
    // CI / air-gapped fallback: upstream history is unavailable on hosted
    // runners, so allow deterministic synthetic candles only when explicitly
    // requested. This is never enabled by default.
    if !seeded && config.seed_synthetic_candles {
        let candles = synthetic_history(chrono::Utc::now().timestamp_millis());
        warn!(
            "No SiftingIO history available — seeding {} SYNTHETIC candles (SEED_SYNTHETIC_CANDLES=1)",
            candles.len()
        );
        *cached_candles.write().await = candles.clone();
        let mut vp_w = vp.write().await;
        vp_w.ingest_candles(candles);
        let levels = vp_w.all_levels();
        drop(vp_w);
        *cached_levels.write().await = levels;
        seeded = true;
    }
    if !seeded {
        warn!("Starting with an empty volume profile.");
    }
    for lvl in cached_levels.read().await.iter() {
        info!(
            "{}: poc={:.3} vah={:.3} val={:.3} direction={}",
            lvl.window, lvl.poc, lvl.vah, lvl.val, lvl.direction
        );
    }

    // ---------- Sifting spot stream (chart + remote source of truth) ----------
    if config.feed_sifting {
        let url = config.sifting_ws_url.clone();
        let key = config.sifting_api_key.clone();
        let symbol = config.sifting_symbol.clone();
        let bc = bc_tx.clone();
        let candle_tx = sifting_candle_tx.clone();
        let profile_candle_tx = profile_uses_fine_bars.then(|| sifting_profile_candle_tx.clone());
        let store = tick_volume.clone();
        let st = status.clone();
        tokio::spawn(async move {
            if let Err(e) = sifting_ws::run_sifting_stream(
                url,
                key,
                symbol,
                bc,
                candle_tx,
                profile_candle_tx,
                store,
                st,
            )
            .await
            {
                tracing::error!("Sifting stream terminated: {e}");
            }
        });
    } else {
        status.set("sifting", "off (no key)");
    }

    // ---------- Sifting 15m candle consumer (chart + swing) ----------
    // SiftingIO broadcasts in-progress chart candles directly from the WS
    // adapter. Closed 15m candles keep the chart and swing detector current;
    // time-window profiles use the separate 1m stream below when available.
    {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let cached_c = cached_candles.clone();
        let bc = bc_tx.clone();
        let profile_uses_fine_bars = profile_uses_fine_bars;
        tokio::spawn(async move {
            while let Some(candle) = sifting_candle_rx.recv().await {
                {
                    let mut buf = cached_c.write().await;
                    buf.retain(|c| c.time != candle.time);
                    buf.push(candle.clone());
                    buf.sort_by_key(|c| c.time);
                    if buf.len() > sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT {
                        let drain = buf.len() - sifting_rest::SIFTING_HISTORY_CANDLE_LIMIT;
                        buf.drain(0..drain);
                    }
                }

                let previous_levels = cached.read().await.clone();
                let mut vp_w = vp.write().await;
                if profile_uses_fine_bars {
                    vp_w.ingest_swing_candle(candle.clone());
                } else {
                    // When minute history is unavailable, keep the legacy
                    // 15m profile running rather than mixing resolutions.
                    vp_w.ingest_candle(candle.clone());
                }
                let levels = vp_w.all_levels();
                drop(vp_w);
                *cached.write().await = levels.clone();
                for lvl in levels {
                    if level_changed(&previous_levels, &lvl) {
                        let _ = bc.send(WsFrame::Levels { data: lvl });
                    }
                }
            }
        });
    }

    // ---------- Fine 1m profile consumer ----------
    // The finer live bars replace the old 15m OHLC-range approximation for
    // PW/PS/CW, while the chart payload remains 15m.
    if profile_uses_fine_bars {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            while let Some(candle) = sifting_profile_candle_rx.recv().await {
                let previous_levels = cached.read().await.clone();
                let mut vp_w = vp.write().await;
                vp_w.ingest_profile_candle(candle);
                let levels = vp_w.all_levels();
                drop(vp_w);
                *cached.write().await = levels.clone();
                for lvl in levels {
                    if level_changed(&previous_levels, &lvl) {
                        let _ = bc.send(WsFrame::Levels { data: lvl });
                    }
                }
            }
        });
    }

    // ---------- Economic calendar ----------
    // Direct ForexFactory weekly feed (no browser needed); the browser MCP
    // server is only used as a fallback when the feed fails.
    {
        let source = Arc::new(calendar::CalendarSource::new(config.calendar_url.clone()));
        let mcp = if config.calendar_use_mcp {
            Some(Arc::new(mcp_client::McpClient::new(
                config.mcp_browser_url.clone(),
                config.mcp_browser_token.clone(),
            )))
        } else {
            None
        };
        let bc = bc_tx.clone();
        let st = status.clone();
        let cal_cache = cached_calendar.clone();
        let interval_secs = config.mcp_scrape_secs.max(60);
        st.set("calendar", "starting");
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                // Ticks immediately on the first pass, so the calendar is
                // populated at boot instead of after the first interval.
                interval.tick().await;
                match source.fetch(mcp.as_deref(), &st).await {
                    Ok((events, origin)) => {
                        let payload = serde_json::json!({
                            "source": origin,
                            "count": events.len(),
                            "updated": chrono::Utc::now().timestamp_millis(),
                            "events": events,
                        });
                        *cal_cache.write().await = payload.clone();
                        let _ = bc.send(WsFrame::Calendar { data: payload });
                    }
                    Err(e) => warn!("Calendar update failed: {e}"),
                }
            }
        });
    }

    // ---------- Econ news audio monitor ----------
    // Watches the calendar for event windows (NFP / CPI / FOMC / ...), finds
    // live coverage (MCP browser + ECON_STREAM_SOURCES watchlist), captures
    // the audio (yt-dlp + ffmpeg) and streams it to Node3 as `audio_chunk`
    // frames on /ws. Node3 transcribes/scores and reports back `transcript` /
    // `sentiment`, served to the desk at GET /ai.
    {
        let cfg = config.clone();
        let bc = bc_tx.clone();
        let cal = cached_calendar.clone();
        let st = status.clone();
        tokio::spawn(async move {
            econ_monitor::run_econ_monitor(cfg, bc, cal, st).await;
        });
    }

    // ---------- Session-close level refresh ----------
    // PS (previous session) must roll forward at every 18:00 New York close,
    // not stay frozen at whatever it was when the process booted. Candles
    // alone cannot be relied on to trigger this (quiet feed, weekend gap), so
    // a timer checks the boundary every 30s and recomputes when it moves.
    {
        let vp = vp.clone();
        let cached = cached_levels.clone();
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                tick.tick().await;
                let now = chrono::Utc::now().timestamp_millis();
                let mut vp_w = vp.write().await;
                if !vp_w.refresh_on_session_close(now) {
                    continue;
                }
                let levels = vp_w.all_levels();
                let session_end = vp_w.ps_session_end();
                drop(vp_w);
                info!(
                    "Session close — levels refreshed (PS session ends {:?})",
                    session_end
                );
                *cached.write().await = levels.clone();
                for lvl in levels {
                    info!(
                        "{}: poc={:.3} vah={:.3} val={:.3}",
                        lvl.window, lvl.poc, lvl.vah, lvl.val
                    );
                    let _ = bc.send(WsFrame::Levels { data: lvl });
                }
            }
        });
    }

    // ---------- Strategy evaluator + demo (paper) execution ----------
    // Order flow is gone: the allocation is decided from the price series, the
    // volume profiles and the tick-volume bars. The demo engine is a local
    // simulator only — no credential, no socket, no order route. Node4 remains
    // the only real execution path.
    let start_price = cached_candles
        .read()
        .await
        .last()
        .map(|candle| candle.close)
        .unwrap_or(0.0);
    let boot_ms = chrono::Utc::now().timestamp_millis();
    let strategy = Arc::new(RwLock::new(StrategyEngine::new(StrategyParams {
        enabled: config.strategy_enabled,
        min_confidence: config.strategy_min_confidence,
        risk_pct: config.strategy_risk_pct,
        atr_period: config.strategy_atr_period,
        atr_stop_mult: config.strategy_atr_stop_mult,
        reward_risk: config.strategy_reward_risk,
        min_lots: config.demo_min_lots,
        max_lots: config.demo_max_lots,
        lot_step: config.demo_lot_step,
        fast_ema: 9,
        slow_ema: 21,
        momentum_bars: 8,
        tick_pressure_bars: 8,
        event_blackout_secs: config.strategy_event_blackout_secs,
        leverage: config.demo_leverage,
        max_margin_pct: 50.0,
    })));
    let demo = Arc::new(RwLock::new(DemoEngine::new(
        DemoParams {
            enabled: config.demo_enabled && config.strategy_enabled,
            start_balance: config.demo_start_balance,
            leverage: config.demo_leverage,
            spread: config.demo_spread,
            min_lots: config.demo_min_lots,
            max_lots: config.demo_max_lots,
            lot_step: config.demo_lot_step,
            stop_out_pct: config.demo_stop_out_pct,
            cooldown_secs: config.demo_cooldown_secs,
        },
        start_price,
        boot_ms,
    )));
    info!(
        "Strategy + demo: XAUUSD 15m, 1:{:.0} leverage, spread {:.2}, {:.2}-{:.2} lots, balance {:.2}, eval every {}s",
        config.demo_leverage,
        config.demo_spread,
        config.demo_min_lots,
        config.demo_max_lots,
        config.demo_start_balance,
        config.strategy_eval_secs
    );

    // Mark the demo account to every live candle frame. Sifting broadcasts the
    // in-progress candle on every accepted tick, so stops, targets and the
    // stop-out fill against the same price series the chart draws.
    {
        let mut rx = bc_tx.subscribe();
        let demo = demo.clone();
        let bc = bc_tx.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(WsFrame::Candle { data }) => {
                        let closed = demo.write().await.set_price(data.close, data.time);
                        if closed.is_some() {
                            let snapshot = demo.read().await.snapshot();
                            let _ = bc.send(WsFrame::Demo { data: snapshot });
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }

    // Evaluate on a timer over *closed* 15m bars, so a decision cannot flicker
    // on every tick and the demo fills against a stable signal.
    if config.strategy_enabled {
        let bc = bc_tx.clone();
        let demo = demo.clone();
        let strategy = strategy.clone();
        let candles = cached_candles.clone();
        let levels = cached_levels.clone();
        let ticks = tick_volume.clone();
        let calendar = cached_calendar.clone();
        let interval_secs = config.strategy_eval_secs.max(1);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                tick.tick().await;
                let now = chrono::Utc::now().timestamp_millis();
                let quote = demo.read().await.quote();
                if quote.mid <= 0.0 {
                    continue;
                }
                let closed_candles = candles.read().await.clone();
                let profile_levels = levels.read().await.clone();
                let tick_bars = ticks.snapshot();
                let calendar_value = calendar.read().await.clone();
                let equity = demo.read().await.equity();

                let input = StrategyInput {
                    now_ms: now,
                    price: quote.mid,
                    candles: &closed_candles,
                    levels: &profile_levels,
                    tick_bars: &tick_bars,
                    equity,
                    next_high_impact_event: nearest_high_impact_event(&calendar_value, now),
                };
                let decision = strategy.write().await.evaluate(&input);

                let report = demo.write().await.apply(&decision, now);
                if report.changed() {
                    info!("demo: {}", report.note);
                }
                if let Ok(value) = serde_json::to_value(&decision) {
                    let _ = bc.send(WsFrame::Strategy { data: value });
                }
                let snapshot = demo.read().await.snapshot();
                let _ = bc.send(WsFrame::Demo { data: snapshot });
            }
        });
    }

    // ---------- HTTP + WS server ----------
    let state = AppState {
        tx: bc_tx.clone(),
        cached_levels: cached_levels.clone(),
        cached_candles: cached_candles.clone(),
        cached_calendar: cached_calendar.clone(),
        tick_volume: tick_volume.clone(),
        vp: vp.clone(),
        status: status.clone(),
        config: config.clone(),
        ai: Arc::new(AiCache::new()),
        strategy: strategy.clone(),
        demo: demo.clone(),
    };
    let app = ws_server::router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    info!(
        "Listening on 0.0.0.0:{} — /health /status /levels /candles /tick-volume /calendar /ai /strategy /demo /ws",
        config.port
    );
    axum::serve(listener, app).await?;

    Ok(())
}
