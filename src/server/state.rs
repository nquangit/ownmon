//! Shared application state for the HTTP server.

use tokio::sync::broadcast;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Broadcast channel for WebSocket updates.
    pub broadcast_tx: broadcast::Sender<String>,
}

impl AppState {
    /// Creates new app state with the given broadcast sender.
    pub fn new(broadcast_tx: broadcast::Sender<String>) -> Self {
        Self { broadcast_tx }
    }

    /// Subscribe to the broadcast channel.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcast_tx.subscribe()
    }
}
