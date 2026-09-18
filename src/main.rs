mod config; mod exec; mod feed; mod levels; mod risk; mod sessions; mod signal; mod state; mod types; mod vp; mod web;

use std::sync::Arc;
use chrono::Utc;
use state::AppState;
use types::{Event, Ray};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cfg = config::load("config.toml");
    let (st, _tx) = AppState::new(cfg.clone());
    *st.equity.write().unwrap() = cfg.risk.paper_equity;

    let feed = feed::Feed::from_cfg(&cfg.feed);
    for tf in ["M15", "H1", "D"] {
        match feed.candles(tf, 500).await {
            Ok(cs) => { tracing::info!("{tf}: {} candles", cs.len()); st.push(tf, cs); }
            Err(e) => tracing::error!("{tf} bootstrap failed: {e}"),
        }
    }
    recompute(&st);
    let loop_st = st.clone();
    let loop_feed = match cfg.feed.kind.as_str() { "csv" => None, _ => Some(cfg.feed.clone()) };
    tokio::spawn(poll(loop_st, feed, loop_feed.is_some()));
    let app = web::router(st);
    tracing::info!("dashboard on http://{}", cfg.server.bind);
    axum::serve(tokio::net::TcpListener::bind(&cfg.server.bind).await.unwrap(), app).await.unwrap();
}

fn recompute(st: &Arc<AppState>) {
    let ck = sessions::Clock::new(st.cfg.tz.offset_hours, &st.cfg.tz.day_roll, &st.cfg.tz.week_open);
    let lc = levels::Cfg { row: st.cfg.vp.row, cw_from_weekday: st.cfg.vp.cw_from_weekday };
    let h1 = st.candles.read().unwrap().get("H1").cloned().unwrap_or_default();
    let rays: Vec<Ray> = levels::rays(Utc::now(), &h1, &ck, &lc)
        .into_iter().map(|r| Ray { label: r.label.to_string(), price: r.price }).collect();
    *st.levels.write().unwrap() = rays.clone();
    let _ = st.tx.send(Event::Levels { rays });
}

async fn poll(st: Arc<AppState>, feed: feed::Feed, live: bool) {
    let mut eng = signal::Engine::new();
    let mut paper = exec::Paper::new(&st.cfg.risk);
    let mut last_roll = chrono::DateTime::UNIX_EPOCH;
    let mut last_m15 = 0i64;
    let mut iv = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        iv.tick().await;
        let now = Utc::now();
        let ck = sessions::Clock::new(st.cfg.tz.offset_hours, &st.cfg.tz.day_roll, &st.cfg.tz.week_open);
        if live {
            for tf in ["M15", "H1", "D"] {
                if let Ok(cs) = feed.candles(tf, if tf == "M15" { 3 } else { 2 }).await {
                    let closed: Vec<_> = cs.into_iter().collect();
                    if let Some(c) = closed.last() {
                        let _ = st.tx.send(Event::Tick { tf: tf.to_string(), candle: *c });
                    }
                    // merge closed candles into cache
                    let mut cur = st.candles.read().unwrap().get(tf).cloned().unwrap_or_default();
                    for c in &closed {
                        if let Some(l) = cur.last_mut() { if l.t == c.t { *l = *c; continue; } }
                        if cur.last().map_or(false, |l: &types::Candle| l.t < c.t) { cur.push(*c); }
                    }
                    st.push(tf, cur);
                }
            }
        }
        let roll = ck.roll(now);
        if roll != last_roll { last_roll = roll; recompute(&st); }
        // signal engine on newly closed M15
        let m15 = st.candles.read().unwrap().get("M15").cloned().unwrap_or_default();
        let closed: Vec<_> = m15.iter().copied().filter(|c| c.t > last_m15).collect();
        for c in closed {
            last_m15 = c.t;
            let levels = st.levels.read().unwrap().clone();
            let h1 = st.candles.read().unwrap().get("H1").cloned().unwrap_or_default();
            let a = risk::atr(&h1, st.cfg.signal.atr_period);
            let eq = *st.equity.read().unwrap();
            for s in eng.on_candle(c, &levels, &st.cfg.signal, &st.cfg.risk, a, eq) {
                tracing::info!("SIGNAL {} {} @ {:.3}", s.level, if s.side == types::Side::Long {"LONG"} else {"SHORT"}, s.entry);
                let p = paper.submit(&s);
                st.signals.write().unwrap().push(s.clone());
                let _ = st.tx.send(Event::Signal { signal: s.clone() });
                let _ = st.tx.send(Event::Position { position: p });
                if st.cfg.exec.alert_telegram {
                    let cfg = st.cfg.clone(); let txt = exec::alert_text(&s);
                    tokio::spawn(async move { exec::telegram(&cfg, &txt).await });
                }
            }
        }
        // mark paper positions
        if let Some(px) = st.last("M15").map(|c| c.c) {
            for p in paper.on_price(now.timestamp(), px, px) {
                *st.equity.write().unwrap() = paper.equity;
                st.positions.write().unwrap().push(p.clone());
                let _ = st.tx.send(Event::Position { position: p });
                let _ = st.tx.send(Event::Equity { equity: paper.equity });
            }
        }
    }
                                   }
