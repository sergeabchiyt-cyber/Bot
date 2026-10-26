//! Node 1 — XAUUSD market data + calendar feed (C1 v2).
//!
//! Read-only by construction: the process pulls candles and ticks from one
//! upstream, folds them into three aligned resolutions, and publishes them
//! over five `GET` routes and one WebSocket. It computes no level, chooses no
//! side, sizes nothing and holds no credential beyond the market feed's.

mod calendar;
mod candles;
mod config;
mod mcp_client;
mod publish;
mod session;
mod sifting_rest;
mod sifting_ws;
mod status;
mod synthetic;
mod tick_volume;
mod types;
mod ws_server;

use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use candles::CandleBook;
use config::Config;
use publish::MarketPublisher;
use status::{BootInfo, FeedStatus, FreshnessTracker};
use tick_volume::TickVolumeStore;
use types::{BROADCAST_CAPACITY, CONTRACT_VERSION, Interval, Sequencer, TICK_VOLUME_CAPACITY};
use ws_server::AppState;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Seed every resolution from SiftingIO history and close the hole between the
/// seed's edge and the live stream.
///
/// SiftingIO's REST history can lag its own live stream by hours, and the live
/// stream only ever appends from "now" — without the tail fetch a restart would
/// boot with a hole in the chart that nothing ever fills.
async fn seed_history(config: &Config, book: &CandleBook) {
    let mut seeded = false;

    if config.sifting_api_key.is_empty() {
        warn!("No SIFTING_API_KEY set — historical candles are unavailable");
    } else {
        for interval in Interval::ALL {
            match sifting_rest::fetch_seed(
                &config.sifting_hist_url,
                &config.sifting_api_key,
                &config.sifting_symbol,
                interval,
            )
            .await
            {
                Ok(bars) => {
                    let report = book.seed(interval, &bars);
                    info!(
                        "Seeded {} {} bars from SiftingIO ({} rejected, {} merged)",
                        report.bars,
                        report.interval.as_str(),
                        report.rejected(),
                        report.merged
                    );
                    if report.bars > 0 {
                        seeded = true;
                    }
                    fill_tail(config, book, interval).await;
                }
                Err(error) => warn!(
                    "SiftingIO {} history unavailable: {error:#}",
                    interval.as_str()
                ),
            }

            if interval != Interval::M1
                && book.bars(interval) == 0
                && book.bars(Interval::M1) > 0
            {
                let minutes = book.closed(Interval::M1);
                let folded = candles::fold_closed_from_minutes(&minutes, interval, now_ms());
                if !folded.is_empty() {
                    let report = book.seed(interval, &folded);
                    info!(
                        "Folded {} {} bars from seeded 1m history ({} rejected)",
                        report.bars,
                        interval.as_str(),
                        report.rejected()
                    );
                    if report.bars > 0 {
                        seeded = true;
                    }
                }
            }
        }
    }

    // CI / air-gapped fallback: no upstream history on hosted runners, so a
    // deterministic series can be seeded when explicitly requested.
    if !seeded && config.seed_synthetic_candles {
        for (interval, bars) in synthetic::history(now_ms()) {
            let report = book.seed(interval, &bars);
            warn!(
                "No upstream history — seeded {} SYNTHETIC {} bars (SEED_SYNTHETIC_CANDLES=1)",
                report.bars,
                report.interval.as_str()
            );
            if report.bars > 0 {
                seeded = true;
            }
        }
    }

    if !seeded {
        warn!("Empty candle cache: /candles stays empty until a live bar closes");
    }
}

async fn fill_tail(config: &Config, book: &CandleBook, interval: Interval) {
    let Some(edge) = book.edge(interval) else {
        return;
    };
    let lag_ms = now_ms() - edge;
    let threshold_ms = (2 * interval.millis()).max(30 * 60_000);
    if lag_ms < threshold_ms {
        return;
    }

    match sifting_rest::fetch_since(
        &config.sifting_hist_url,
        &config.sifting_api_key,
        &config.sifting_symbol,
        interval,
        edge,
    )
    .await
    {
        Ok(tail) if !tail.is_empty() => {
            let report = book.seed(interval, &tail);
            info!(
                "SiftingIO {} history tail: {} bar(s) fetched, cache now {} bars",
                interval.as_str(),
                report.accepted,
                report.bars
            );
        }
        Ok(_) => warn!(
            "SiftingIO {} history ends {} minutes behind the live stream; /status reports the gap",
            interval.as_str(),
            lag_ms / 60_000
        ),
        Err(error) => warn!(
            "SiftingIO {} history tail fetch failed: {error}",
            interval.as_str()
        ),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    info!(
        "Starting XAUUSD engine v{} (C1 v{}) on port {}",
        env!("CARGO_PKG_VERSION"),
        CONTRACT_VERSION,
        config.port
    );

    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    let status = FeedStatus::new();
    let freshness = FreshnessTracker::new("sifting");
    let book = CandleBook::new();
    let tick_volume = TickVolumeStore::new(TICK_VOLUME_CAPACITY);
    let calendar = Arc::new(RwLock::new(serde_json::json!({
        "source": "pending",
        "count": 0,
        "updated": 0,
        "events": [],
    })));
    let boot = Arc::new(BootInfo::new(now_ms()));
    let publisher = MarketPublisher::new(
        tx.clone(),
        Sequencer::new(),
        tick_volume.clone(),
        freshness.clone(),
    );

    // ---------- Cold-start history ----------
    seed_history(&config, &book).await;

    // ---------- Live market stream ----------
    if config.feed_sifting {
        let book = book.clone();
        let publisher = publisher.clone();
        let feed_status = status.clone();
        let url = config.sifting_ws_url.clone();
        let key = config.sifting_api_key.clone();
        let symbol = config.sifting_symbol.clone();
        tokio::spawn(async move {
            if let Err(error) =
                sifting_ws::run_stream(url, key, symbol, book, publisher, feed_status).await
            {
                tracing::error!("Market stream terminated: {error}");
            }
        });
    } else {
        status.set("sifting", "off (no key)");
        warn!("FEED_SIFTING is off — no live ticks, so /candles will not advance");
    }

    // ---------- Economic calendar ----------
    // Direct ForexFactory weekly feed first, SiftingIO economic calendar API
    // second when keyed, and an external MCP browser third only if configured.
    {
        let source = Arc::new(calendar::CalendarSource::with_sifting(
            config.calendar_url.clone(),
            Some(config.sifting_hist_url.clone()),
            Some(config.sifting_api_key.clone()),
        ));
        let mcp = if config.calendar_use_mcp {
            Some(Arc::new(mcp_client::McpClient::new(
                config.mcp_browser_url.clone(),
                config.mcp_browser_token.clone(),
            )))
        } else {
            None
        };
        let publisher = publisher.clone();
        let feed_status = status.clone();
        let cache = calendar.clone();
        let interval_secs = config.mcp_scrape_secs;
        feed_status.set("calendar", "starting");
        tokio::spawn(async move {
            let mut populated = false;
            loop {
                match source.fetch(mcp.as_deref(), &feed_status).await {
                    Ok((events, origin)) => {
                        populated = true;
                        let payload = serde_json::json!({
                            "source": origin,
                            "count": events.len(),
                            "updated": now_ms(),
                            "events": events,
                        });
                        *cache.write().await = payload.clone();
                        publisher.publish_calendar(payload);
                    }
                    Err(error) => warn!("Calendar update failed: {error:#}"),
                }
                let wait_secs = if populated {
                    interval_secs
                } else {
                    interval_secs.min(60)
                };
                tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
            }
        });
    }

    // ---------- HTTP + WS server ----------
    let state = AppState {
        publisher,
        candles: book,
        tick_volume,
        calendar,
        status,
        freshness,
        boot,
        config: config.clone(),
    };
    ws_server::spawn_status_poller(state.clone());

    let app = ws_server::router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    info!(
        "Listening on 0.0.0.0:{} — /health /status /candles /tick-volume /calendar /ws",
        config.port
    );
    axum::serve(listener, app).await?;

    Ok(())
}
