//! Data types for activity tracking.
//!
//! Defines the core data structures for storing window sessions
//! and application statistics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a single window focus session.
///
/// A session starts when a window gains focus and ends when focus
/// moves to a different window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSession {
    /// Window handle (HWND as isize). Not serialized as it's not meaningful outside the process.
    #[serde(skip)]
    pub window_handle: isize,

    /// Process ID of the window's owner process.
    pub process_id: u32,

    /// Name of the executable (e.g., "chrome.exe").
    pub process_name: String,

    /// Title of the window at session start.
    pub window_title: String,

    /// When this window gained focus.
    pub start_time: DateTime<Utc>,

    /// When this window lost focus. `None` if this is the current session.
    pub end_time: Option<DateTime<Utc>>,

    /// Number of keystrokes while this window was focused.
    pub keystrokes: u64,

    /// Number of mouse clicks while this window was focused.
    pub mouse_clicks: u64,

    /// Number of mouse scroll events while this window was focused.
    pub mouse_scrolls: u64,

    /// Whether this session represents idle/AFK time.
    pub is_idle: bool,
}

impl WindowSession {
    /// Creates a new session starting now.
    pub fn new(
        window_handle: isize,
        process_id: u32,
        process_name: String,
        window_title: String,
    ) -> Self {
        Self {
            window_handle,
            process_id,
            process_name,
            window_title,
            start_time: Utc::now(),
            end_time: None,
            keystrokes: 0,
            mouse_clicks: 0,
            mouse_scrolls: 0,
            is_idle: false,
        }
    }

    /// Finalizes the session by setting the end time to now.
    pub fn finalize(&mut self) {
        self.end_time = Some(Utc::now());
    }

    /// Returns the duration of this session in seconds.
    /// Returns 0 if the session is still active.
    pub fn duration_secs(&self) -> i64 {
        match self.end_time {
            Some(end) => (end - self.start_time).num_seconds().max(0),
            None => (Utc::now() - self.start_time).num_seconds().max(0),
        }
    }
}

/// Aggregated statistics for a single application.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplicationStats {
    /// Name of the executable.
    pub process_name: String,

    /// Total time focused on this application in seconds.
    pub total_focus_duration_secs: u64,

    /// Total keystrokes made while this application was focused.
    pub total_keystrokes: u64,

    /// Total mouse clicks made while this application was focused.
    pub total_clicks: u64,

    /// Number of separate focus sessions for this application.
    pub session_count: u32,
}

impl ApplicationStats {
    /// Creates new stats for an application.
    pub fn new(process_name: String) -> Self {
        Self {
            process_name,
            ..Default::default()
        }
    }

    /// Adds data from a completed session to these stats.
    pub fn add_session(&mut self, session: &WindowSession) {
        self.total_focus_duration_secs += session.duration_secs() as u64;
        self.total_keystrokes += session.keystrokes;
        self.total_clicks += session.mouse_clicks;
        self.session_count += 1;
    }
}

/// Summary of today's activity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailySummary {
    /// Total keystrokes today.
    pub total_keystrokes: u64,

    /// Total mouse clicks today.
    pub total_clicks: u64,

    /// Total focus time in seconds today.
    pub total_focus_time_secs: u64,

    /// Number of unique applications used.
    pub app_count: u32,

    /// Number of focus sessions.
    pub session_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_session_new() {
        let session =
            WindowSession::new(123, 456, "test.exe".to_string(), "Test Window".to_string());

        assert_eq!(session.window_handle, 123);
        assert_eq!(session.process_id, 456);
        assert_eq!(session.process_name, "test.exe");
        assert_eq!(session.window_title, "Test Window");
        assert!(session.end_time.is_none());
        assert_eq!(session.keystrokes, 0);
    }

    #[test]
    fn test_window_session_finalize() {
        let mut session = WindowSession::new(1, 1, "test.exe".to_string(), "Test".to_string());
        assert!(session.end_time.is_none());

        session.finalize();
        assert!(session.end_time.is_some());
    }

    #[test]
    fn test_application_stats_add_session() {
        let mut stats = ApplicationStats::new("chrome.exe".to_string());

        let mut session =
            WindowSession::new(1, 100, "chrome.exe".to_string(), "Google".to_string());
        session.keystrokes = 50;
        session.mouse_clicks = 10;
        session.finalize();

        stats.add_session(&session);

        assert_eq!(stats.total_keystrokes, 50);
        assert_eq!(stats.total_clicks, 10);
        assert_eq!(stats.session_count, 1);
    }

    #[test]
    fn test_serialization() {
        let session = WindowSession::new(999, 123, "app.exe".to_string(), "My App".to_string());
        let json = serde_json::to_string(&session).unwrap();

        // window_handle should be skipped
        assert!(!json.contains("999"));
        assert!(json.contains("app.exe"));
        assert!(json.contains("My App"));
    }
}
