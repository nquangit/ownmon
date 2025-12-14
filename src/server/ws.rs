//! WebSocket handler for real-time updates.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;

use crate::server::state::AppState;
use crate::store::ACTIVITY_STORE;

/// WebSocket upgrade handler.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handles an individual WebSocket connection.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Send current activity state on connection
    if let Some(initial_state) = get_current_state() {
        let _ = sender.send(Message::Text(initial_state)).await;
    }

    // Subscribe to broadcast channel
    let mut rx = state.subscribe();

    // Spawn task to receive from broadcast and send to WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages (for future use, e.g., commands)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Close(_) => break,
                Message::Ping(data) => {
                    // Pong is handled automatically by axum
                    let _ = data;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    tracing::debug!("WebSocket connection closed");
}

/// Gets the current activity state for initial WebSocket message.
fn get_current_state() -> Option<String> {
    let store = ACTIVITY_STORE.read().ok()?;

    let current_session = store.current_session.as_ref().map(|s| {
        serde_json::json!({
            "process_name": s.process_name,
            "window_title": s.window_title,
            "start_time": s.start_time.to_rfc3339(),
        })
    });

    let current_media = store.current_media.as_ref().map(|m| {
        serde_json::json!({
            "title": m.media_info.title,
            "artist": m.media_info.artist,
            "album": m.media_info.album,
            "is_playing": m.media_info.is_playing(),
            "start_time": m.start_time.to_rfc3339(),
        })
    });

    let summary = store.get_daily_summary();

    let message = serde_json::json!({
        "type": "initial_state",
        "data": {
            "session": current_session,
            "media": current_media,
            "stats": {
                "sessions": summary.session_count,
                "keystrokes": summary.total_keystrokes,
                "clicks": summary.total_clicks,
                "focus_time_secs": summary.total_focus_time_secs,
            }
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    serde_json::to_string(&message).ok()
}
