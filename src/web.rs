use std::sync::Arc;
use axum::{extract::{State, WebSocketUpgrade}, response::IntoResponse, routing::get, Router};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use crate::state::AppState;
use crate::types::Event;

pub fn router(st: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(dash))
        .route("/api/state", get(snapshot))
        .route("/ws", get(ws))
        .with_state(st)
}

async fn dash() -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
     include_str!("../dashboard/index.html"))
}

async fn snapshot(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    let tf = |t: &str| st.candles.read().unwrap().get(t).cloned().unwrap_or_default();
    axum::Json(serde_json::json!({
        "candles": { "H1": tf("H1"), "D": tf("D"), "M15": tf("M15") },
        "levels": *st.levels.read().unwrap(),
        "signals": *st.signals.read().unwrap(),
        "positions": *st.positions.read().unwrap(),
        "equity": *st.equity.read().unwrap(),
        "mode": st.cfg.exec.mode,
    }))
}

async fn ws(ws: WebSocketUpgrade, State(st): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(move |sock| pipe(sock, st))
}

async fn pipe(mut sock: WebSocket, st: Arc<AppState>) {
    let mut rx = st.tx.subscribe();
    if let Ok(s) = reqwest::get("http://127.0.0.1/api/state").await { let _ = s; } // noop guard removed below
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(e) => { if sock.send(Message::Text(serde_json::to_string(&e).unwrap())).await.is_err() { break; } }
                Err(_) => break,
            },
            m = sock.recv() => { if m.is_none() || matches!(m, Some(Err(_))) { break; } }
        }
    }
}
