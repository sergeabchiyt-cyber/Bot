use std::collections::VecDeque;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::status::FeedStatus;
use crate::tick_volume::TickVolumeStore;
use crate::types::{TickVolumeBar, VpCandle, WsFrame};

const PING_INTERVAL_SECS: u64 = 30; // Sifting closes silent connections after 90s
const RECONNECT_SECS: u64 = 5;
const BUCKET_MS: i64 = 15 * 60 * 1000; // 15-minute chart candles
const RATE_WINDOW_MS: i64 = 10_000; // rolling window for `ticks_per_sec`
const SOURCE: &str = "sifting";

/// Tick-rule classification against the previous tick's price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickDir {
    Up,
    Down,
    Flat,
}

impl TickDir {
    fn classify(prev: Option<f64>, price: f64) -> Self {
        match prev {
            Some(p) if price > p => TickDir::Up,
            Some(p) if price < p => TickDir::Down,
            _ => TickDir::Flat,
        }
    }
}

/// One 15m bucket being built from ticks.
///
/// `ticks` is the bucket's volume. SiftingIO's historical bars define `v` as
/// "the number of price updates in the bucket", so live candles count updates
/// too. That keeps the 2,000-bar REST seed and the live edge in one unit, for
/// both the chart's volume pane and the volume profile. (Summing `P`, the
/// last-trade size, used to mix a size-weighted live edge into a tick-count
/// history.)
struct Aggregator {
    start_time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    ticks: u64,
    up_ticks: u64,
    down_ticks: u64,
    last_tick: i64,
}

impl Aggregator {
    fn new(start_time: i64, price: f64, ts: i64, dir: TickDir) -> Self {
        let mut agg = Self {
            start_time,
            open: price,
            high: price,
            low: price,
            close: price,
            ticks: 0,
            up_ticks: 0,
            down_ticks: 0,
            last_tick: ts,
        };
        agg.count(ts, dir);
        agg
    }

    fn update(&mut self, price: f64, ts: i64, dir: TickDir) {
        if price > self.high {
            self.high = price;
        }
        if price < self.low {
            self.low = price;
        }
        self.close = price;
        self.count(ts, dir);
    }

    fn count(&mut self, ts: i64, dir: TickDir) {
        self.ticks += 1;
        match dir {
            TickDir::Up => self.up_ticks += 1,
            TickDir::Down => self.down_ticks += 1,
            TickDir::Flat => {}
        }
        self.last_tick = self.last_tick.max(ts);
    }

    fn candle(&self) -> VpCandle {
        VpCandle {
            time: self.start_time,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.ticks as f64,
            source: SOURCE.into(),
        }
    }

    fn tick_volume(&self, ticks_per_sec: f64, closed: bool) -> TickVolumeBar {
        TickVolumeBar {
            time: self.start_time,
            ticks: self.ticks,
            up_ticks: self.up_ticks,
            down_ticks: self.down_ticks,
            flat_ticks: self.ticks - self.up_ticks - self.down_ticks,
            close: self.close,
            last_tick: self.last_tick,
            ticks_per_sec,
            closed,
            source: SOURCE.into(),
        }
    }
}

/// What one accepted tick produced: the bucket it closed (if any) and the
/// current in-progress bucket.
#[derive(Debug)]
struct TickOutcome {
    closed: Option<(VpCandle, TickVolumeBar)>,
    live: (VpCandle, TickVolumeBar),
}

/// Stream state that has to survive reconnects. Resetting it on every drop
/// would throw away the in-progress bucket's tick count and skip that
/// bucket's close.
struct TickState {
    agg: Option<Aggregator>,
    last_price: Option<f64>,
    /// `(t, p, P)` of the last accepted tick, to drop the cached snapshot
    /// tick Sifting replays on every (re)subscribe.
    last_key: Option<(i64, u64, u64)>,
    /// Stream timestamps of recent ticks for the rolling rate.
    recent: VecDeque<i64>,
    newest_ts: i64,
}

impl TickState {
    fn new() -> Self {
        Self {
            agg: None,
            last_price: None,
            last_key: None,
            recent: VecDeque::new(),
            newest_ts: i64::MIN,
        }
    }

    fn ticks_per_sec(&self) -> f64 {
        let rate = self.recent.len() as f64 / (RATE_WINDOW_MS as f64 / 1000.0);
        (rate * 100.0).round() / 100.0
    }

    /// Fold one tick in. Returns `None` for a tick that must not be counted:
    /// an exact repeat of the previous tick (the re-subscribe snapshot) or a
    /// late tick for a bucket that's already closed. A late tick used to roll
    /// the aggregator *backwards* and emit a truncated "closed" candle.
    fn on_tick(&mut self, price: f64, ts: i64, size: Option<f64>) -> Option<TickOutcome> {
        let key = (ts, price.to_bits(), size.unwrap_or(0.0).to_bits());
        if self.last_key == Some(key) {
            return None;
        }
        let bucket = ts - ts.rem_euclid(BUCKET_MS);
        if let Some(current) = self.agg.as_ref() {
            if bucket < current.start_time {
                debug!(ts, bucket, current = current.start_time, "late Sifting tick dropped");
                return None;
            }
        }
        self.last_key = Some(key);

        let dir = TickDir::classify(self.last_price, price);
        self.last_price = Some(price);

        let mut closed = None;
        let same_bucket = matches!(self.agg.as_ref(), Some(c) if c.start_time == bucket);
        if same_bucket {
            if let Some(current) = self.agg.as_mut() {
                current.update(price, ts, dir);
            }
        } else {
            // Close with the rate as it stood before the new bucket's first tick.
            let rate = self.ticks_per_sec();
            if let Some(previous) = self.agg.take() {
                closed = Some((previous.candle(), previous.tick_volume(rate, true)));
            }
            self.agg = Some(Aggregator::new(bucket, price, ts, dir));
        }

        self.recent.push_back(ts);
        self.newest_ts = self.newest_ts.max(ts);
        let cutoff = self.newest_ts - RATE_WINDOW_MS;
        while let Some(&front) = self.recent.front() {
            if front <= cutoff {
                self.recent.pop_front();
            } else {
                break;
            }
        }

        let tps = self.ticks_per_sec();
        let current = self.agg.as_ref().expect("aggregator was just set");
        Some(TickOutcome {
            closed,
            live: (current.candle(), current.tick_volume(tps, false)),
        })
    }
}

/// SiftingIO spot stream for the chart and the live edge of the VP history.
/// Per their docs:
/// - connect with `?key=`, first frame is `{f:"ack", op:"auth", ...}`
/// - subscribe `{op:"subscribe", product:"com", symbols:["XAUUSD"]}`
/// - ticks are `{f:"tick", class:"com", s, p, P, b, B, a, A, t(ms)}`
/// - the server closes connections idle for 90s → app-level ping every 30s
///
/// Every accepted tick broadcasts the in-progress `candle` and its
/// `tick_volume` bar. When a bucket rolls, the finished candle and a
/// `closed: true` tick-volume bar go out first, and the candle is also sent to
/// `candle_tx` so the backend updates its cached chart and VP (never from
/// Binance price candles). Tick-volume bars are written to `tick_store`
/// before they're broadcast, so a client subscribing mid-bucket replays the
/// live bar.
pub async fn run_sifting_stream(
    base_url: String,
    api_key: String,
    symbol: String,
    bc_tx: broadcast::Sender<WsFrame>,
    candle_tx: mpsc::Sender<VpCandle>,
    tick_store: TickVolumeStore,
    status: FeedStatus,
) -> anyhow::Result<()> {
    let url = if base_url.contains('?') {
        format!("{base_url}&key={api_key}")
    } else {
        format!("{base_url}?key={api_key}")
    };

    // Outlives each connection: a reconnect must not reset the bucket's count.
    let mut state = TickState::new();

    loop {
        status.set("sifting", "connecting");
        info!("Connecting to Sifting WS");
        match connect_async(&url).await {
            Ok((ws_stream, _)) => {
                info!("Sifting WS connected");
                status.set("sifting", "connected");
                let (mut write, mut read) = ws_stream.split();

                let subscribe_msg = serde_json::json!({
                    "op": "subscribe",
                    "product": "com",
                    "symbols": [symbol]
                });
                if write
                    .send(Message::Text(subscribe_msg.to_string().into()))
                    .await
                    .is_err()
                {
                    warn!("Sifting subscribe send failed");
                    status.set("sifting", "error");
                } else {
                    // Keepalive task owns the write half.
                    let ping_task = tokio::spawn(async move {
                        let mut iv = tokio::time::interval(tokio::time::Duration::from_secs(
                            PING_INTERVAL_SECS,
                        ));
                        iv.tick().await; // first tick fires immediately; skip
                        loop {
                            iv.tick().await;
                            let ping = serde_json::json!({ "op": "ping" });
                            if write
                                .send(Message::Text(ping.to_string().into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    });

                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                status.mark_msg("sifting");
                                handle_text(
                                    &text,
                                    &symbol,
                                    &mut state,
                                    &bc_tx,
                                    &candle_tx,
                                    &tick_store,
                                );
                            }
                            Ok(Message::Close(frame)) => {
                                info!("Sifting WS closed by server: {:?}", frame);
                                break;
                            }
                            Ok(_) => {}
                            Err(e) => {
                                warn!("Sifting WS error: {}", e);
                                break;
                            }
                        }
                    }

                    ping_task.abort();
                    status.set("sifting", "disconnected");
                }
            }
            Err(e) => {
                error!("Sifting connect failed: {}", e);
                status.set("sifting", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

fn handle_text(
    text: &str,
    symbol: &str,
    state: &mut TickState,
    bc_tx: &broadcast::Sender<WsFrame>,
    candle_tx: &mpsc::Sender<VpCandle>,
    tick_store: &TickVolumeStore,
) {
    let Some((price, ts, size)) = parse_tick(text, symbol) else {
        return;
    };
    let Some(outcome) = state.on_tick(price, ts, size) else {
        return;
    };

    if let Some((candle, bar)) = outcome.closed {
        tick_store.upsert(bar.clone());
        let _ = candle_tx.try_send(candle.clone());
        let _ = bc_tx.send(WsFrame::Candle { data: candle });
        let _ = bc_tx.send(WsFrame::TickVolume { data: bar });
    }

    let (candle, bar) = outcome.live;
    tick_store.upsert(bar.clone());
    let _ = bc_tx.send(WsFrame::Candle { data: candle });
    let _ = bc_tx.send(WsFrame::TickVolume { data: bar });
}

/// Extract `(price, ts_ms, trade_size)` from a Sifting `tick` frame for
/// `symbol`. Control frames, other symbols, and unusable prices return `None`.
fn parse_tick(text: &str, symbol: &str) -> Option<(f64, i64, Option<f64>)> {
    let val: Value = serde_json::from_str(text).ok()?;

    match val.get("f").and_then(|f| f.as_str()) {
        Some("tick") => {}
        Some("error") => {
            warn!(
                "Sifting server error frame: code={} message={}",
                val.get("code").and_then(|c| c.as_str()).unwrap_or("?"),
                val.get("message").and_then(|m| m.as_str()).unwrap_or("?")
            );
            return None;
        }
        // "ack" / "pong" handshake + keepalive frames, and anything unknown.
        _ => return None,
    }

    if val.get("s").and_then(|s| s.as_str()) != Some(symbol) {
        return None;
    }

    // Prefer the last-trade price `p`, fall back to bid/ask mid.
    let price = val
        .get("p")
        .and_then(|v| v.as_f64())
        .or_else(|| {
            match (
                val.get("b").and_then(|v| v.as_f64()),
                val.get("a").and_then(|v| v.as_f64()),
            ) {
                (Some(b), Some(a)) if b > 0.0 && a > 0.0 => Some((b + a) / 2.0),
                _ => None,
            }
        })
        .filter(|p| p.is_finite() && *p > 0.0)?;
    let ts = val
        .get("t")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    // `P` no longer drives volume; it only helps recognise a repeated tick.
    let size = val
        .get("P")
        .and_then(|v| v.as_f64())
        .filter(|size| size.is_finite());

    Some((price, ts, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    const M15: i64 = BUCKET_MS;
    const T0: i64 = 1_700_000_100_000 - (1_700_000_100_000 % BUCKET_MS);

    fn tick(p: f64, t: i64) -> String {
        serde_json::json!({ "f": "tick", "class": "com", "s": "XAUUSD", "p": p, "P": 1.0, "t": t })
            .to_string()
    }

    #[test]
    fn candle_volume_is_the_tick_count_like_sifting_history() {
        let mut st = TickState::new();
        st.on_tick(100.0, T0, Some(5.0));
        st.on_tick(101.0, T0 + 1, Some(7.0));
        let out = st.on_tick(100.5, T0 + 2, Some(9.0)).unwrap();
        let (candle, bar) = out.live;
        // Three price updates, whatever their trade size.
        assert_eq!(candle.volume, 3.0);
        assert_eq!(bar.ticks, 3);
        assert_eq!(candle.high, 101.0);
        assert_eq!(candle.close, 100.5);
        assert_eq!(bar.close, candle.close);
        assert_eq!(bar.time, candle.time);
    }

    #[test]
    fn tick_rule_splits_up_down_and_flat() {
        let mut st = TickState::new();
        for (i, p) in [100.0, 100.5, 100.5, 100.25, 100.75].iter().enumerate() {
            st.on_tick(*p, T0 + i as i64, None);
        }
        let bar = st.on_tick(100.0, T0 + 10, None).unwrap().live.1;
        // first=flat, +up, =flat, -down, +up, -down
        assert_eq!((bar.ticks, bar.up_ticks, bar.down_ticks, bar.flat_ticks), (6, 2, 2, 2));
    }

    #[test]
    fn bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar() {
        let mut st = TickState::new();
        st.on_tick(100.0, T0, None);
        assert!(st.on_tick(100.5, T0 + 1_000, None).unwrap().closed.is_none());

        let out = st.on_tick(101.0, T0 + M15, None).unwrap();
        let (closed_candle, closed_bar) = out.closed.expect("bucket must close");
        assert!(closed_bar.closed);
        assert_eq!(closed_bar.time, T0);
        assert_eq!(closed_bar.ticks, 2);
        assert_eq!(closed_candle.volume, 2.0);

        let (live_candle, live_bar) = out.live;
        assert!(!live_bar.closed);
        assert_eq!(live_bar.time, T0 + M15);
        assert_eq!(live_bar.ticks, 1);
        // The tick rule carries across the boundary: 100.5 -> 101.0 is an uptick.
        assert_eq!(live_bar.up_ticks, 1);
        assert_eq!(live_candle.open, 101.0);
    }

    #[test]
    fn resubscribe_snapshot_and_late_ticks_are_not_counted() {
        let mut st = TickState::new();
        st.on_tick(100.0, T0, Some(1.0));
        st.on_tick(100.5, T0 + M15, Some(2.0));
        // Sifting replays the last tick from cache on (re)subscribe.
        assert!(st.on_tick(100.5, T0 + M15, Some(2.0)).is_none());
        // A straggler from the closed bucket must not roll the bar backwards.
        assert!(st.on_tick(99.0, T0 + 5, Some(1.0)).is_none());
        let bar = st.on_tick(100.6, T0 + M15 + 1, Some(1.0)).unwrap().live.1;
        assert_eq!(bar.time, T0 + M15);
        assert_eq!(bar.ticks, 2);
    }

    #[test]
    fn rolling_rate_counts_the_last_ten_seconds() {
        let mut st = TickState::new();
        for i in 0..20 {
            st.on_tick(100.0 + i as f64 * 0.01, T0 + i * 500, None); // 2 ticks/s
        }
        let bar = st.on_tick(101.0, T0 + 20 * 500, None).unwrap().live.1;
        assert_eq!(bar.ticks_per_sec, 2.0);
    }

    #[test]
    fn handle_text_writes_the_store_and_broadcasts_candle_plus_tick_volume() {
        let (bc, mut rx) = broadcast::channel(64);
        let (ctx, mut crx) = mpsc::channel(8);
        let store = TickVolumeStore::new(16);
        let mut st = TickState::new();

        for (p, t) in [(100.0, T0), (100.5, T0 + 1), (101.0, T0 + M15)] {
            handle_text(&tick(p, t), "XAUUSD", &mut st, &bc, &ctx, &store);
        }
        // Control frames and other symbols are ignored.
        handle_text(r#"{"f":"pong"}"#, "XAUUSD", &mut st, &bc, &ctx, &store);
        handle_text(&tick(1.0, T0 + M15 + 1).replace("XAUUSD", "XAGUSD"), "XAUUSD", &mut st, &bc, &ctx, &store);

        let mut kinds = Vec::new();
        while let Ok(f) = rx.try_recv() {
            kinds.push(match f {
                WsFrame::Candle { .. } => "candle",
                WsFrame::TickVolume { data } if data.closed => "tv_closed",
                WsFrame::TickVolume { .. } => "tv",
                _ => "other",
            });
        }
        assert_eq!(
            kinds,
            ["candle", "tv", "candle", "tv", "candle", "tv_closed", "candle", "tv"]
        );

        let closed = crx.try_recv().expect("closed candle goes to the VP consumer");
        assert_eq!((closed.time, closed.volume), (T0, 2.0));
        assert!(crx.try_recv().is_err());

        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);
        assert!(snap[0].closed && snap[0].ticks == 2);
        assert!(!snap[1].closed && snap[1].ticks == 1);
    }

    #[test]
    fn tick_volume_frame_wire_shape() {
        let mut st = TickState::new();
        let bar = st.on_tick(100.0, T0, None).unwrap().live.1;
        let json = serde_json::to_value(WsFrame::TickVolume { data: bar }).unwrap();
        assert_eq!(json["type"], "tick_volume");
        let mut keys: Vec<_> = json["data"].as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "close", "closed", "down_ticks", "flat_ticks", "last_tick", "source",
                "ticks", "ticks_per_sec", "time", "up_ticks"
            ]
        );
    }
}
