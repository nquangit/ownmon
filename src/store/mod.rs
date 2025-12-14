//! Data storage and aggregation module.
//!
//! Provides thread-safe storage for activity tracking data including
//! window sessions, input counts, and aggregated statistics.

pub mod activity_store;
pub mod aggregator;
pub mod types;

pub use activity_store::*;
pub use aggregator::*;
pub use types::*;

use crate::database::Database;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex, RwLock};

/// Global thread-safe activity store.
pub static ACTIVITY_STORE: Lazy<Arc<RwLock<ActivityStore>>> =
    Lazy::new(|| Arc::new(RwLock::new(ActivityStore::new())));

/// Global database connection (initialized on first use).
pub static DATABASE: Lazy<Option<Arc<Mutex<Database>>>> = Lazy::new(|| match Database::open() {
    Ok(db) => {
        tracing::info!("Database initialized successfully");
        Some(Arc::new(Mutex::new(db)))
    }
    Err(e) => {
        tracing::error!(
            ?e,
            "Failed to initialize database, running without persistence"
        );
        None
    }
});

/// Global WebSocket broadcast sender (set by HTTP server).
pub static BROADCAST_TX: once_cell::sync::OnceCell<tokio::sync::broadcast::Sender<String>> =
    once_cell::sync::OnceCell::new();

/// Sends an update to all connected WebSocket clients.
pub fn broadcast_update(update_type: &str, data: &impl serde::Serialize) {
    if let Some(tx) = BROADCAST_TX.get() {
        let message = serde_json::json!({
            "type": update_type,
            "data": data,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        if let Ok(json) = serde_json::to_string(&message) {
            let _ = tx.send(json);
        }
    }
}

/// Saves pending sessions and media to the database.
///
/// Call this periodically (e.g., every few seconds) and on shutdown.
/// This is crash-safe: each session is saved as soon as it completes.
pub fn save_pending_to_db() {
    let Some(db_arc) = DATABASE.as_ref() else {
        return;
    };

    let Ok(db) = db_arc.lock() else {
        return;
    };

    // Drain pending items from store
    let (sessions, media) = {
        let Ok(mut store) = ACTIVITY_STORE.write() else {
            return;
        };
        (store.drain_pending_sessions(), store.drain_pending_media())
    };

    // Save sessions
    for session in sessions {
        if let Some(end_time) = session.end_time {
            if let Err(e) = db.save_session(
                &session.process_name,
                &session.window_title,
                session.start_time,
                end_time,
                session.keystrokes,
                session.mouse_clicks,
                session.mouse_scrolls,
                session.is_idle,
            ) {
                tracing::warn!(?e, "Failed to save session to database");
            }
        }
    }

    // Save media
    for m in media {
        if let Some(end_time) = m.end_time {
            if let Err(e) = db.save_media(
                &m.media_info.title,
                &m.media_info.artist,
                &m.media_info.album,
                &m.media_info.source_app_id,
                m.start_time,
                end_time,
            ) {
                tracing::warn!(?e, "Failed to save media session to database");
            }
        }
    }
}

/// Finalizes and saves all current activity before shutdown.
pub fn finalize_and_save() {
    // Finalize current sessions
    if let Ok(mut store) = ACTIVITY_STORE.write() {
        store.finalize_current_session();
    }

    // Save all pending
    save_pending_to_db();

    tracing::info!("All pending data saved to database");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_store_accessible() {
        // Just verify we can access the global store
        let store = ACTIVITY_STORE.read().unwrap();
        let _ = store.session_count();
    }
}
