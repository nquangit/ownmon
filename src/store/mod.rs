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

use crate::crypto::{hash_and_sign_session, KeyManager};
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

/// Global key manager for integrity signing (initialized on first use).
pub static KEY_MANAGER: Lazy<Option<KeyManager>> = Lazy::new(|| match KeyManager::init() {
    Ok(km) => Some(km),
    Err(e) => {
        tracing::error!(
            ?e,
            "Failed to initialize key manager, running without integrity"
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

    // Get key manager for signing
    let key_manager = KEY_MANAGER.as_ref();

    // Get last session hash for chaining
    let mut prev_hash = db.get_last_session_hash().ok().flatten();

    // Save sessions with integrity
    for session in sessions {
        if let Some(end_time) = session.end_time {
            let (hash, signature, used_prev_hash) = if let Some(km) = key_manager {
                let start_str = session.start_time.to_rfc3339();
                let end_str = end_time.to_rfc3339();
                let (h, s) = hash_and_sign_session(
                    km.signing_key(),
                    &session.process_name,
                    &session.window_title,
                    &start_str,
                    &end_str,
                    session.keystrokes,
                    session.mouse_clicks,
                    session.mouse_scrolls,
                    prev_hash.as_deref(),
                );
                (Some(h), Some(s), prev_hash.take())
            } else {
                (None, None, None)
            };

            if let Err(e) = db.save_session(
                &session.process_name,
                &session.window_title,
                session.start_time,
                end_time,
                session.keystrokes,
                session.mouse_clicks,
                session.mouse_scrolls,
                session.is_idle,
                hash.as_deref(),
                signature.as_deref(),
                used_prev_hash.as_deref(),
            ) {
                tracing::warn!(?e, "Failed to save session to database");
            } else {
                // Update prev_hash for next session in chain
                prev_hash = hash;
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

    // Compute daily integrity for today
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if let Err(e) = compute_daily_integrity(&today) {
        tracing::warn!(error = %e, "Failed to compute daily integrity on shutdown");
    }

    tracing::info!("All pending data saved to database");
}

/// Computes and saves daily integrity (Merkle root) for a given date.
/// Call this at end of day or on startup for previous incomplete days.
pub fn compute_daily_integrity(date: &str) -> Result<(), String> {
    use crate::crypto::{build_merkle_root, sign_hash};

    let Some(db_arc) = DATABASE.as_ref() else {
        return Err("Database not initialized".to_string());
    };
    let db = db_arc.lock().map_err(|e| e.to_string())?;

    let Some(km) = KEY_MANAGER.as_ref() else {
        return Err("Key manager not initialized".to_string());
    };

    // Get all session hashes for the date
    let hashes = db
        .get_session_hashes_for_date(date)
        .map_err(|e| e.to_string())?;

    if hashes.is_empty() {
        tracing::debug!(date, "No sessions to compute integrity for");
        return Ok(());
    }

    // Build Merkle root
    let merkle_root =
        build_merkle_root(&hashes).ok_or_else(|| "Failed to build Merkle root".to_string())?;

    // Get previous day's root for chaining
    let prev_day_root = db.get_previous_day_root(date).map_err(|e| e.to_string())?;

    // Create data to sign: merkle_root + prev_day_root + date
    let sign_data = format!(
        "{}|{}|{}",
        merkle_root,
        prev_day_root.as_deref().unwrap_or("genesis"),
        date
    );
    let signature = sign_hash(&sign_data, km.signing_key());

    // Save to database
    db.save_daily_integrity(
        date,
        &merkle_root,
        prev_day_root.as_deref(),
        hashes.len() as u32,
        &signature,
    )
    .map_err(|e| e.to_string())?;

    tracing::info!(
        date,
        session_count = hashes.len(),
        "Daily integrity computed and saved"
    );

    Ok(())
}

/// Checks for and computes daily integrity for any incomplete previous days.
/// Call this on application startup.
pub fn check_and_compute_missing_integrity() {
    let Some(db_arc) = DATABASE.as_ref() else {
        return;
    };
    let Ok(db) = db_arc.lock() else {
        return;
    };

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Get dates with sessions but no integrity record
    let missing_dates = match db.get_dates_missing_integrity(&today) {
        Ok(dates) => dates,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to query missing integrity dates");
            return;
        }
    };

    if missing_dates.is_empty() {
        tracing::debug!("No missing daily integrity records");
        return;
    }

    tracing::info!(
        count = missing_dates.len(),
        "Found days with missing integrity, computing..."
    );

    // Need to drop db lock before calling compute_daily_integrity (which acquires its own)
    drop(db);

    for date in missing_dates {
        if let Err(e) = compute_daily_integrity(&date) {
            tracing::warn!(date = %date, error = %e, "Failed to compute missing daily integrity");
        }
    }
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
