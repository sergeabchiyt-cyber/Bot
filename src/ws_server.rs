//! HTTP + WebSocket surface (C1 v2).
//!
//! Five `GET` routes and one upgrade. There is no write route anywhere: the
//! CORS layer only offers `GET, OPTIONS`, and the router only registers `get`.
//!
//! The socket is a one-way market feed. A client may send `subscribe` and
//! `heartbeat`; every other frame — including anything shaped like an order or
//! a fill — is dropped and never rebroadcast.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json, Router,
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{RwLock, broadcast};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::candles::{CAPACITY_H1, CAPACITY_M1, CAPACITY_M15, CandleBook};
use crate::config::Config;
use crate::publish::MarketPublisher;
use crate::session;
use crate::sifting_rest::SEED_BARS;
use crate::status::{BootInfo, FeedStatus, FreshnessTracker};
use crate::tick_volume::TickVolumeStore;
use crate::types::{
    BROADCAST_CAPACITY, CONTRACT_VERSION, Candle, Freshness, FreshnessState, HEARTBEAT_SECS,
    Interval, Publication, Topic, WsFrame, is_rejected_kind,
};

/// Closed candles replayed per resolution when a client subscribes. The full
/// history is a REST call (`GET /candles?interval=1h`): replay is only the
/// live edge.
fn replay_bars(interval: Interval) -> usize {
    match interval {
        Interval::M1 => 240,
        Interval::M15 => 96,
        Interval::H1 => 48,
    }
}

fn capacity(interval: Interval) -> usize {
    match interval {
        Interval::M1 => CAPACITY_M1,
        Interval::M15 => CAPACITY_M15,
        Interval::H1 => CAPACITY_H1,
    }
}

/// Everything a handler needs. Cheap to clone (every field is a handle).
#[derive(Clone)]
pub struct AppState {
    pub publisher: MarketPublisher,
    pub candles: CandleBook,
    pub tick_volume: TickVolumeStore,
    pub calendar: Arc<RwLock<serde_json::Value>>,
    pub status: FeedStatus,
    pub freshness: FreshnessTracker,
    pub boot: Arc<BootInfo>,
    pub config: Config,
}

/// Browser-facing CORS policy.
///
/// The Node 2 dashboard is a static site served from its own origin, so that
/// origin has to be allowed explicitly: no wildcard, no `Any`, and no
/// credentials — nothing here is cookie/token authenticated, and mirroring
/// arbitrary origins would let any page read the API.
///
/// `AllowOrigin::list` compares the request's `Origin` first and simply omits
/// the header when it does not match, which is what "only this origin may call
/// the API" requires (unlike `exact`, which is a constant header value emitted
/// to every caller).
pub fn cors_layer(allowed_origin: &str) -> CorsLayer {
    let origin = allowed_origin.parse::<HeaderValue>().unwrap_or_else(|e| {
        panic!("CORS_ALLOWED_ORIGIN must be a valid header value ({allowed_origin:?}): {e}")
    });

    CorsLayer::new()
        .allow_origin(AllowOrigin::list([origin]))
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::ACCEPT, header::ORIGIN])
        .max_age(Duration::from_secs(600))
        // tower-http's default `vary` only lists the preflight request headers.
        // Origin has to be in there too: the body is the same for everyone but
        // the allow-header differs per origin, so a shared cache (Render's
        // proxy) must never hand ours to another requester.
        .vary([
            header::ORIGIN,
            header::ACCESS_CONTROL_REQUEST_METHOD,
            header::ACCESS_CONTROL_REQUEST_HEADERS,
        ])
    // Deliberately no `.allow_credentials(true)`.
}

pub fn router(state: AppState) -> Router {
    let cors = cors_layer(&state.config.cors_allowed_origin);
    info!(
        origin = %state.config.cors_allowed_origin,
        "CORS: browser access to the REST API allowed for this origin"
    );

    Router::new()
        .route("/health", get(health))
        .route("/status", get(status_snapshot))
        .route("/candles", get(candles_snapshot))
        .route("/tick-volume", get(tick_volume_snapshot))
        .route("/calendar", get(calendar_snapshot))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(cors)
}

async fn health() -> &'static str {
    "ok"
}

/// Closed bars for one resolution, ascending and unique by `time`.
///
/// `interval` defaults to `15m` (the chart's original contract) and rejects
/// anything else with a 400 instead of silently serving another resolution.
/// Optional `limit` returns only the newest `N` closed bars (still ascending);
/// omitting `limit` serves the full retained series unchanged.
async fn candles_snapshot(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let interval = match params.get("interval") {
        None => Interval::M15,
        Some(raw) => match Interval::parse(raw) {
            Some(interval) => interval,
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "interval must be one of 1m, 15m, 1h",
                        "contract_version": CONTRACT_VERSION,
                    })),
                )
                    .into_response();
            }
        },
    };
    let bars: Vec<Candle> = match params.get("limit") {
        None => state.candles.closed(interval),
        Some(raw) => match raw.trim().parse::<std::num::NonZeroUsize>() {
            Ok(limit) => state.candles.recent(interval, limit.get()),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "limit must be a positive integer",
                        "contract_version": CONTRACT_VERSION,
                    })),
                )
                    .into_response();
            }
        },
    };
    ([(header::CACHE_CONTROL, "no-store")], Json(bars)).into_response()
}

/// Live tick-volume bars (oldest first; the last one may be the in-progress
/// bucket, `closed: false`).
async fn tick_volume_snapshot(State(state): State<AppState>) -> Response {
    let bars = state.tick_volume.snapshot();
    ([(header::CACHE_CONTROL, "no-store")], Json(bars)).into_response()
}

async fn calendar_snapshot(State(state): State<AppState>) -> Response {
    let body = state.calendar.read().await.clone();
    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

async fn status_snapshot(State(state): State<AppState>) -> Response {
    let body = state.status_document().await;
    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

impl AppState {
    /// The full `/status` document (also the payload of every `status` frame).
    pub async fn status_document(&self) -> serde_json::Value {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let freshness = self
            .freshness
            .snapshot(now_ms, session::market_open(now_ms));
        self.status_document_at(now_ms, &freshness).await
    }

    async fn status_document_at(&self, now_ms: i64, freshness: &Freshness) -> serde_json::Value {
        let current = session::current_session(now_ms);
        let mut intervals = serde_json::Map::new();
        for interval in Interval::ALL {
            let edge = self.candles.edge(interval);
            intervals.insert(
                interval.as_str().to_string(),
                serde_json::json!({
                    "bars": self.candles.bars(interval),
                    "capacity": capacity(interval),
                    "first_bar": self.candles.origin(interval),
                    "last_bar": edge,
                    "forming": self.candles.forming(interval).map(|bar| bar.time),
                    "edge_lag_minutes": edge.map(|time| (now_ms - time).max(0) / 60_000),
                }),
            );
        }

        let counters = self.candles.counters();
        let dropped_ticks = counters.dropped();

        let calendar = self.calendar.read().await;
        let calendar_summary = serde_json::json!({
            "source": calendar.get("source").cloned().unwrap_or(serde_json::Value::Null),
            "count": calendar.get("count").cloned().unwrap_or(serde_json::Value::Null),
            "updated": calendar.get("updated").cloned().unwrap_or(serde_json::Value::Null),
        });

        serde_json::json!({
            "service": "xauusd-engine",
            "version": env!("CARGO_PKG_VERSION"),
            "contract_version": CONTRACT_VERSION,
            "boot_id": self.boot.boot_id,
            "uptime_ms": self.boot.uptime_ms(now_ms),
            "market_open": current.market_open,
            "session": {
                "anchor": "18:00 America/New_York",
                "open_ms": current.open_ms,
                "close_ms": current.close_ms,
                "market_open": current.market_open,
            },
            "freshness": freshness,
            "feeds": self.status.snapshot(),
            "candles": {
                "seed_bars": SEED_BARS,
                "intervals": intervals,
                "counters": counters,
                "dropped_ticks": dropped_ticks,
            },
            "calendar": calendar_summary,
            "ws": {
                "topics": Topic::ALL.map(Topic::as_str),
                "heartbeat_secs": HEARTBEAT_SECS,
                "broadcast_capacity": BROADCAST_CAPACITY,
                "replay_bars": {
                    "1m": replay_bars(Interval::M1),
                    "15m": replay_bars(Interval::M15),
                    "1h": replay_bars(Interval::H1),
                },
            },
        })
    }

    async fn replay_status(&self) -> Publication {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let freshness = self
            .freshness
            .snapshot(now_ms, session::market_open(now_ms));
        Publication::replay(
            Topic::Status.as_str(),
            self.status_document_at(now_ms, &freshness).await,
        )
    }
}

/// Publish a `status` frame on **every** transition — freshness state, feed
/// state or market-open flag — not only when a feed reconnects.
///
/// One poll per second: the states it watches do not change faster than that,
/// and a tick-level status frame would drown the candles it describes.
pub fn spawn_status_poller(state: AppState) {
    tokio::spawn(async move {
        let mut last_freshness: Option<FreshnessState> = None;
        let mut last_market_open: Option<bool> = None;
        let mut last_feeds = String::new();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let now_ms = chrono::Utc::now().timestamp_millis();
            let current = session::current_session(now_ms);
            let freshness = state.freshness.snapshot(now_ms, current.market_open);
            let feeds = state.status.fingerprint();
            let changed = last_freshness != Some(freshness.state)
                || last_market_open != Some(current.market_open)
                || last_feeds != feeds;
            if !changed {
                continue;
            }
            last_freshness = Some(freshness.state);
            last_market_open = Some(current.market_open);
            last_feeds = feeds;
            let document = state.status_document_at(now_ms, &freshness).await;
            state.publisher.publish_status(document);
        }
    });
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn send_frame(sender: &mut SplitSink<WebSocket, Message>, frame: &Publication) -> bool {
    let json = match serde_json::to_string(frame) {
        Ok(json) => json,
        Err(_) => return false,
    };
    sender.send(Message::Text(json.into())).await.is_ok()
}

/// True when `frame` is a feed frame the client asked for.
///
/// Heartbeats and gap notices are sent directly by the socket loop, so they
/// never reach this filter.
fn frame_matches(frame: &Publication, topics: &[Topic], intervals: &[Interval]) -> bool {
    match frame.topic() {
        None => false,
        Some(Topic::Candle) => {
            if !topics.contains(&Topic::Candle) {
                return false;
            }
            frame
                .data
                .as_ref()
                .and_then(|data| data.get("interval"))
                .and_then(|value| value.as_str())
                .and_then(Interval::parse)
                .is_some_and(|interval| intervals.contains(&interval))
        }
        Some(topic) => topics.contains(&topic),
    }
}

/// Send the cached state a fresh subscriber needs before the live stream
/// starts: closed candles per requested resolution (ascending), tick volume,
/// and the calendar snapshot. Everything here is `replay: true` with no `seq`,
/// and the burst never contains a forming bucket.
async fn replay(
    state: &AppState,
    topics: &[Topic],
    intervals: &[Interval],
    sender: &mut SplitSink<WebSocket, Message>,
) -> bool {
    if topics.contains(&Topic::Candle) {
        for interval in intervals {
            for candle in state.candles.recent(*interval, replay_bars(*interval)) {
                let Ok(data) = serde_json::to_value(&candle) else {
                    continue;
                };
                let frame = Publication::replay(Topic::Candle.as_str(), data);
                if !send_frame(sender, &frame).await {
                    return false;
                }
            }
        }
    }
    if topics.contains(&Topic::TickVolume) {
        for bar in state.tick_volume.snapshot() {
            let Ok(data) = serde_json::to_value(&bar) else {
                continue;
            };
            let frame = Publication::replay(Topic::TickVolume.as_str(), data);
            if !send_frame(sender, &frame).await {
                return false;
            }
        }
    }
    if topics.contains(&Topic::Calendar) {
        let cached = state.calendar.read().await.clone();
        let frame = Publication::replay(Topic::Calendar.as_str(), cached);
        if !send_frame(sender, &frame).await {
            return false;
        }
    }
    true
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.publisher.subscribe();

    let session_id = format!(
        "sess-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    info!("WS session {session_id} opened");

    let mut topics: Vec<Topic> = Vec::new();
    let mut intervals: Vec<Interval> = Interval::ALL.to_vec();
    let mut subscribed = false;
    let mut heartbeat = tokio::time::interval(Duration::from_secs(HEARTBEAT_SECS));

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if !send_frame(&mut sender, &Publication::heartbeat()).await {
                    info!("WS session {session_id} closed while sending a heartbeat");
                    return;
                }
            }

            message = receiver.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                            continue;
                        };
                        // Explicit, logged refusal: nothing execution-shaped
                        // is parsed, cached or rebroadcast.
                        if is_rejected_kind(&value) {
                            info!("WS session {session_id} ignored an execution-shaped frame");
                            continue;
                        }
                        match serde_json::from_value::<WsFrame>(value) {
                            Ok(WsFrame::Subscribe {
                                topics: requested_topics,
                                intervals: requested_intervals,
                            }) => {
                                topics = requested_topics
                                    .iter()
                                    .filter_map(|topic| Topic::parse(topic.as_str()))
                                    .collect();
                                let wanted: Vec<Interval> = requested_intervals
                                    .iter()
                                    .filter_map(|raw| Interval::parse(raw.as_str()))
                                    .collect();
                                if !wanted.is_empty() {
                                    intervals = wanted;
                                }
                                subscribed = true;
                                info!(
                                    "WS session {session_id} subscribed to {:?} intervals {:?}",
                                    topics, intervals
                                );
                                if !replay(&state, &topics, &intervals, &mut sender).await {
                                    return;
                                }
                                if !send_frame(&mut sender, &state.replay_status().await).await {
                                    return;
                                }
                            }
                            Ok(WsFrame::Heartbeat) => {
                                if !send_frame(&mut sender, &Publication::heartbeat()).await {
                                    return;
                                }
                            }
                            Err(_) => {}
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        info!("WS session {session_id} error: {e}");
                        return;
                    }
                    None => {
                        info!("WS session {session_id} disconnected");
                        return;
                    }
                }
            }

            result = rx.recv() => {
                match result {
                    Ok(frame) => {
                        if !subscribed || !frame_matches(&frame, &topics, &intervals) {
                            continue;
                        }
                        if !send_frame(&mut sender, &frame).await {
                            info!("WS session {session_id} send failed");
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        // A slow subscriber lost frames: tell it how many
                        // instead of silently skipping candles.
                        info!("WS session {session_id} lagged {dropped} frame(s)");
                        if !send_frame(&mut sender, &Publication::gap(dropped)).await {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return;
                    }
                }
            }
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SYMBOL, Sequencer};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    const ALLOWED: &str = "https://static-dash-frontend.onrender.com";
    const FOREIGN: &str = "https://evil.example";
    const REST_ROUTES: [&str; 5] = [
        "/health",
        "/status",
        "/candles",
        "/tick-volume",
        "/calendar",
    ];

    fn candle(interval: Interval, time: i64) -> Candle {
        Candle {
            symbol: SYMBOL.to_string(),
            interval: interval.as_str().to_string(),
            time: interval.align(time),
            open: 3380.0,
            high: 3385.0,
            low: 3378.0,
            close: 3383.0,
            volume: 412.0,
            source: "sifting".to_string(),
            closed: true,
        }
    }

    fn bar(time: i64) -> crate::types::TickVolumeBar {
        crate::types::TickVolumeBar {
            symbol: SYMBOL.to_string(),
            interval: Interval::M15.as_str().to_string(),
            time,
            ticks: 412,
            up_ticks: 150,
            down_ticks: 140,
            flat_ticks: 122,
            close: 3383.0,
            last_tick: time + 899_000,
            ticks_per_sec: 0.4,
            closed: false,
            source: "sifting".to_string(),
        }
    }

    fn app(allowed: &str) -> Router {
        let (tx, _rx) = broadcast::channel(64);
        let mut config = Config::from_env();
        config.cors_allowed_origin = allowed.to_string();

        let store = TickVolumeStore::new(16);
        store.upsert(bar(1_700_000_100_000));

        let book = CandleBook::new();
        for interval in Interval::ALL {
            book.seed(interval, &[candle(interval, 1_700_000_100_000)]);
        }

        router(AppState {
            publisher: MarketPublisher::new(
                tx,
                Sequencer::new(),
                store.clone(),
                FreshnessTracker::new("sifting"),
            ),
            candles: book,
            tick_volume: store,
            calendar: Arc::new(RwLock::new(serde_json::json!({
                "source": "forexfactory",
                "count": 0,
                "events": [],
                "updated": 1_700_000_000_000_i64
            }))),
            status: FeedStatus::new(),
            freshness: FreshnessTracker::new("sifting"),
            boot: Arc::new(BootInfo::new(1_700_000_000_000)),
            config,
        })
    }

    fn build(method: &str, uri: &str, origin: Option<&str>) -> Request<Body> {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        request.body(Body::empty()).expect("request")
    }

    async fn send(method: &str, uri: &str, origin: Option<&str>) -> Response {
        app(ALLOWED)
            .oneshot(build(method, uri, origin))
            .await
            .expect("router response")
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    fn acao(response: &Response) -> Option<String> {
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .map(|value| value.to_str().expect("header value").to_string())
    }

    #[tokio::test]
    async fn allowed_origin_may_read_every_rest_route() {
        for uri in REST_ROUTES {
            let response = send("GET", uri, Some(ALLOWED)).await;
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(acao(&response).as_deref(), Some(ALLOWED), "{uri}");
            assert_ne!(
                acao(&response).as_deref(),
                Some("*"),
                "{uri} must not use a wildcard"
            );

            let vary = response
                .headers()
                .get_all(header::VARY)
                .into_iter()
                .map(|value| value.to_str().unwrap().to_lowercase())
                .collect::<Vec<_>>()
                .join(", ");
            assert!(vary.contains("origin"), "{uri} vary: {vary}");
        }
    }

    #[tokio::test]
    async fn only_the_configured_origin_is_ever_allowed() {
        for uri in REST_ROUTES {
            let response = send("GET", uri, Some(FOREIGN)).await;
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(
                acao(&response),
                None,
                "{uri} leaked an allow-header to a foreign origin"
            );

            let response = send("GET", uri, None).await;
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(acao(&response), None, "{uri} sent CORS with no Origin");
        }
    }

    #[tokio::test]
    async fn cors_grant_is_scoped_to_the_configured_origin() {
        let response = send("GET", "/status", Some(ALLOWED)).await;
        assert!(
            !response
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            "credentials must not be enabled"
        );

        let response = app("https://other.onrender.com")
            .oneshot(build("GET", "/status", Some(ALLOWED)))
            .await
            .expect("router response");
        assert_eq!(acao(&response), None);

        let dev = "http://localhost:5173";
        let response = app(dev)
            .oneshot(build("GET", "/candles", Some(dev)))
            .await
            .expect("router response");
        assert_eq!(acao(&response).as_deref(), Some(dev));
    }

    #[tokio::test]
    async fn preflight_is_answered_for_the_allowed_origin_only() {
        let request = Request::builder()
            .method("OPTIONS")
            .uri("/candles")
            .header(header::ORIGIN, ALLOWED)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
            .body(Body::empty())
            .expect("request");
        let response = app(ALLOWED).oneshot(request).await.expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(acao(&response).as_deref(), Some(ALLOWED));
        let methods = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .expect("allow-methods")
            .to_str()
            .expect("header value");
        assert!(
            methods.contains("GET") && methods.contains("OPTIONS"),
            "{methods}"
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_MAX_AGE)
                .expect("max-age")
                .to_str()
                .unwrap(),
            "600"
        );

        let request = Request::builder()
            .method("OPTIONS")
            .uri("/candles")
            .header(header::ORIGIN, FOREIGN)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .expect("request");
        let response = app(ALLOWED).oneshot(request).await.expect("response");
        assert_eq!(
            acao(&response),
            None,
            "a foreign origin must not pass preflight"
        );
    }

    /// Node 1 is a market-data service: there is no route that accepts an
    /// order, a fill or any other broker write.
    #[tokio::test]
    async fn no_route_accepts_a_broker_write() {
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            for uri in REST_ROUTES {
                let response = send(method, uri, Some(ALLOWED)).await;
                assert!(
                    response.status().is_client_error(),
                    "{method} {uri} was accepted ({}) - Node 1 takes no writes",
                    response.status()
                );
            }
        }

        let response = send("POST", "/candles", Some(ALLOWED)).await;
        if let Some(allow) = response.headers().get(header::ALLOW) {
            let allow = allow.to_str().unwrap();
            assert!(allow.contains("GET"), "GET must stay advertised");
            assert!(!allow.contains("POST"), "POST must not be advertised");
        }
    }

    #[tokio::test]
    async fn candles_are_served_per_interval_and_reject_an_unknown_one() {
        for interval in Interval::ALL {
            let uri = format!("/candles?interval={}", interval.as_str());
            let response = send("GET", &uri, Some(ALLOWED)).await;
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(
                response.headers()[header::CACHE_CONTROL]
                    .to_str()
                    .expect("cache-control"),
                "no-store"
            );

            let body = body_json(response).await;
            let bars = body.as_array().expect("array");
            assert_eq!(bars.len(), 1, "{uri}");
            assert_eq!(bars[0]["interval"], interval.as_str());
            assert_eq!(bars[0]["symbol"], SYMBOL);
            assert_eq!(bars[0]["closed"], true);
            assert_eq!(
                bars[0]["time"].as_i64().unwrap() % interval.millis(),
                0,
                "{uri} served an unaligned bar"
            );
        }

        // No `interval` keeps the chart's original 15m default.
        let body = body_json(send("GET", "/candles", Some(ALLOWED)).await).await;
        assert_eq!(body[0]["interval"], "15m");

        for bad in ["5m", "1d", "", "hour"] {
            let uri = format!("/candles?interval={bad}");
            let response = send("GET", &uri, Some(ALLOWED)).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    #[tokio::test]
    async fn status_reports_the_c1_envelope() {
        let body = body_json(send("GET", "/status", Some(ALLOWED)).await).await;

        assert_eq!(body["contract_version"], "2.0");
        assert_eq!(body["service"], "xauusd-engine");
        assert!(
            body["boot_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("boot-")),
            "boot_id: {}",
            body["boot_id"]
        );
        assert_eq!(body["freshness"]["feed"], "sifting");
        let state = body["freshness"]["state"]
            .as_str()
            .expect("freshness state");
        assert!(
            ["live", "stale", "offline", "closed_market"].contains(&state),
            "unexpected freshness state {state}"
        );
        assert!(body["market_open"].is_boolean());
        assert_eq!(body["session"]["anchor"], "18:00 America/New_York");
        assert_eq!(body["candles"]["intervals"]["1h"]["bars"].as_u64(), Some(1));
        assert!(body["candles"]["counters"]["accepted"].as_u64().is_some());
        assert!(body["feeds"].is_object());
        assert_eq!(body["ws"]["heartbeat_secs"], 30);

        // Nothing that was removed may come back into the document.
        for gone in ["volume_profile", "strategy", "demo", "ai"] {
            assert!(body.get(gone).is_none(), "{gone} must not be reported");
        }
        for key in [
            "side",
            "stop_loss",
            "take_profit",
            "lots",
            "action",
            "venue",
        ] {
            assert!(
                !body.to_string().contains(&format!("\"{key}\"")),
                "{key} leaked into /status"
            );
        }
    }

    #[tokio::test]
    async fn boot_id_is_stable_across_requests() {
        let router = app(ALLOWED);
        let first = body_json(
            router
                .clone()
                .oneshot(build("GET", "/status", None))
                .await
                .expect("response"),
        )
        .await;
        let second = body_json(
            router
                .oneshot(build("GET", "/status", None))
                .await
                .expect("response"),
        )
        .await;
        assert_eq!(first["boot_id"], second["boot_id"]);
        assert_eq!(first["contract_version"], second["contract_version"]);
    }

    #[tokio::test]
    async fn tick_volume_is_served_uncached_with_its_envelope() {
        let response = send("GET", "/tick-volume", Some(ALLOWED)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL]
                .to_str()
                .expect("cache-control"),
            "no-store"
        );

        let body = body_json(response).await;
        let bars = body.as_array().expect("array");
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0]["symbol"], SYMBOL);
        assert_eq!(bars[0]["interval"], "15m");
        assert_eq!(bars[0]["ticks"], 412);
        assert_eq!(
            bars[0]["up_ticks"].as_u64().unwrap()
                + bars[0]["down_ticks"].as_u64().unwrap()
                + bars[0]["flat_ticks"].as_u64().unwrap(),
            412
        );
        assert_eq!(bars[0]["closed"], false);
    }

    /// Routes that belonged to the removed services must be gone, not
    /// silently aliased to something else.
    #[tokio::test]
    async fn removed_routes_are_not_served() {
        for uri in ["/levels", "/vp", "/strategy", "/demo", "/ai"] {
            let response = send("GET", uri, Some(ALLOWED)).await;
            assert_eq!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{uri} still has a handler"
            );
        }
    }

    #[test]
    fn candle_frames_are_filtered_by_the_requested_intervals() {
        let publication = Publication::live(
            "candle",
            1,
            serde_json::json!({ "interval": "1h", "closed": true }),
        );
        assert!(frame_matches(
            &publication,
            &[Topic::Candle],
            &[Interval::H1]
        ));
        assert!(!frame_matches(
            &publication,
            &[Topic::Candle],
            &[Interval::M15]
        ));
        assert!(!frame_matches(
            &publication,
            &[Topic::TickVolume],
            &[Interval::H1]
        ));

        let calendar = Publication::live("calendar", 2, serde_json::json!({}));
        assert!(frame_matches(&calendar, &[Topic::Calendar], &[]));
        assert!(!frame_matches(&calendar, &[Topic::Candle], &[]));

        // A heartbeat is never forwarded through the topic filter.
        assert!(!frame_matches(
            &Publication::heartbeat(),
            &Topic::ALL,
            &Interval::ALL
        ));
    }

    #[test]
    fn every_interval_replays_a_usable_tail() {
        for interval in Interval::ALL {
            assert!(replay_bars(interval) >= 48, "{}", interval.as_str());
            assert!(capacity(interval) >= replay_bars(interval));
        }
    }

    #[test]
    fn a_lagged_subscriber_is_told_how_much_it_lost() {
        // The socket loop maps `Lagged(n)` to `Publication::gap(n)`; this pins
        // the frame it produces.
        let gap = Publication::gap(7);
        assert_eq!(gap.kind, "gap");
        assert_eq!(gap.data.as_ref().unwrap()["dropped"], 7);
        assert_eq!(gap.seq, None);
        assert!(!gap.replay);
    }

    #[test]
    fn malformed_origin_panics_at_boot() {
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let good = std::panic::catch_unwind(|| cors_layer("https://ok.example"));
        let bad = std::panic::catch_unwind(|| cors_layer("https://bad example\r\n"));
        std::panic::set_hook(quiet);

        assert!(good.is_ok());
        assert!(bad.is_err());
    }

    #[tokio::test]
    async fn candles_limit_returns_last_n_closed_bars_and_preserves_default() {
        const {
            assert!(
                CAPACITY_M1 >= 43_200,
                "full 1m retention must remain intact"
            );
            assert!(
                BROADCAST_CAPACITY >= 4_096,
                "broadcast buffer must stay at least 4096"
            );
        }

        let (tx, _rx) = broadcast::channel(64);
        let mut config = Config::from_env();
        config.cors_allowed_origin = ALLOWED.to_string();
        let store = TickVolumeStore::new(16);
        let book = CandleBook::new();
        let base_t = 1_700_000_100_000 - 1_700_000_100_000 % 3_600_000;
        let seeded: Vec<Candle> = (0..5_i64)
            .map(|i| candle(Interval::M1, base_t + i * 60_000))
            .collect();
        book.seed(Interval::M1, &seeded);
        // Add a live forming bucket at base_t + 5m to ensure `/candles` never returns it.
        book.on_tick(3384.0, base_t + 5 * 60_000, None, base_t + 6 * 60_000);

        let router = router(AppState {
            publisher: MarketPublisher::new(
                tx,
                Sequencer::new(),
                store.clone(),
                FreshnessTracker::new("sifting"),
            ),
            candles: book,
            tick_volume: store,
            calendar: Arc::new(RwLock::new(serde_json::json!({
                "source": "ff_json",
                "count": 0,
                "events": [],
                "updated": base_t
            }))),
            status: FeedStatus::new(),
            freshness: FreshnessTracker::new("sifting"),
            boot: Arc::new(BootInfo::new(base_t)),
            config,
        });

        // Default `/candles?interval=1m` returns all 5 closed bars.
        let full = body_json(
            router
                .clone()
                .oneshot(build("GET", "/candles?interval=1m", None))
                .await
                .expect("full response"),
        )
        .await;
        let full_arr = full.as_array().expect("array");
        assert_eq!(full_arr.len(), 5);
        assert_eq!(full_arr[0]["time"], base_t);
        assert_eq!(full_arr[4]["time"], base_t + 4 * 60_000);

        // `?limit=2` returns the newest 2 closed bars in ascending order.
        let tail = body_json(
            router
                .clone()
                .oneshot(build("GET", "/candles?interval=1m&limit=2", None))
                .await
                .expect("limit=2 response"),
        )
        .await;
        let tail_arr = tail.as_array().expect("array");
        assert_eq!(tail_arr.len(), 2);
        assert_eq!(tail_arr[0]["time"], base_t + 3 * 60_000);
        assert_eq!(tail_arr[1]["time"], base_t + 4 * 60_000);
        assert!(tail_arr.iter().all(|c| c["closed"] == true));

        // `?limit=99` caps cleanly at available closed bars.
        let capped = body_json(
            router
                .clone()
                .oneshot(build("GET", "/candles?interval=1m&limit=99", None))
                .await
                .expect("limit=99 response"),
        )
        .await;
        assert_eq!(capped.as_array().unwrap().len(), 5);

        // Malformed or non-positive `limit` returns 400 with contract_version.
        for bad in ["0", "-3", "abc", ""] {
            let uri = format!("/candles?interval=1m&limit={bad}");
            let resp = router
                .clone()
                .oneshot(build("GET", &uri, None))
                .await
                .expect("bad limit response");
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{uri}");
            let err_body = body_json(resp).await;
            assert_eq!(err_body["contract_version"], CONTRACT_VERSION, "{uri}");
        }
    }

    #[tokio::test]
    async fn ws_broadcast_overflow_emits_gap_frame_over_the_socket() {
        let (tx, _rx) = broadcast::channel(4);
        let mut config = Config::from_env();
        config.cors_allowed_origin = ALLOWED.to_string();
        let store = TickVolumeStore::new(16);
        let book = CandleBook::new();
        let base_t = 1_700_000_100_000 - 1_700_000_100_000 % 3_600_000;
        book.seed(Interval::M15, &[candle(Interval::M15, base_t)]);

        let publisher = MarketPublisher::new(
            tx,
            Sequencer::new(),
            store.clone(),
            FreshnessTracker::new("sifting"),
        );

        let app = router(AppState {
            publisher: publisher.clone(),
            candles: book.clone(),
            tick_volume: store,
            calendar: Arc::new(RwLock::new(serde_json::json!({
                "source": "ff_json",
                "count": 0,
                "events": [],
                "updated": base_t
            }))),
            status: FeedStatus::new(),
            freshness: FreshnessTracker::new("sifting"),
            boot: Arc::new(BootInfo::new(base_t)),
            config,
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
            .await
            .expect("ws connect");
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            r#"{"type":"subscribe","topics":["candle"],"intervals":["15m"]}"#.into(),
        ))
        .await
        .expect("send subscribe");

        // Drain the replay burst through the closing replay `status` frame.
        loop {
            let msg = ws.next().await.expect("frame").expect("ws message");
            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                let frame: serde_json::Value = serde_json::from_str(&text).expect("json");
                if frame["type"] == "status" && frame["replay"] == true {
                    break;
                }
            }
        }

        // Overflow the 4-slot broadcast ring buffer without reading from `ws`.
        for step in 1..=10_i64 {
            let ev = book.on_tick(
                3380.0 + step as f64 * 0.1,
                base_t + 900_000 + step * 1_000,
                None,
                base_t + 1_800_000,
            );
            publisher.publish_tick(&ev);
        }

        // Next read must deliver a `gap` control frame reporting the lost frames.
        let mut saw_gap = false;
        for _ in 0..16 {
            let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
                .await
                .expect("timeout waiting for gap")
                .expect("stream open")
                .expect("ws frame");
            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                let frame: serde_json::Value = serde_json::from_str(&text).expect("json");
                if frame["type"] == "gap" {
                    assert!(frame["data"]["dropped"].as_u64().unwrap_or(0) > 0);
                    assert!(frame.get("seq").is_none());
                    assert!(frame.get("replay").is_none());
                    saw_gap = true;
                    break;
                }
            }
        }
        assert!(saw_gap, "overflow must emit a gap frame");
    }
}
