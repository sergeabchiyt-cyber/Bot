use std::sync::Arc;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, RwLock};
use tracing::info;

use crate::config::Config;
use crate::status::FeedStatus;
use crate::types::{VpLevels, VpCandle, WsFrame};
use crate::volume_profile::VolumeProfileEngine;

#[derive(Clone)]
pub struct AppState {
    pub tx: broadcast::Sender<WsFrame>,
    pub subscriptions: Arc<DashMap<String, Vec<String>>>,
    pub cached_levels: Arc<RwLock<Vec<VpLevels>>>,
    pub cached_candles: Arc<RwLock<Vec<VpCandle>>>,
    pub cached_calendar: Arc<RwLock<serde_json::Value>>,
    pub vp: Arc<RwLock<VolumeProfileEngine>>,
    pub status: FeedStatus,
    pub config: Config,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/levels", get(levels_snapshot))
        .route("/candles", get(candles_snapshot))
        .route("/calendar", get(calendar_snapshot))
        .route("/status", get(status_snapshot))
        .route("/ws", get(ws_handler))
        .with_state(state)
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

async fn calendar_snapshot(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.cached_calendar.read().await.clone())
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
                            if let WsFrame::Subscribe { topics: t } = frame {
                                info!("WS session {} subscribed to {:?}", session_id, t);
                                topics = t;
                                subscribed = true;

                                // Replay cached state so new clients fill
                                // instantly (levels + recent candles + status).
                                if topics.iter().any(|x| x == "levels") {
                                    let cached = state.cached_levels.read().await;
                                    for lvl in cached.iter() {
                                        let frame = WsFrame::Levels { data: lvl.clone() };
                                        let json = serde_json::to_string(&frame).unwrap_or_default();
                                        if sender.send(Message::Text(json.into())).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                                if topics.iter().any(|x| x == "candle") {
                                    let cached = state.cached_candles.read().await;
                                    for c in cached.iter() {
                                        let frame = WsFrame::Candle { data: c.clone() };
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
                            WsFrame::Bubbles { .. } => "bubbles",
                            WsFrame::Trades { .. } => "trades",
                            WsFrame::Calendar { .. } => "calendar",
                            WsFrame::Status { .. } => "status",
                            WsFrame::Heartbeat => {
                                // keepalive for proxies — always forwarded
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
