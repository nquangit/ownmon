//! Activity store for tracking window sessions and input counts.
//!
//! Provides the main data store that holds current and completed sessions,
//! along with methods for session management and aggregation.

use super::types::{ApplicationStats, DailySummary, WindowSession};
use crate::media::MediaSession;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// The main store for all activity data.
///
/// This struct should be wrapped in `Arc<RwLock<ActivityStore>>` for
/// thread-safe access from multiple threads (hooks, polling, HTTP server).
#[derive(Debug, Default)]
pub struct ActivityStore {
    /// The currently active window session, if any.
    pub current_session: Option<WindowSession>,

    /// All completed sessions (finalized when window focus changed).
    pub completed_sessions: Vec<WindowSession>,

    /// Cached aggregated statistics per application.
    /// Recomputed when needed.
    app_aggregates: HashMap<String, ApplicationStats>,

    /// Timestamp of the last poll cycle.
    pub last_poll_time: Option<DateTime<Utc>>,

    // === Media Tracking ===
    /// The currently playing media session, if any.
    pub current_media: Option<MediaSession>,

    /// History of played media sessions.
    pub media_history: Vec<MediaSession>,

    // === AFK Tracking ===
    /// Timestamp of the last keyboard/mouse input
    pub last_input_time: DateTime<Utc>,

    // === Database Queue ===
    /// Sessions pending save to database (drained periodically).
    pending_sessions: Vec<WindowSession>,

    /// Media sessions pending save to database.
    pending_media: Vec<MediaSession>,
}

impl ActivityStore {
    /// Creates a new empty activity store.
    pub fn new() -> Self {
        Self {
            last_input_time: Utc::now(),
            ..Default::default()
        }
    }

    /// Switches to a new window session.
    ///
    /// This will:
    /// 1. Finalize the current session (if any)
    /// 2. Update aggregated stats
    /// 3. Move the old session to completed_sessions
    /// 4. Create a new current session
    ///
    /// # Arguments
    /// * `hwnd` - Window handle as isize
    /// * `pid` - Process ID
    /// * `process_name` - Name of the executable
    /// * `window_title` - Title of the window
    pub fn switch_session(
        &mut self,
        hwnd: isize,
        pid: u32,
        process_name: &str,
        window_title: &str,
    ) {
        // 1. Finalize current session if exists
        if let Some(mut old_session) = self.current_session.take() {
            old_session.finalize();

            // Check minimum duration (get from DB or default to 3 seconds)
            let min_duration = crate::store::DATABASE
                .as_ref()
                .and_then(|db| db.lock().ok())
                .and_then(|d| d.get_config("min_session_duration_secs").ok().flatten())
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(3);

            let duration = old_session.duration_secs();
            if duration >= min_duration {
                // 2. Update aggregates
                self.update_aggregates(&old_session);

                // 3. Queue for database save
                self.pending_sessions.push(old_session.clone());

                // 4. Store completed session
                self.completed_sessions.push(old_session);
            } else {
                tracing::trace!(
                    duration_secs = duration,
                    min_duration_secs = min_duration,
                    process = %old_session.process_name,
                    "Skipping short session"
                );
            }
        }

        // 4. Create new session
        self.current_session = Some(WindowSession::new(
            hwnd,
            pid,
            process_name.to_string(),
            window_title.to_string(),
        ));
        // Reset last_input_time to now (start of new session)
        self.last_input_time = Utc::now();
        self.last_poll_time = Some(Utc::now());
    }

    /// Updates the aggregated stats with a completed session.
    fn update_aggregates(&mut self, session: &WindowSession) {
        let stats = self
            .app_aggregates
            .entry(session.process_name.clone())
            .or_insert_with(|| ApplicationStats::new(session.process_name.clone()));

        stats.add_session(session);
    }

    /// Gets the window handle of the current session.
    ///
    /// Returns `None` if there is no active session.
    pub fn current_window_handle(&self) -> Option<isize> {
        self.current_session.as_ref().map(|s| s.window_handle)
    }

    /// Adds input counts to the current session (bulk update).
    ///
    /// This is more efficient than calling increment methods repeatedly,
    /// especially when flushing atomic counters from hooks.
    pub fn add_input_counts(&mut self, keystrokes: u64, clicks: u64, scrolls: u64) {
        // Check if user is resuming from idle (was inactive > threshold, now active)
        if keystrokes > 0 || clicks > 0 || scrolls > 0 {
            let now = Utc::now();
            let time_since_last_input = (now - self.last_input_time).num_seconds();

            // Get AFK threshold from config (default 300 seconds = 5 minutes)
            let afk_threshold = crate::store::DATABASE
                .as_ref()
                .and_then(|db| db.lock().ok())
                .and_then(|d| d.get_config("afk_threshold_secs").ok().flatten())
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(300);

            // If user was idle > threshold and now returning
            if time_since_last_input > afk_threshold {
                if let Some(session) = &self.current_session {
                    // Only split if session is not already marked as idle
                    if !session.is_idle {
                        self.split_on_resume_from_idle();
                    }
                }
            }

            self.last_input_time = now;
        }

        if let Some(session) = &mut self.current_session {
            session.keystrokes += keystrokes;
            session.mouse_clicks += clicks;
            session.mouse_scrolls += scrolls;
        }
    }

    /// Helper to save session if it meets minimum duration requirement.
    fn save_session_if_valid(&mut self, session: WindowSession) {
        let min_duration = crate::store::DATABASE
            .as_ref()
            .and_then(|db| db.lock().ok())
            .and_then(|d| d.get_config("min_session_duration_secs").ok().flatten())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(3);

        let duration = session.duration_secs();
        if duration >= min_duration {
            self.update_aggregates(&session);
            self.pending_sessions.push(session.clone());
            self.completed_sessions.push(session);
        } else {
            tracing::trace!(
                duration_secs = duration,
                min_duration_secs = min_duration,
                process = %session.process_name,
                "Skipping short split session"
            );
        }
    }

    /// Splits current session when user resumes from idle.
    ///
    /// Creates idle session for the AFK period and new active session for resumed activity.
    fn split_on_resume_from_idle(&mut self) {
        let session = self.current_session.take().unwrap();

        // Ensure last_input_time is not before session start
        let effective_last_input = std::cmp::max(self.last_input_time, session.start_time);

        // Check if session had any activity before going idle
        let has_activity =
            session.keystrokes > 0 || session.mouse_clicks > 0 || session.mouse_scrolls > 0;

        if has_activity {
            // Split into active + idle
            let process_name = session.process_name.clone();

            // 1. Active session: start to last_input (with all activity)
            let mut active_session = session.clone();
            active_session.end_time = Some(effective_last_input);
            active_session.is_idle = false;

            self.save_session_if_valid(active_session);

            // 2. Idle session: last_input to now (zero activity)
            let mut idle_session = session.clone();
            idle_session.start_time = effective_last_input;
            idle_session.end_time = Some(Utc::now());
            idle_session.keystrokes = 0;
            idle_session.mouse_clicks = 0;
            idle_session.mouse_scrolls = 0;
            idle_session.is_idle = true;

            self.save_session_if_valid(idle_session);

            // 3. Create new active session for resumed activity (same window)
            self.current_session = Some(WindowSession::new(
                session.window_handle,
                session.process_id,
                session.process_name,
                session.window_title,
            ));

            tracing::info!(
                process = %process_name,
                "User resumed from idle - split into active + idle + new active"
            );
        } else {
            // No activity before idle - just mark as idle and create new
            let mut idle_session = session;
            idle_session.end_time = Some(Utc::now());
            idle_session.is_idle = true;

            let idle_clone = idle_session.clone();
            self.save_session_if_valid(idle_session);

            // Create new active session
            self.current_session = Some(WindowSession::new(
                idle_clone.window_handle,
                idle_clone.process_id,
                idle_clone.process_name,
                idle_clone.window_title,
            ));
        }
    }

    /// Checks if user is idle and splits the current session if needed.
    ///
    /// Should be called periodically (e.g., from poller loop).
    /// If idle for >5 minutes, finalizes current session with idle time set.
    pub fn check_and_split_on_idle(&mut self) {
        // Get AFK threshold from config (default 300 seconds = 5 minutes)
        let afk_threshold = crate::store::DATABASE
            .as_ref()
            .and_then(|db| db.lock().ok())
            .and_then(|d| d.get_config("afk_threshold_secs").ok().flatten())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(300);

        if self.current_session.is_none() {
            return; // No active session
        }

        let time_since_last_input = (Utc::now() - self.last_input_time).num_seconds();

        if time_since_last_input > afk_threshold {
            let session = self.current_session.as_mut().unwrap();

            // Check if already marked as idle
            if session.is_idle {
                // Update end_time to capture continued idle period
                session.end_time = Some(Utc::now());
                return;
            }

            // First time going idle - need to split
            let mut session = self.current_session.take().unwrap();

            // Ensure last_input_time is not before session start
            let effective_last_input = std::cmp::max(self.last_input_time, session.start_time);

            // Check if session had any activity
            let has_activity =
                session.keystrokes > 0 || session.mouse_clicks > 0 || session.mouse_scrolls > 0;

            if has_activity {
                // Split into TWO sessions: active part + idle part

                // Save process name before moving session
                let process_name = session.process_name.clone();

                // 1. Active session: start_time to last_input_time (with all activity)
                let mut active_session = session.clone();
                active_session.end_time = Some(effective_last_input);
                active_session.is_idle = false;

                self.save_session_if_valid(active_session);

                // 2. Idle session: last_input_time to now (zero activity, is_idle=true)
                let mut idle_session = session;
                idle_session.start_time = effective_last_input;
                idle_session.end_time = Some(Utc::now());
                idle_session.keystrokes = 0;
                idle_session.mouse_clicks = 0;
                idle_session.mouse_scrolls = 0;
                idle_session.is_idle = true;

                tracing::info!(
                    process = %process_name,
                    "Split session into active + idle parts"
                );

                // Keep idle session as current to track continued idle time
                self.current_session = Some(idle_session);
            } else {
                // No activity - entire session is idle
                session.end_time = Some(Utc::now());
                session.is_idle = true;

                tracing::debug!("Session marked as fully idle (no activity)");

                // Keep as current session to track continued idle time
                self.current_session = Some(session);
            }
        }
    }

    /// Returns the total number of completed sessions.
    pub fn session_count(&self) -> usize {
        self.completed_sessions.len()
    }

    /// Computes aggregated statistics for all applications.
    pub fn compute_application_stats(&self) -> HashMap<String, ApplicationStats> {
        let mut stats: HashMap<String, ApplicationStats> = HashMap::new();

        for session in &self.completed_sessions {
            let entry = stats
                .entry(session.process_name.clone())
                .or_insert_with(|| ApplicationStats::new(session.process_name.clone()));

            entry.add_session(session);
        }

        // Include current session if active
        if let Some(session) = &self.current_session {
            let entry = stats
                .entry(session.process_name.clone())
                .or_insert_with(|| ApplicationStats::new(session.process_name.clone()));

            entry.total_keystrokes += session.keystrokes;
            entry.total_clicks += session.mouse_clicks;
            entry.total_focus_duration_secs += session.duration_secs() as u64;
            // Don't increment session_count for current session
        }

        stats
    }

    /// Gets a summary of today's activity.
    pub fn get_daily_summary(&self) -> DailySummary {
        let stats = self.compute_application_stats();

        DailySummary {
            total_keystrokes: stats.values().map(|s| s.total_keystrokes).sum(),
            total_clicks: stats.values().map(|s| s.total_clicks).sum(),
            total_focus_time_secs: stats.values().map(|s| s.total_focus_duration_secs).sum(),
            app_count: stats.len() as u32,
            session_count: self.completed_sessions.len() as u32
                + if self.current_session.is_some() { 1 } else { 0 },
        }
    }

    /// Serializes the store to JSON.
    pub fn to_json(&self) -> String {
        let data = serde_json::json!({
            "current_session": self.current_session,
            "completed_sessions_count": self.completed_sessions.len(),
            "recent_sessions": self.completed_sessions.iter().rev().take(5).collect::<Vec<_>>(),
            "daily_summary": self.get_daily_summary(),
            "current_media": self.current_media,
            "media_history_count": self.media_history.len(),
        });

        serde_json::to_string_pretty(&data).unwrap_or_else(|_| "{}".to_string())
    }

    /// Prunes old sessions to prevent unbounded memory growth.
    ///
    /// # Arguments
    /// * `max_sessions` - Maximum number of sessions to keep
    pub fn prune_old_sessions(&mut self, max_sessions: usize) {
        if self.completed_sessions.len() > max_sessions {
            let drain_count = self.completed_sessions.len() - max_sessions;
            self.completed_sessions.drain(0..drain_count);
        }

        // Also prune media history
        if self.media_history.len() > max_sessions {
            let drain_count = self.media_history.len() - max_sessions;
            self.media_history.drain(0..drain_count);
        }
    }

    // === Media Tracking Methods ===

    /// Updates the currently playing media.
    ///
    /// If the media has changed (different title/artist), the old session
    /// is finalized and a new one is created.
    pub fn update_media(&mut self, media_info: crate::media::MediaInfo) {
        // Check if media has changed
        let media_changed = match &self.current_media {
            Some(current) => !current.is_same_media(&media_info),
            None => true,
        };

        if media_changed && media_info.is_playing() {
            // Finalize current media session if exists
            if let Some(mut old_media) = self.current_media.take() {
                old_media.finalize();
                self.media_history.push(old_media);
            }

            // Start new media session
            self.current_media = Some(MediaSession::new(media_info));

            if let Some(ref media) = self.current_media {
                tracing::debug!(
                    title = %media.media_info.title,
                    artist = %media.media_info.artist,
                    app = %media.media_info.source_app_id,
                    "New media detected"
                );
            }
        } else if !media_info.is_playing() && self.current_media.is_some() {
            // Media stopped/paused, finalize current session
            if let Some(mut old_media) = self.current_media.take() {
                old_media.finalize();
                self.pending_media.push(old_media.clone());
                self.media_history.push(old_media);
                tracing::debug!("Media playback stopped");
            }
        }
    }

    /// Gets a summary of media listening history.
    pub fn get_media_summary(&self) -> Vec<&MediaSession> {
        self.media_history.iter().rev().take(10).collect()
    }

    /// Returns total media listening time in seconds.
    pub fn total_media_time_secs(&self) -> i64 {
        let history_time: i64 = self.media_history.iter().map(|m| m.duration_secs()).sum();
        let current_time = self.current_media.as_ref().map_or(0, |m| m.duration_secs());
        history_time + current_time
    }

    // === Database Queue Methods ===

    /// Drains and returns pending window sessions for database save.
    pub fn drain_pending_sessions(&mut self) -> Vec<WindowSession> {
        std::mem::take(&mut self.pending_sessions)
    }

    /// Drains and returns pending media sessions for database save.
    pub fn drain_pending_media(&mut self) -> Vec<MediaSession> {
        std::mem::take(&mut self.pending_media)
    }

    /// Returns true if there are pending items to save.
    pub fn has_pending_saves(&self) -> bool {
        !self.pending_sessions.is_empty() || !self.pending_media.is_empty()
    }

    /// Queues the current session for save (call before shutdown).
    pub fn finalize_current_session(&mut self) {
        if let Some(mut session) = self.current_session.take() {
            session.finalize();
            self.pending_sessions.push(session.clone());
            self.completed_sessions.push(session);
        }
        if let Some(mut media) = self.current_media.take() {
            media.finalize();
            self.pending_media.push(media.clone());
            self.media_history.push(media);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_store_is_empty() {
        let store = ActivityStore::new();
        assert!(store.current_session.is_none());
        assert!(store.completed_sessions.is_empty());
    }

    #[test]
    fn test_switch_session_creates_new() {
        let mut store = ActivityStore::new();
        store.switch_session(1, 100, "chrome.exe", "Google");

        assert!(store.current_session.is_some());
        let session = store.current_session.as_ref().unwrap();
        assert_eq!(session.process_name, "chrome.exe");
        assert_eq!(session.window_title, "Google");
    }

    #[test]
    fn test_switch_session_finalizes_old() {
        let mut store = ActivityStore::new();

        store.switch_session(2, 200, "notepad.exe", "Untitled");
        store.add_input_counts(2, 0, 0);

        // Sleep to ensure session duration >= 3s (min_session_duration)
        std::thread::sleep(std::time::Duration::from_secs(3));

        store.switch_session(3, 300, "chrome.exe", "Google");

        // Old session should be completed
        assert_eq!(store.completed_sessions.len(), 1);
        assert_eq!(store.completed_sessions[0].keystrokes, 2);
        assert!(store.completed_sessions[0].end_time.is_some());

        // New session should be active
        assert_eq!(
            store.current_session.as_ref().unwrap().process_name,
            "chrome.exe"
        );
        assert_eq!(
            store.current_session.as_ref().unwrap().window_title,
            "Google"
        );
    }

    #[test]
    fn test_add_input_counts() {
        let mut store = ActivityStore::new();
        store.switch_session(1, 100, "test.exe", "Test");

        store.add_input_counts(100, 50, 25);

        let session = store.current_session.as_ref().unwrap();
        assert_eq!(session.keystrokes, 100);
        assert_eq!(session.mouse_clicks, 50);
        assert_eq!(session.mouse_scrolls, 25);
    }

    #[test]
    fn test_current_window_handle() {
        let mut store = ActivityStore::new();
        assert!(store.current_window_handle().is_none());

        store.switch_session(12345, 100, "test.exe", "Test");
        assert_eq!(store.current_window_handle(), Some(12345));
    }

    #[test]
    fn test_compute_application_stats() {
        let mut store = ActivityStore::new();

        store.switch_session(1, 100, "chrome.exe", "Tab 1");
        store.add_input_counts(10, 5, 0);
        std::thread::sleep(std::time::Duration::from_secs(3));

        store.switch_session(2, 100, "chrome.exe", "Tab 2");
        store.add_input_counts(20, 10, 0);
        std::thread::sleep(std::time::Duration::from_secs(3));

        store.switch_session(3, 200, "code.exe", "Editor");

        let stats = store.compute_application_stats();

        assert!(stats.contains_key("chrome.exe"));
        assert!(stats.contains_key("code.exe"));

        let chrome_stats = &stats["chrome.exe"];
        assert_eq!(chrome_stats.total_keystrokes, 30); // 10 + 20
        assert_eq!(chrome_stats.total_clicks, 15); // 5 + 10
        assert_eq!(chrome_stats.session_count, 2);
    }

    #[test]
    fn test_to_json() {
        let mut store = ActivityStore::new();
        store.switch_session(1, 100, "test.exe", "Test");

        let json = store.to_json();
        assert!(json.contains("test.exe"));
        assert!(json.contains("daily_summary"));
    }

    #[test]
    fn test_prune_old_sessions() {
        let mut store = ActivityStore::new();

        for i in 0..10 {
            store.switch_session(i, i as u32, &format!("app{}.exe", i), "Window");
            store.add_input_counts(1, 0, 0); // Add some activity
            std::thread::sleep(std::time::Duration::from_millis(3000)); // Sleep 3s to meet min duration
        }

        // Final switch to finalize the last session
        store.switch_session(11, 11, "final.exe", "Final");

        assert_eq!(store.completed_sessions.len(), 10); // All 10 sessions meet min duration

        store.prune_old_sessions(5);
        assert_eq!(store.completed_sessions.len(), 5);

        // Should keep the most recent ones
        assert_eq!(store.completed_sessions[0].process_name, "app5.exe");
    }
}
