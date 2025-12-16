//! Statistics aggregation utilities.
//!
//! Provides functions for computing aggregated statistics from
//! activity data.

use super::types::{ApplicationStats, DailySummary, WindowSession};
use chrono::{DateTime, Timelike, Utc};
use std::collections::HashMap;

/// Computes aggregated statistics grouped by application.
pub fn compute_app_stats(sessions: &[WindowSession]) -> HashMap<String, ApplicationStats> {
    let mut stats: HashMap<String, ApplicationStats> = HashMap::new();

    for session in sessions {
        let entry = stats
            .entry(session.process_name.clone())
            .or_insert_with(|| ApplicationStats::new(session.process_name.clone()));

        entry.add_session(session);
    }

    stats
}

/// Computes statistics for a specific time range.
pub fn compute_stats_for_range(
    sessions: &[WindowSession],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> DailySummary {
    let filtered: Vec<_> = sessions
        .iter()
        .filter(|s| s.start_time >= start && s.start_time < end)
        .collect();

    let mut total_keystrokes = 0u64;
    let mut total_clicks = 0u64;
    let mut total_focus_secs = 0u64;
    let mut apps: std::collections::HashSet<String> = std::collections::HashSet::new();

    for session in &filtered {
        total_keystrokes += session.keystrokes;
        total_clicks += session.mouse_clicks;
        total_focus_secs += session.duration_secs() as u64;
        apps.insert(session.process_name.clone());
    }

    DailySummary {
        total_keystrokes,
        total_clicks,
        total_focus_time_secs: total_focus_secs,
        app_count: apps.len() as u32,
        session_count: filtered.len() as u32,
    }
}

/// Returns the top N applications by focus time.
pub fn top_apps_by_focus_time(
    sessions: &[WindowSession],
    n: usize,
) -> Vec<(String, ApplicationStats)> {
    let stats = compute_app_stats(sessions);
    let mut sorted: Vec<_> = stats.into_iter().collect();
    sorted.sort_by(|a, b| {
        b.1.total_focus_duration_secs
            .cmp(&a.1.total_focus_duration_secs)
    });
    sorted.truncate(n);
    sorted
}

/// Returns the top N applications by keystrokes.
pub fn top_apps_by_keystrokes(
    sessions: &[WindowSession],
    n: usize,
) -> Vec<(String, ApplicationStats)> {
    let stats = compute_app_stats(sessions);
    let mut sorted: Vec<_> = stats.into_iter().collect();
    sorted.sort_by(|a, b| b.1.total_keystrokes.cmp(&a.1.total_keystrokes));
    sorted.truncate(n);
    sorted
}

/// Groups sessions by hour of day for activity heatmap.
pub fn activity_by_hour(sessions: &[WindowSession]) -> [u64; 24] {
    let mut hours = [0u64; 24];

    for session in sessions {
        let hour = session.start_time.hour() as usize;
        hours[hour] += session.duration_secs() as u64;
    }

    hours
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_session(process: &str, keys: u64, clicks: u64) -> WindowSession {
        let mut session = WindowSession::new(1, 100, process.to_string(), "Test".to_string());
        session.keystrokes = keys;
        session.mouse_clicks = clicks;
        session.finalize();
        session
    }

    #[test]
    fn test_compute_app_stats() {
        let sessions = vec![
            create_test_session("chrome.exe", 100, 50),
            create_test_session("chrome.exe", 50, 25),
            create_test_session("code.exe", 200, 10),
        ];

        let stats = compute_app_stats(&sessions);

        assert_eq!(stats.len(), 2);
        assert_eq!(stats["chrome.exe"].total_keystrokes, 150);
        assert_eq!(stats["chrome.exe"].session_count, 2);
        assert_eq!(stats["code.exe"].total_keystrokes, 200);
    }

    #[test]
    fn test_top_apps_by_keystrokes() {
        let sessions = vec![
            create_test_session("a.exe", 100, 0),
            create_test_session("b.exe", 200, 0),
            create_test_session("c.exe", 50, 0),
        ];

        let top = top_apps_by_keystrokes(&sessions, 2);

        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "b.exe");
        assert_eq!(top[1].0, "a.exe");
    }
}
