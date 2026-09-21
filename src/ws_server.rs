use std::sync::Arc;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, RwLock};
use tracing::info;

use crate::types::{VpLevels, WsFrame};
use crate::volume_profile::VolumeProfileEngine;

#[derive(Clone)]
pub struct AppState {
    pub tx: broadcast::Sender<WsFrame>,
    pub subscriptions: Arc<DashMap<String, Vec<String>>>,
    pub cached_levels: Arc<RwLock<Vec<VpLevels>>>,
    pub vp: Arc<RwLock<VolumeProfileEngine>>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/levels", get(levels_snapshot))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn levels_snapshot(State(state): State<AppState>) -> Json<Vec<VpLevels>> {
    let vp = state.vp.read().await;
    Json(vp.all_levels_full())
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

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                            if let WsFrame::Subscribe { topics: t } = frame {
                                info!("WS session {} subscribed to {:?}", session_id, t);
                                topics = t;
                                subscribed = true;

                                // Replay cached levels (PW/PS only)
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
                            WsFrame::Bubbles { .. } => "bubbles",
                            WsFrame::Trades { .. } => "trades",
                            WsFrame::Sentiment { .. } => "sentiment",
                            WsFrame::Transcript { .. } => "transcript",
                            WsFrame::Calendar { .. } => "calendar",
                            WsFrame::Learn { .. } => "learn",
                            WsFrame::AudioChunk { .. } => "audio_chunk",
                            _ => continue,
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