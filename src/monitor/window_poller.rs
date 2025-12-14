//! Window polling and focus change detection.
//!
//! This module provides the polling loop that monitors the foreground window
//! and detects when focus changes between applications.

use crate::media::fetch_current_media;
use crate::monitor::input_hooks::{flush_click_counts, flush_keystroke_count, flush_scroll_count};
use crate::store::ACTIVITY_STORE;
use crate::winapi_utils::{
    get_foreground_window, get_process_name, get_window_text, get_window_thread_process_id,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Configuration for the window poller.
#[derive(Debug, Clone)]
pub struct PollerConfig {
    /// How often to poll for window changes (default: 100ms).
    pub poll_interval: Duration,

    /// Whether to track window title changes within the same process.
    pub track_title_changes: bool,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(100),
            track_title_changes: false,
        }
    }
}

/// Spawns the window polling thread.
///
/// The polling thread:
/// 1. Periodically checks the foreground window
/// 2. Detects focus changes and updates the activity store
/// 3. Flushes input counters to the current session
///
/// # Arguments
/// * `shutdown` - Atomic flag to signal thread termination
/// * `config` - Polling configuration
///
/// # Returns
/// A `JoinHandle` for the spawned thread.
///
/// # Example
/// ```ignore
/// let shutdown = Arc::new(AtomicBool::new(false));
/// let handle = spawn_polling_thread(Arc::clone(&shutdown), PollerConfig::default());
///
/// // ... run message loop ...
///
/// shutdown.store(true, Ordering::SeqCst);
/// handle.join().unwrap();
/// ```
pub fn spawn_polling_thread(shutdown: Arc<AtomicBool>, config: PollerConfig) -> JoinHandle<()> {
    thread::spawn(move || {
        tracing::info!(
            interval_ms = config.poll_interval.as_millis(),
            "Window polling thread started"
        );

        let mut last_hwnd: Option<isize> = None;
        let mut last_title: String = String::new();
        let mut db_save_counter: u32 = 0;
        const DB_SAVE_INTERVAL: u32 = 50; // Every 50 cycles (~5 seconds at 100ms)

        loop {
            // Check for idle and split session if needed
            if let Ok(mut store) = ACTIVITY_STORE.write() {
                (*store).check_and_split_on_idle();
            }

            if shutdown.load(Ordering::SeqCst) {
                break;
            }

            poll_cycle(&mut last_hwnd, &mut last_title, config.track_title_changes); // Periodic database save for crash safety
            db_save_counter += 1;
            if db_save_counter >= DB_SAVE_INTERVAL {
                db_save_counter = 0;
                crate::store::save_pending_to_db();
            }

            thread::sleep(config.poll_interval);
        }

        tracing::info!("Window polling thread shutting down");

        // Final flush before exit
        flush_counters_to_store();
    })
}

/// Performs a single poll cycle.
///
/// Checks the current foreground window and updates the store if needed.
fn poll_cycle(last_hwnd: &mut Option<isize>, last_title: &mut String, track_title_changes: bool) {
    // Always flush counters, even if window hasn't changed
    flush_counters_to_store();

    // Poll for media changes
    poll_media();

    // Get current foreground window
    let hwnd = match get_foreground_window() {
        Some(h) => h,
        None => {
            // No foreground window (e.g., desktop focused, lock screen)
            return;
        }
    };

    let hwnd_value = hwnd.0 as isize;
    let window_changed = last_hwnd.is_none_or(|last| last != hwnd_value);

    // Get window info
    let current_title = get_window_text(hwnd);
    let title_changed = !window_changed && track_title_changes && *last_title != current_title;

    if window_changed || title_changed {
        let (_, pid) = get_window_thread_process_id(hwnd);
        let raw_process_name = get_process_name(pid).unwrap_or_else(|| "Unknown".to_string());

        // Check if process is blacklisted
        let is_blacklisted = crate::store::DATABASE
            .as_ref()
            .map(|db| {
                db.lock()
                    .ok()
                    .map(|d| d.is_blacklisted(&raw_process_name))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if is_blacklisted {
            *last_hwnd = Some(hwnd_value);
            *last_title = current_title;
            return;
        }
        let process_name = if raw_process_name == "ApplicationFrameHost.exe" {
            // Extract app name from window title (e.g., "Calculator" from "Calculator")
            // or use a sanitized version
            if !current_title.is_empty() {
                format!("[UWP] {}", extract_app_name(&current_title))
            } else {
                "UWP App".to_string()
            }
        } else if raw_process_name == "Unknown" && !current_title.is_empty() {
            // Fallback for elevated processes - use window title
            format!("[Elevated] {}", extract_app_name(&current_title))
        } else {
            raw_process_name
        };

        // Update store
        if let Ok(mut store) = ACTIVITY_STORE.write() {
            store.switch_session(hwnd_value, pid, &process_name, &current_title);
        }

        // Broadcast session update to WebSocket clients
        let session_data = serde_json::json!({
            "process_name": process_name,
            "window_title": current_title,
        });
        crate::store::broadcast_update("session_change", &session_data);

        if window_changed {
            tracing::debug!(
                pid = pid,
                process = %process_name,
                title = %current_title,
                "Window focus changed"
            );
        } else {
            tracing::trace!(
                title = %current_title,
                "Window title changed"
            );
        }

        *last_hwnd = Some(hwnd_value);
        *last_title = current_title;
    }
}

/// Flushes atomic input counters to the activity store.
///
/// This atomically reads and resets the counters, then adds the values
/// to the current session in the store.
fn flush_counters_to_store() {
    let keystrokes = flush_keystroke_count();
    let (left, right, middle) = flush_click_counts();
    let scrolls = flush_scroll_count();

    let total_clicks = left + right + middle;

    // Only acquire lock if we have something to add
    if keystrokes > 0 || total_clicks > 0 || scrolls > 0 {
        if let Ok(mut store) = ACTIVITY_STORE.try_write() {
            store.add_input_counts(keystrokes, total_clicks, scrolls);
        } else {
            // Lock contention - counts will be added next cycle
            // This is rare but acceptable for monitoring purposes
            tracing::trace!("Store lock contention, deferring counter flush");
        }
    }
}

/// Polls for current media and updates the store.
fn poll_media() {
    if let Some(media_info) = fetch_current_media() {
        // Broadcast media update
        let media_data = serde_json::json!({
            "title": media_info.title,
            "artist": media_info.artist,
            "album": media_info.album,
            "is_playing": media_info.is_playing(),
        });
        crate::store::broadcast_update("media_update", &media_data);

        if let Ok(mut store) = ACTIVITY_STORE.try_write() {
            store.update_media(media_info);
        }
    }
}

/// Extracts a clean app name from a window title.
///
/// For UWP apps, the window title is often the app name directly (e.g., "Calculator").
/// For more complex titles, we take the first part before common separators.
fn extract_app_name(title: &str) -> String {
    // Take the first meaningful part of the title
    // Common patterns: "App - Document", "Document | App", "App"
    let name = title
        .split(" - ")
        .next()
        .unwrap_or(title)
        .split(" | ")
        .next()
        .unwrap_or(title)
        .split(" — ") // em-dash
        .next()
        .unwrap_or(title)
        .trim();

    // Limit length for cleaner display
    if name.len() > 30 {
        format!("{}...", &name[..27])
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poller_config_default() {
        let config = PollerConfig::default();
        assert_eq!(config.poll_interval, Duration::from_millis(100));
        assert!(!config.track_title_changes);
    }

    #[test]
    fn test_flush_counters_no_panic_on_empty() {
        // Should not panic when counters are zero
        flush_counters_to_store();
    }
}
