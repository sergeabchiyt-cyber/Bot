use std::sync::Arc;
use std::time::Duration;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderValue, Method},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, RwLock};
use tracing::info;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::ai_cache::AiCache;
use crate::config::Config;
use crate::status::FeedStatus;
use crate::tick_volume::TickVolumeStore;
use crate::types::{TickVolumeBar, VpLevels, VpCandle, WsFrame};
use crate::volume_profile::VolumeProfileEngine;

/// Fourteen ATR true ranges need fifteen OHLC bars (the first close is the
/// reference close for the first range). Replay just this recent candle tail
/// over WS; clients that need the full chart history can use GET /candles.
const CANDLE_REPLAY_LIMIT: usize = 15;

#[derive(Clone)]
pub struct AppState {
    pub tx: broadcast::Sender<WsFrame>,
    pub subscriptions: Arc<DashMap<String, Vec<String>>>,
    pub cached_levels: Arc<RwLock<Vec<VpLevels>>>,
    pub cached_candles: Arc<RwLock<Vec<VpCandle>>>,
    pub cached_calendar: Arc<RwLock<serde_json::Value>>,
    pub tick_volume: TickVolumeStore,
    pub vp: Arc<RwLock<VolumeProfileEngine>>,
    pub status: FeedStatus,
    pub config: Config,
    /// Everything Node3 reports back (transcripts, sentiment, health).
    pub ai: Arc<AiCache>,
}

/// Browser-facing CORS policy.
///
/// The Node2 dashboard is a static site served from its own origin, so that
/// origin has to be allowed explicitly: no wildcard, no `Any`, and no
/// credentials — nothing here is cookie/token authenticated, and mirroring
/// arbitrary origins would let any page read the API.
///
/// `AllowOrigin::list` is used instead of `AllowOrigin::exact` on purpose.
/// `exact` is a *constant* header value, so tower-http emits
/// `access-control-allow-origin` on every response regardless of who asked —
/// a foreign origin would still see it (harmless to browsers, which block the
/// read, but it also makes the response cacheable under a lie). `list` compares
/// the request's `Origin` first and simply omits the header when it does not
/// match, which is what "only this origin may call the API" requires.
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
        // Origin has to be in there too: the response body is the same for
        // everyone but the allow-header differs per origin, so a shared cache
        // (Render's proxy) must never hand ours to another requester.
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
        .route("/levels", get(levels_snapshot))
        .route("/candles", get(candles_snapshot))
        .route("/tick-volume", get(tick_volume_snapshot))
        .route("/calendar", get(calendar_snapshot))
        .route("/ai", get(ai_snapshot))
        .route("/status", get(status_snapshot))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(cors)
}

async fn health() -> &'static str {
    "ok"
}

async fn levels_snapshot(State(state): State<AppState>) -> Json<Vec<VpLevels>> {
    let vp = state.vp.read().await;
    Json(vp.all_levels())
}

async fn candles_snapshot(State(state): State<AppState>) -> Json<Vec<VpCandle>> {
    let candles = state.cached_candles.read().await;
    Json(candles.clone())
}

/// Live tick-volume bars built since boot (oldest first; the last one may be
/// the in-progress bucket, `closed: false`). History before boot is the
/// `volume` field of `/candles`, which uses the same tick-count unit.
async fn tick_volume_snapshot(State(state): State<AppState>) -> Response {
    let bars: Vec<TickVolumeBar> = state.tick_volume.snapshot();
    ([(header::CACHE_CONTROL, "no-store")], Json(bars)).into_response()
}

async fn calendar_snapshot(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.cached_calendar.read().await.clone())
}

/// What the econ news delivered today: Node3's transcripts + sentiment
/// (plus last health), filtered to the current UTC day.
async fn ai_snapshot(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.ai.snapshot_today())
}

/// Fill in the `data` envelope at the edge for loose producer payloads:
/// `{"type":"learn","features":[..],"target":1}` and
/// `{"type":"audio_chunk","audio":"<b64>"}` are accepted alongside the
/// canonical `{"type":"...","data":{...}}` shape Node3 uses, and normalized
/// before rebroadcast so subscribers always see one wire format.
fn normalize_envelope(raw: &str, frame: WsFrame) -> WsFrame {
    let needs_payload = match &frame {
        WsFrame::AudioChunk { data } | WsFrame::Learn { data } => data.is_null(),
        _ => return frame,
    };
    if !needs_payload {
        return frame;
    }
    let Ok(obj) = serde_json::from_str::<serde_json::Value>(raw) else {
        return frame;
    };
    let Some(map) = obj.as_object() else {
        return frame;
    };
    match frame {
        WsFrame::AudioChunk { .. } => {
            let payload = if let Some(a) = map.get("audio") {
                serde_json::json!({ "data": a.clone() })
            } else if let Some(a) = map.get("payload") {
                serde_json::json!({ "data": a.clone() })
            } else {
                serde_json::Value::Null
            };
            WsFrame::AudioChunk { data: payload }
        }
        WsFrame::Learn { .. } => {
            let mut m = serde_json::Map::new();
            for k in ["features", "target"] {
                if let Some(v) = map.get(k) {
                    m.insert(k.to_string(), v.clone());
                }
            }
            WsFrame::Learn {
                data: serde_json::Value::Object(m),
            }
        }
        other => other,
    }
}

async fn status_snapshot(State(state): State<AppState>) -> Response {
    let body = serde_json::json!({
        "status": state.status.snapshot(),
        "venue": format!("{:?}", state.config.execution_venue()),
        "version": env!("CARGO_PKG_VERSION"),
    });
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(body),
    )
        .into_response()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let session_id = format!(
        "sess-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    info!("WS session {} opened", session_id);

    let mut topics: Vec<String> = Vec::new();
    let mut subscribed = false;
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let json = serde_json::to_string(&WsFrame::Heartbeat).unwrap_or_default();
                if sender.send(Message::Text(json.into())).await.is_err() {
                    info!("WS session {} send failed", session_id);
                    return;
                }
            }

            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                          match frame {
                            WsFrame::Subscribe { topics: t } => {
                                info!("WS session {} subscribed to {:?}", session_id, t);
                                topics = t;
                                subscribed = true;

                                // Replay cached state as soon as the client
                                // subscribes (the first message after connect).
                                // Clone under the read lock, then release it
                                // before any network write.
                                if topics.iter().any(|x| x == "levels") {
                                    let cached = state.cached_levels.read().await.clone();
                                    for lvl in cached {
                                        let frame = WsFrame::Levels { data: lvl };
                                        let json = serde_json::to_string(&frame).unwrap_or_default();
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                                if topics.iter().any(|x| x == "candle") {
                                    let cached = {
                                        let candles = state.cached_candles.read().await;
                                        let first = candles.len().saturating_sub(CANDLE_REPLAY_LIMIT);
                                        candles[first..].to_vec()
                                    };
                                    for c in cached {
                                        let frame = WsFrame::Candle { data: c };
                                        let json = serde_json::to_string(&frame).unwrap_or_default();
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                                if topics.iter().any(|x| x == "tick_volume") {
                                    for bar in state.tick_volume.snapshot() {
                                        let frame = WsFrame::TickVolume { data: bar };
                                        let json = serde_json::to_string(&frame).unwrap_or_default();
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                                if topics.iter().any(|x| x == "calendar") {
                                    let cached = state.cached_calendar.read().await.clone();
                                    let frame = WsFrame::Calendar { data: cached };
                                    let json = serde_json::to_string(&frame).unwrap_or_default();
                                    if sender.send(Message::Text(json.into())).await.is_err() {
                                        return;
                                    }
                                }
                                let frame = WsFrame::Status { data: state.status.snapshot() };
                                let json = serde_json::to_string(&frame).unwrap_or_default();
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    return;
                                }
                            }

                            // AI ingest: external producers (the econ monitor,
                            // scripts) may publish audio chunks / learner
                            // samples through the WS itself. The frame is
                            // rebroadcast on the bus so Node3 — subscribed to
                            // `audio_chunk` + `learn` — receives it.
                            WsFrame::AudioChunk { .. } | WsFrame::Learn { .. } => {
                                let frame = normalize_envelope(&text, frame);
                                let _ = state.tx.send(frame);
                            }

                            // Node3 (or another execution producer) publishes
                            // completed orders/signals on `trades`. Rebroadcast
                            // unchanged so every dashboard subscribed to the
                            // trades topic sees the same TradeEvent envelope.
                            WsFrame::Trades { .. } => {
                                let _ = state.tx.send(frame);
                            }

                            // Node3 answers: cache for GET /ai and fan out to
                            // subscribers of the matching topic.
                            WsFrame::Transcript { .. }
                            | WsFrame::Sentiment { .. }
                            | WsFrame::Health { .. }
                            | WsFrame::Prediction { .. } => {
                                state.ai.record(&frame);
                                let _ = state.tx.send(frame);
                            }

                            // Requests aimed at the AI node — rebroadcast so
                            // the fan-out forwards them past the topic filter
                            // (Node3 subscribes to audio_chunk + learn).
                            WsFrame::SentimentReq { .. } | WsFrame::PredictReq { .. } => {
                                let _ = state.tx.send(frame);
                            }

                            _ => {}
                          }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => {
                        info!("WS session {} disconnected", session_id);
                        return;
                    }
                }
            }

            result = rx.recv() => {
                match result {
                    Ok(frame) => {
                        if !subscribed { continue; }
                        let topic = match &frame {
                            WsFrame::Levels { .. } => "levels",
                            WsFrame::Candle { .. } => "candle",
                            WsFrame::TickVolume { .. } => "tick_volume",
                            WsFrame::Bubbles { .. } => "bubbles",
                            WsFrame::Trades { .. } => "trades",
                            WsFrame::Calendar { .. } => "calendar",
                            WsFrame::Status { .. } => "status",
                            WsFrame::AudioChunk { .. } => "audio_chunk",
                            WsFrame::Learn { .. } => "learn",
                            WsFrame::Transcript { .. } => "transcript",
                            WsFrame::Sentiment { .. } => "sentiment",
                            WsFrame::Health { .. } => "health",
                            WsFrame::Prediction { .. } => "prediction",
                            WsFrame::Heartbeat
                            | WsFrame::SentimentReq { .. }
                            | WsFrame::PredictReq { .. } => {
                                // keepalive for proxies and requests aimed at
                                // the AI node — always forwarded, regardless
                                // of topic subscription
                                let json = serde_json::to_string(&frame).unwrap_or_default();
                                if sender.send(Message::Text(json.into())).await.is_err() {
                                    info!("WS session {} send failed", session_id);
                                    return;
                                }
                                continue;
                            }
                            WsFrame::Subscribe { .. } => continue,
                        };
                        if !topics.iter().any(|t| t == topic) { continue; }

                        let json = serde_json::to_string(&frame).unwrap_or_default();
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            info!("WS session {} send failed", session_id);
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        info!("WS session {} lagged {} frames", session_id, n);
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
// CORS policy tests — the Node2 static site lives on its own origin, so
// these pin down exactly which requester gets an allow-header.
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    const ALLOWED: &str = "https://static-dash-frontend.onrender.com";
    const FOREIGN: &str = "https://evil.example";
    const REST_ROUTES: [&str; 7] = [
        "/health",
        "/levels",
        "/candles",
        "/tick-volume",
        "/calendar",
        "/ai",
        "/status",
    ];

    /// Router with an empty-but-valid snapshot cache, configured for `allowed`.
    fn app(allowed: &str) -> Router {
        let (tx, _rx) = broadcast::channel(16);
        let mut config = Config::from_env();
        config.cors_allowed_origin = allowed.to_string();

        let state = AppState {
            tx: tx.clone(),
            subscriptions: Arc::new(DashMap::new()),
            cached_levels: Arc::new(RwLock::new(Vec::new())),
            cached_candles: Arc::new(RwLock::new(vec![VpCandle {
                time: 1_700_000_000_000,
                open: 3381.0,
                high: 3385.5,
                low: 3378.25,
                close: 3383.75,
                volume: 412.0,
                source: "sifting".into(),
            }])),
            cached_calendar: Arc::new(RwLock::new(serde_json::json!({
                "source": "forexfactory", "count": 0, "events": []
            }))),
            tick_volume: {
                let store = TickVolumeStore::new(16);
                store.upsert(TickVolumeBar {
                    time: 1_700_000_000_000,
                    ticks: 412,
                    up_ticks: 150,
                    down_ticks: 140,
                    flat_ticks: 122,
                    close: 3383.75,
                    last_tick: 1_700_000_899_000,
                    ticks_per_sec: 0.4,
                    closed: false,
                    source: "sifting".into(),
                });
                store
            },
            vp: Arc::new(RwLock::new(VolumeProfileEngine::new())),
            status: FeedStatus::new(tx),
            config,
            ai: Arc::new(AiCache::new()),
        };
        router(state)
    }

    fn build(method: &str, uri: &str, origin: Option<&str>) -> Request<Body> {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(o) = origin {
            req = req.header(header::ORIGIN, o);
        }
        req.body(Body::empty()).unwrap()
    }

    async fn send(method: &str, uri: &str, origin: Option<&str>) -> Response {
        app(ALLOWED)
            .oneshot(build(method, uri, origin))
            .await
            .unwrap()
    }

    fn acao(res: &Response) -> Option<String> {
        res.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .map(|v| v.to_str().unwrap().to_string())
    }

    #[tokio::test]
    async fn allowed_origin_may_read_every_rest_route() {
        for uri in REST_ROUTES {
            let res = send("GET", uri, Some(ALLOWED)).await;
            assert_eq!(res.status(), StatusCode::OK, "{uri}");
            assert_eq!(acao(&res).as_deref(), Some(ALLOWED), "{uri}");
            assert_ne!(acao(&res).as_deref(), Some("*"), "{uri} must not use a wildcard");

            let vary = res
                .headers()
                .get_all(header::VARY)
                .into_iter()
                .map(|v| v.to_str().unwrap().to_lowercase())
                .collect::<Vec<_>>()
                .join(", ");
            assert!(vary.contains("origin"), "{uri} vary: {vary}");
        }
    }

    #[tokio::test]
    async fn only_the_configured_origin_is_ever_allowed() {
        for uri in REST_ROUTES {
            // Foreign origin and non-browser clients (no Origin at all, e.g.
            // curl or another server) get the data but no allow-header, so a
            // browser on any other page cannot read the response.
            let res = send("GET", uri, Some(FOREIGN)).await;
            assert_eq!(res.status(), StatusCode::OK, "{uri}");
            assert_eq!(acao(&res), None, "{uri} leaked an allow-header to a foreign origin");

            let res = send("GET", uri, None).await;
            assert_eq!(res.status(), StatusCode::OK, "{uri}");
            assert_eq!(acao(&res), None, "{uri} sent CORS with no Origin");
        }
    }

    #[tokio::test]
    async fn cors_grant_is_scoped_to_the_configured_origin() {
        // Nothing is authenticated with cookies/tokens, so credentials must stay
        // off — with them a wildcard would expose the API to any site.
        let res = send("GET", "/levels", Some(ALLOWED)).await;
        assert!(
            !res.headers().contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            "credentials must not be enabled"
        );

        // An origin that is not the site's never unlocks the API, even if it
        // shares the render.com suffix.
        let res = app("https://other.onrender.com")
            .oneshot(build("GET", "/levels", Some(ALLOWED)))
            .await
            .unwrap();
        assert_eq!(acao(&res), None);

        // The grant follows the configuration, so a moved site (or a local dev
        // server) only needs CORS_ALLOWED_ORIGIN — nothing is hardcoded.
        let dev = "http://localhost:5173";
        let res = app(dev)
            .oneshot(build("GET", "/levels", Some(dev)))
            .await
            .unwrap();
        assert_eq!(acao(&res).as_deref(), Some(dev));
        let res = app(dev).oneshot(build("GET", "/levels", Some(ALLOWED))).await.unwrap();
        assert_eq!(acao(&res), None);
    }

    #[tokio::test]
    async fn preflight_is_answered_for_the_allowed_origin_only() {
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/candles")
            .header(header::ORIGIN, ALLOWED)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
            .body(Body::empty())
            .unwrap();
        let res = app(ALLOWED).oneshot(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(acao(&res).as_deref(), Some(ALLOWED));
        let methods = res
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(methods.contains("GET") && methods.contains("OPTIONS"), "{methods}");
        assert!(
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("content-type")
        );
        assert_eq!(
            res.headers()
                .get(header::ACCESS_CONTROL_MAX_AGE)
                .unwrap()
                .to_str()
                .unwrap(),
            "600"
        );
        assert!(!res.headers().contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS));

        let req = Request::builder()
            .method("OPTIONS")
            .uri("/candles")
            .header(header::ORIGIN, FOREIGN)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .unwrap();
        let res = app(ALLOWED).oneshot(req).await.unwrap();
        assert_eq!(acao(&res), None, "a foreign origin must not pass preflight");
    }

    #[tokio::test]
    async fn payload_shape_is_untouched_by_the_layer() {
        // /candles must keep returning SiftingIO candle objects; the CORS layer
        // may not rewrap or rename anything.
        let res = send("GET", "/candles", Some(ALLOWED)).await;
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let candles = json.as_array().unwrap();
        assert_eq!(candles.len(), 1);
        let c = &candles[0];
        assert_eq!(c["source"], "sifting");
        assert_eq!(c["time"], 1_700_000_000_000i64);
        for key in ["open", "high", "low", "close", "volume"] {
            assert!(c[key].is_f64(), "missing numeric field {key}");
        }
        // Exactly the seven documented keys, no extras (no key or token ever
        // rides along in the body).
        let mut keys: Vec<String> = c.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "close", "high", "low", "open", "source", "time", "volume"
            ]
        );
    }

    #[tokio::test]
    async fn tick_volume_route_serves_the_live_bars_uncached() {
        let res = send("GET", "/tick-volume", Some(ALLOWED)).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(header::CACHE_CONTROL).unwrap().to_str().unwrap(),
            "no-store"
        );
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let bars = json.as_array().unwrap();
        assert_eq!(bars.len(), 1);
        let b = &bars[0];
        assert_eq!(b["time"], 1_700_000_000_000i64);
        assert_eq!(b["ticks"], 412);
        assert_eq!(
            b["up_ticks"].as_u64().unwrap()
                + b["down_ticks"].as_u64().unwrap()
                + b["flat_ticks"].as_u64().unwrap(),
            412
        );
        assert_eq!(b["closed"], false);
        assert_eq!(b["source"], "sifting");
    }

    #[test]
    fn malformed_origin_panics_at_boot() {
        // Spaces and control characters are illegal in a header value. Failing at
        // boot beats silently emitting an unparsable response header forever.
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let good = std::panic::catch_unwind(|| cors_layer("https://ok.example"));
        let bad = std::panic::catch_unwind(|| cors_layer("https://bad example\r\n"));
        std::panic::set_hook(quiet);

        assert!(good.is_ok());
        assert!(bad.is_err());
    }

    #[tokio::test]
    async fn ai_route_reports_todays_digest() {
        let res = send("GET", "/ai", Some(ALLOWED)).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        for key in [
            "day_start",
            "generated",
            "transcripts",
            "sentiments",
            "predictions",
            "health",
            "counts",
        ] {
            assert!(json.get(key).is_some(), "/ai missing {key}");
        }
        assert!(json["transcripts"].as_array().unwrap().is_empty());
    }

    #[test]
    fn envelope_normalization_wraps_loose_producer_payloads() {
        // Learn with top-level features/target (the shape Node3's handler
        // documents) is wrapped into the canonical data envelope.
        let f = normalize_envelope(
            r#"{"type":"learn","features":[1.5,-0.4,0.8,0.68],"target":1}"#,
            WsFrame::Learn {
                data: serde_json::Value::Null,
            },
        );
        match f {
            WsFrame::Learn { data } => {
                assert_eq!(data["features"][0], 1.5);
                assert_eq!(data["target"], 1);
            }
            other => panic!("wrong variant: {other:?}"),
        }

        // Audio carried under an `audio` key becomes data.data.
        let f = normalize_envelope(
            r#"{"type":"audio_chunk","audio":"AAAA"}"#,
            WsFrame::AudioChunk {
                data: serde_json::Value::Null,
            },
        );
        match f {
            WsFrame::AudioChunk { data } => assert_eq!(data["data"], "AAAA"),
            other => panic!("wrong variant: {other:?}"),
        }

        // Canonical payloads pass through untouched.
        let f = normalize_envelope(
            r#"{"type":"learn","data":{"features":[1.0],"target":0.0}}"#,
            WsFrame::Learn {
                data: serde_json::json!({"features": [1.0], "target": 0.0}),
            },
        );
        match f {
            WsFrame::Learn { data } => assert_eq!(data["target"], 0.0),
            other => panic!("wrong variant: {other:?}"),
        }
    }
}
