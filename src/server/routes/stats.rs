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
    let store = ACTIVITY_STORE.read().unwrap();
    let summary = store.get_daily_summary();

    Json(StatsResponse {
        sessions: summary.session_count,
        unique_apps: summary.app_count,
        keystrokes: summary.total_keystrokes,
        clicks: summary.total_clicks,
        focus_time_secs: summary.total_focus_time_secs,
        media_time_secs: store.total_media_time_secs(),
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
