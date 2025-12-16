//! Media endpoint with flexible filtering.

use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::database::MediaRecord;
use crate::store::{ACTIVITY_STORE, DATABASE};

/// Flexible query parameters for media.
#[derive(Deserialize)]
pub struct MediaQuery {
    /// Filter by date (YYYY-MM-DD)
    pub date: Option<String>,
    /// Filter from start time (ISO 8601)
    pub from: Option<String>,
    /// Filter to end time (ISO 8601)
    pub to: Option<String>,
    /// Filter by artist (supports * wildcard)
    pub artist: Option<String>,
    /// Filter by source app (supports * wildcard)
    pub source_app: Option<String>,
    /// Limit results (default: 1000)
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
    /// Order by: "asc" or "desc" (default: desc)
    pub order: Option<String>,
}

/// Current playing media info.
#[derive(Serialize)]
pub struct CurrentMedia {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_app: String,
    pub start_time: String,
    pub duration_secs: i64,
    pub is_playing: bool,
}

/// Response wrapper with metadata.
#[derive(Serialize)]
pub struct MediaResponse {
    /// Currently playing media (if any)
    pub current: Option<CurrentMedia>,
    /// Historical media from database
    pub history: Vec<MediaRecord>,
    pub total: i64,
    pub limit: usize,
    pub offset: usize,
}

/// GET /api/media - Flexible media query.
///
/// Query params:
/// - `date`: Filter by date (YYYY-MM-DD)
/// - `from`: Filter from start time (ISO 8601)
/// - `to`: Filter to end time (ISO 8601)  
/// - `artist`: Filter by artist (supports * wildcard)
/// - `source_app`: Filter by source app (supports * wildcard)
/// - `limit`: Max results (default 1000)
/// - `offset`: Pagination offset
/// - `order`: "asc" or "desc" (default desc)
pub async fn get_media(Query(query): Query<MediaQuery>) -> Json<MediaResponse> {
    // Performance safeguards: limit max results to prevent memory issues
    let limit = query.limit.unwrap_or(500).min(2000);
    let offset = query.offset.unwrap_or(0);
    let order_desc = query.order.as_deref().unwrap_or("desc") != "asc";

    // Default to today's date if no time filters provided (prevents full table scan)
    let default_date = if query.date.is_none() && query.from.is_none() && query.to.is_none() {
        Some(chrono::Utc::now().format("%Y-%m-%d").to_string())
    } else {
        query.date.clone()
    };

    // Get current playing media from memory store
    let current = {
        let store = ACTIVITY_STORE.read().unwrap();
        store.current_media.as_ref().map(|m| CurrentMedia {
            title: m.media_info.title.clone(),
            artist: m.media_info.artist.clone(),
            album: m.media_info.album.clone(),
            source_app: m.media_info.source_app_id.clone(),
            start_time: m.start_time.to_rfc3339(),
            duration_secs: m.duration_secs(),
            is_playing: m.media_info.is_playing(),
        })
    };

    // Get historical media from database
    let Some(db_arc) = DATABASE.as_ref() else {
        return Json(MediaResponse {
            current,
            history: vec![],
            total: 0,
            limit,
            offset,
        });
    };

    let Ok(db) = db_arc.lock() else {
        return Json(MediaResponse {
            current,
            history: vec![],
            total: 0,
            limit,
            offset,
        });
    };

    let (history, total) = match db.query_media_flexible(
        default_date.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
        query.artist.as_deref(),
        query.source_app.as_deref(),
        limit,
        offset,
        order_desc,
    ) {
        Ok(result) => result,
        Err(_) => {
            return Json(MediaResponse {
                current,
                history: vec![],
                total: 0,
                limit,
                offset,
            });
        }
    };

    Json(MediaResponse {
        current,
        history,
        total,
        limit,
        offset,
    })
}
