use std::sync::Arc;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, RwLock};
use tracing::info;

use crate::types::WsFrame;

#[derive(Clone)]
pub struct AppState {
    pub tx: broadcast::Sender<WsFrame>,
    pub subscriptions: Arc<DashMap<String, Vec<String>>>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
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

    // Shared topic list — written by the read task, read by the write loop
    let topics = Arc::new(RwLock::new(Vec::<String>::new()));

    // ---- Read task: handles inbound frames (subscribe, etc.)
    let topics_writer = topics.clone();
    let read_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(frame) = serde_json::from_str::<WsFrame>(&text) {
                    if let WsFrame::Subscribe { topics: t } = frame {
                        *topics_writer.write().await = t;
                    }
                }
            }
        }
    });

    // ---- Write loop: filters broadcast frames by subscribed topics
    while let Ok(frame) = rx.recv().await {
        // Determine the topic of the outbound frame
        let topic = match &frame {
            WsFrame::Levels { .. } => "levels",
            WsFrame::Bubbles { .. } => "bubbles",
            WsFrame::Trades { .. } => "trades",
            WsFrame::Sentiment { .. } => "sentiment",
            WsFrame::Calendar { .. } => "calendar",
            WsFrame::Learn { .. } => "learn",
            WsFrame::AudioChunk { .. } => "audio_chunk",
            _ => continue,
        };

        // Scope the read lock so it is dropped before the `.await` on send
        let allowed = {
            let current = topics.read().await;
            !current.is_empty() && current.iter().any(|t| t == topic)
        };
        if !allowed {
            continue;
        }

        let json = serde_json::to_string(&frame).unwrap_or_default();
        if sender.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }

    info!("WS session {} closed", session_id);
    read_task.abort();
}