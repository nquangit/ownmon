//! Statistics endpoints.

use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::store::{ACTIVITY_STORE, DATABASE};

#[derive(Serialize)]
pub struct StatsResponse {
    pub sessions: u32,
    pub unique_apps: u32,
    pub keystrokes: u64,
    pub clicks: u64,
    pub focus_time_secs: u64,
    pub media_time_secs: i64,
}

#[derive(Serialize)]
pub struct AppStats {
    pub process_name: String,
    pub focus_time_secs: u64,
    pub keystrokes: u64,
    pub clicks: u64,
    pub session_count: u32,
}

#[derive(Deserialize)]
pub struct DailyQuery {
    pub date: Option<String>,
}

/// GET /api/stats - Today's summary statistics.
pub async fn get_stats() -> Json<StatsResponse> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Query all of today's sessions from database using flexible query
    let (sessions, _total) = DATABASE
        .as_ref()
        .and_then(|db| db.lock().ok())
        .and_then(|d| {
            d.query_sessions_flexible(
                Some(&today), // date filter
                None,         // from
                None,         // to
                None,         // process
                10000,        // limit (large number to get all)
                0,            // offset
                false,        // order_desc
            )
            .ok()
        })
        .unwrap_or((vec![], 0));

    // Compute stats from database sessions
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

    // Add current session if active (not yet in database)
    let store = ACTIVITY_STORE.read().unwrap();
    if let Some(current) = &store.current_session {
        total_keystrokes += current.keystrokes;
        total_clicks += current.mouse_clicks;
        total_duration += current.duration_secs();
        unique_apps.insert(current.process_name.clone());
    }

    let media_time = store.total_media_time_secs();

    Json(StatsResponse {
        sessions: sessions.len() as u32
            + if store.current_session.is_some() {
                1
            } else {
                0
            },
        unique_apps: unique_apps.len() as u32,
        keystrokes: total_keystrokes,
        clicks: total_clicks,
        focus_time_secs: total_duration.max(0) as u64,
        media_time_secs: media_time,
    })
}

/// GET /api/stats/daily?date=YYYY-MM-DD - Stats for a specific date.
pub async fn get_daily_stats(Query(query): Query<DailyQuery>) -> Json<Option<DailyStatsResponse>> {
    let date = query
        .date
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    let Some(db_arc) = DATABASE.as_ref() else {
        return Json(None);
    };

    let Ok(db) = db_arc.lock() else {
        return Json(None);
    };

    // Query aggregated stats from sessions table
    match db.get_stats_for_date(&date) {
        Ok((keystrokes, clicks, focus_secs)) => Json(Some(DailyStatsResponse {
            date,
            keystrokes,
            clicks,
            focus_secs,
        })),
        Err(_) => Json(None),
    }
}

#[derive(Serialize)]
pub struct DailyStatsResponse {
    pub date: String,
    pub keystrokes: i64,
    pub clicks: i64,
    pub focus_secs: i64,
}

/// GET /api/apps - Top apps by focus time.
pub async fn get_top_apps() -> Json<Vec<AppStats>> {
    let store = ACTIVITY_STORE.read().unwrap();
    let stats = store.compute_application_stats();

    let mut apps: Vec<AppStats> = stats
        .into_iter()
        .map(|(name, stat)| AppStats {
            process_name: name,
            focus_time_secs: stat.total_focus_duration_secs,
            keystrokes: stat.total_keystrokes,
            clicks: stat.total_clicks,
            session_count: stat.session_count,
        })
        .collect();

    // Sort by focus time descending
    apps.sort_by(|a, b| b.focus_time_secs.cmp(&a.focus_time_secs));

    Json(apps)
}

#[derive(Deserialize)]
pub struct HourlyQuery {
    pub date: Option<String>,
}

/// GET /api/stats/hourly?date=YYYY-MM-DD - Hourly breakdown for charts.
pub async fn get_hourly_stats(
    Query(query): Query<HourlyQuery>,
) -> Json<Vec<crate::database::HourlyStats>> {
    let date = query
        .date
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());

    let Some(db_arc) = DATABASE.as_ref() else {
        return Json(vec![]);
    };

    let Ok(db) = db_arc.lock() else {
        return Json(vec![]);
    };

    match db.get_hourly_stats(&date) {
        Ok(stats) => Json(stats),
        Err(_) => Json(vec![]),
    }
}

#[derive(Deserialize)]
pub struct TimelineQuery {
    pub days: Option<i32>,
}

/// GET /api/stats/timeline?days=7 - Daily timeline for trend charts.
pub async fn get_timeline(
    Query(query): Query<TimelineQuery>,
) -> Json<Vec<crate::database::DailyTimeline>> {
    let days = query.days.unwrap_or(7);

    let Some(db_arc) = DATABASE.as_ref() else {
        return Json(vec![]);
    };

    let Ok(db) = db_arc.lock() else {
        return Json(vec![]);
    };

    match db.get_timeline(days) {
        Ok(timeline) => Json(timeline),
        Err(_) => Json(vec![]),
    }
}
