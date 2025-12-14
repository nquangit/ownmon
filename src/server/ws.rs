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

    // Query database for today's stats (same as /api/stats)
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (sessions, _) = crate::store::DATABASE
        .as_ref()
        .and_then(|db| db.lock().ok())
        .and_then(|d| {
            d.query_sessions_flexible(Some(&today), None, None, None, 10000, 0, false)
                .ok()
        })
        .unwrap_or((vec![], 0));

    // Compute stats from database
    let mut total_keystrokes = 0u64;
    let mut total_clicks = 0u64;
    let mut total_duration = 0i64;
    let mut unique_apps = std::collections::HashSet::new();

    for session in &sessions {
        total_keystrokes += session.keystrokes as u64;
        total_clicks += session.clicks as u64;
        total_duration += session.duration_secs;
        unique_apps.insert(session.process_name.clone());
    }

    // Add current session
    if let Some(current) = &store.current_session {
        total_keystrokes += current.keystrokes;
        total_clicks += current.mouse_clicks;
        total_duration += current.duration_secs() as i64;
        unique_apps.insert(current.process_name.clone());
    }

    let message = serde_json::json!({
        "type": "initial_state",
        "data": {
            "session": current_session,
            "media": current_media,
            "stats": {
                "sessions": sessions.len() + if store.current_session.is_some() { 1 } else { 0 },
                "unique_apps": unique_apps.len(),
                "keystrokes": total_keystrokes,
                "clicks": total_clicks,
                "focus_time_secs": total_duration.max(0),
            }
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    serde_json::to_string(&message).ok()
}
