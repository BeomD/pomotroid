//! WebSocket endpoint. Each connected client receives every broadcast frame
//! (`timer:tick`, `timer:round-change`, `settings:changed`, …). Commands are
//! sent over HTTP, so this socket is server → client only.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};

use crate::state::AppState;

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    log::info!("[ws] client connected ({} subscribers)", state.events.receiver_count());
    let mut rx = state.events.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Forward broadcast frames to this client.
    let mut send_task = tokio::spawn(async move {
        while let Ok(frame) = rx.recv().await {
            if sender.send(Message::Text(frame.into())).await.is_err() {
                break;
            }
        }
    });

    // Drain incoming frames (pings / close). We ignore command payloads.
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(_msg)) = receiver.next().await {}
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
