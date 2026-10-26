mod config;
mod diagnostics;
mod drawings;
mod feed;
mod health;
mod history;
mod intent;
mod market;
mod ny;
mod risk;
mod store;
mod strategy;
mod types;
mod vp;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time::{interval, sleep};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::Config;
use diagnostics::DiagnosticsHub;
use feed::{FeedEvent, FeedValidator};
use risk::RiskProjector;
use strategy::{ScanContext, StrategyEngine, Trigger};
use types::ScanningSnapshot;

/// First pause before reconnecting to Node 1; doubles on each consecutive failure.
const RECONNECT_BASE: Duration = Duration::from_secs(2);
/// Upper bound on the reconnect pause.
const RECONNECT_MAX: Duration = Duration::from_secs(60);
/// A session that stayed up at least this long resets the backoff.
const RECONNECT_STABLE_AFTER: Duration = Duration::from_secs(60);
/// How often the pipeline refreshes profiles, silence state, and scanning.
const REFRESH_EVERY: Duration = Duration::from_secs(5);

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Strategy and risk state. It lives across Node 1 reconnects so break and
/// retest state is not reset by a dropped socket.
struct Pipeline {
    config: Config,
    engine: StrategyEngine,
    risk: RiskProjector,
    /// Newest closed strategy-interval bar that was evaluated. Guards against
    /// evaluating the same bar twice after a reconnect.
    last_evaluated: Option<i64>,
}

impl Pipeline {
    fn new(config: Config) -> Self {
        let risk = RiskProjector::new(&config);
        Self {
            config,
            engine: StrategyEngine::default(),
            risk,
            last_evaluated: None,
        }
    }

    async fn sync_levels(&mut self, hub: &DiagnosticsHub) {
        self.engine.sync_levels(&hub.market_levels().await);
    }

    async fn scanning(&self, hub: &DiagnosticsHub) -> ScanningSnapshot {
        let (tradeable, freshness_state) = hub.trading_state().await;
        self.engine.scanning(
            &self.risk,
            &ScanContext {
                tradeable,
                freshness_state: &freshness_state,
                strategy_interval: &self.config.strategy_interval,
                volume_threshold: self.config.volume_threshold,
                timestamp: now_ms(),
            },
        )
    }

    async fn handle(&mut self, hub: &DiagnosticsHub, event: FeedEvent) {
        match event {
            FeedEvent::Candle {
                interval: bar_interval,
                candle,
                closed,
                is_new,
                replay,
                freshness,
            } => {
                hub.record_freshness(&freshness).await;
                hub.record_bar(bar_interval, &candle, closed, is_new, replay).await;
                if !closed {
                    return;
                }
                if hub.recompute_market(now_ms()).await {
                    self.sync_levels(hub).await;
                }
                if bar_interval != self.config.strategy_interval {
                    return;
                }

                // Only a new, live, open-market bar may trigger. Replays and
                // stale or closed-market bars only feed ATR history.
                let fresh_enough = self
                    .last_evaluated
                    .is_none_or(|last| candle.time > last);
                let evaluate = !replay && is_new && freshness.tradeable() && fresh_enough;
                if !evaluate {
                    self.engine.seed(&candle);
                    self.risk.update_atr(self.engine.atr());
                    return;
                }

                self.last_evaluated = Some(candle.time);
                let report = self
                    .engine
                    .evaluate(&candle, self.config.volume_threshold);
                self.risk.update_atr(self.engine.atr());
                let scanning = self.scanning(hub).await;
                let triggered: Option<Trigger> = report.triggered.clone();
                hub.record_evaluation(scanning, &report).await;
                if let Some(trigger) = triggered {
                    self.emit(hub, &trigger).await;
                }
            }
            FeedEvent::TickVolume {
                ticks,
                last_tick_ms,
                freshness,
            } => {
                hub.record_freshness(&freshness).await;
                hub.record_tick_volume(ticks, last_tick_ms).await;
            }
            FeedEvent::Calendar { items, freshness } => {
                hub.record_freshness(&freshness).await;
                hub.record_calendar(items).await;
            }
            FeedEvent::Status { freshness } => hub.record_freshness(&freshness).await,
            FeedEvent::Heartbeat | FeedEvent::Ignored => {}
        }
    }

    /// Periodic work: silence detection, profile refresh, and scanning state.
    async fn refresh(&mut self, hub: &DiagnosticsHub) {
        let now = now_ms();
        hub.check_silence(now).await;
        if hub.recompute_market(now).await {
            self.sync_levels(hub).await;
        }
        let scanning = self.scanning(hub).await;
        hub.record_scanning(scanning).await;
    }

    /// Turn a confirmed retest into an intent. Node 3 never sizes or places the order.
    async fn emit(&self, hub: &DiagnosticsHub, trigger: &Trigger) {
        let Some(projection) = self.risk.projection() else {
            warn!("Retest confirmed on {} but ATR is not ready; no intent", trigger.level_name);
            return;
        };
        let Some(order) = self.risk.order(trigger.reference_price, trigger.side) else {
            warn!(
                "Retest confirmed on {} but risk bands cannot be met; no intent",
                trigger.level_name
            );
            return;
        };
        let intent = intent::build_intent(&intent::IntentInputs {
            trigger,
            order: &order,
            projection,
            volume_gate: self.config.volume_filter_enabled(),
            ttl_secs: self.config.signal_ttl_secs,
            now_ms: now_ms(),
        });
        info!(
            "Strategy confirmed {} at {:.2} on {}; emitting intent {} for Node 4",
            trigger.side, trigger.reference_price, trigger.level_name, intent.intent_id
        );
        if !hub.record_intent(intent).await {
            warn!("Intent was not emitted (duplicate or pending queue full)");
        }
    }
}

/// Load closed history over REST, record it, and rebuild the levels from it.
///
/// Every resolution is fetched and validated before any bar is recorded, so a
/// failure leaves the store as it was. The strategy interval also seeds ATR.
async fn load_history(
    config: &Config,
    hub: &DiagnosticsHub,
    pipeline: &mut Pipeline,
) -> Result<(), String> {
    let base = history::rest_base(&config.node1_ws_url)?;
    let loaded = history::fetch_all(&base).await?;
    for (interval, bars) in &loaded {
        for bar in bars {
            hub.record_bar(interval, bar, true, true, true).await;
            if *interval == config.strategy_interval {
                pipeline.engine.seed(bar);
            }
        }
        info!("Loaded {} closed {interval} bars from Node 1 history", bars.len());
    }
    hub.recompute_market(now_ms()).await;
    pipeline.sync_levels(hub).await;
    pipeline.risk.update_atr(pipeline.engine.atr());
    Ok(())
}

/// One Node 1 connection. Returns the reason it ended.
async fn feed_session(
    config: &Config,
    hub: &DiagnosticsHub,
    pipeline: &mut Pipeline,
    reconnect: bool,
) -> String {
    info!("Connecting to Node 1 WS at {}", config.node1_ws_url);
    let (mut ws, _) = match connect_async(&config.node1_ws_url).await {
        Ok(connection) => connection,
        Err(error) => return format!("connection failed: {error}"),
    };
    hub.set_node1_state(true, "connected", reconnect).await;

    // Node 1 contract v2. The subscription names only the topics Node 3 uses.
    // It never subscribes to Node 1 `levels`, which Node 3 does not trust.
    let subscription = serde_json::json!({
        "type": "subscribe",
        "contract_version": feed::CONTRACT_VERSION,
        "topics": ["candle", "tick_volume", "calendar", "status"],
    });
    if let Err(error) = ws.send(Message::Text(subscription.to_string().into())).await {
        return format!("subscribe failed: {error}");
    }

    // Subscribed. Live frames queue in the socket while history loads, so none are lost.
    // The subscription also replays recent bars, which the store dedupes by open time.
    if let Err(reason) = load_history(config, hub, pipeline).await {
        error!("Node 1 history load failed: {reason}");
        return format!("history load failed: {reason}");
    }

    // Split so the read and write halves never share one mutable borrow.
    let (mut sink, mut stream) = ws.split();
    let mut validator = FeedValidator::default();
    let mut tick = interval(REFRESH_EVERY);
    tick.tick().await;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                pipeline.refresh(hub).await;
            }
            message = stream.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        hub.record_ws_message().await;
                        match validator.accept(&text) {
                            Ok(FeedEvent::Heartbeat) => {
                                let heartbeat = Message::Text(r#"{"type":"heartbeat"}"#.into());
                                let _ = sink.send(heartbeat).await;
                            }
                            Ok(event) => pipeline.handle(hub, event).await,
                            Err(reason) => {
                                // Fail closed: drop the connection and never trade on this frame.
                                error!("Node 1 contract violation: {reason}");
                                hub.record_violation(&reason).await;
                                return format!("contract violation: {reason}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        hub.record_ws_message().await;
                        let _ = sink.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => return "closed by Node 1".into(),
                    Some(Err(error)) => return format!("stream error: {error}"),
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}

/// Reconnect pause after `failures` consecutive failed sessions: 2s, 4s, ... capped at 60s.
fn reconnect_delay(failures: u32) -> Duration {
    let factor = 1u32 << failures.min(6);
    (RECONNECT_BASE * factor).min(RECONNECT_MAX)
}

async fn feed_loop(config: Config, hub: DiagnosticsHub) {
    let mut pipeline = Pipeline::new(config.clone());
    let mut attempts: u64 = 0;
    let mut failures: u32 = 0;
    loop {
        let started = std::time::Instant::now();
        let reason = feed_session(&config, &hub, &mut pipeline, attempts > 0).await;
        if started.elapsed() >= RECONNECT_STABLE_AFTER {
            failures = 0;
        }
        let delay = reconnect_delay(failures);
        warn!("Node 1 feed ended: {reason}; reconnecting in {}s", delay.as_secs());
        // A violation keeps its own record; this only marks the stream down.
        hub.set_node1_state(false, &reason, false).await;
        attempts += 1;
        failures = failures.saturating_add(1);
        sleep(delay).await;
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::*;

    #[test]
    fn backoff_doubles_from_two_seconds_and_caps_at_sixty() {
        let secs: Vec<u64> = (0..8).map(|n| reconnect_delay(n).as_secs()).collect();
        assert_eq!(secs, vec![2, 4, 8, 16, 32, 60, 60, 60]);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Node 1 is reached over wss://. Select the ring provider explicitly; an error here only
    // means a provider was already installed, which is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env();
    for warning in &config.warnings {
        warn!("config: {warning}");
    }
    info!("Starting Node 3 (strategy-only service, v{})", env!("CARGO_PKG_VERSION"));
    info!("Node 1 market stream: {}", config.node1_ws_url);
    info!("HTTP/public WS/private execution port: {}", config.port);
    info!(
        "Week anchor {}; PS mode {}; CW first publish {}; VA {:.0}%; {} rows",
        config.week_anchor,
        config.ps_mode.as_str(),
        config.cw_first_publish,
        config.va_pct * 100.0,
        config.vp_rows
    );
    if config.volume_filter_enabled() {
        info!("Volume gate enabled at {:.0}", config.volume_threshold);
    } else {
        info!("Volume gate disabled (VOLUME_THRESHOLD=0)");
    }
    if !config.node4_link_configured() {
        warn!(
            "NODE4_SHARED_TOKEN is not configured: the strategy will run and queue intents, but Node 4 cannot connect"
        );
    }

    let hub = DiagnosticsHub::new(&config);
    tokio::spawn(health::serve(config.clone(), hub.clone()));
    feed_loop(config, hub).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_starts_without_levels_or_atr() {
        let pipeline = Pipeline::new(Config::default());
        assert!(pipeline.engine.atr().is_none());
        assert!(pipeline.risk.projection().is_none());
        assert!(pipeline.last_evaluated.is_none());
    }
}
