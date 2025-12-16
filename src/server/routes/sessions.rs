//! Sessions endpoint with flexible filtering.

use axum::{extract::Query, Json};
use serde::{Deserialize, Serialize};

use crate::database::{Category, SessionWithDuration};
use crate::store::DATABASE;

/// Flexible query parameters for sessions.
#[derive(Deserialize)]
pub struct SessionsQuery {
    /// Filter by date (YYYY-MM-DD)
    pub date: Option<String>,
    /// Filter from start time (ISO 8601)
    pub from: Option<String>,
    /// Filter to end time (ISO 8601)
    pub to: Option<String>,
    /// Filter by process name (exact or pattern with *)
    pub app: Option<String>,
    /// Filter by category ID
    pub category: Option<i64>,
    /// Limit results (default: 1000)
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
    /// Order by: "asc" or "desc" (default: desc)
    pub order: Option<String>,
}

/// Enhanced session response with category info.
#[derive(Serialize)]
pub struct SessionWithCategory {
    #[serde(flatten)]
    pub session: SessionWithDuration,
    pub category: Option<Category>,
}

/// Response wrapper with metadata.
#[derive(Serialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionWithCategory>,
    pub total: i64,
    pub limit: usize,
    pub offset: usize,
}

/// GET /api/sessions - Flexible sessions query.
///
/// Query params:
/// - `date`: Filter by date (YYYY-MM-DD)
/// - `from`: Filter from start time (ISO 8601)
/// - `to`: Filter to end time (ISO 8601)  
/// - `app`: Filter by process name (supports * wildcard)
/// - `category`: Filter by category ID
/// - `limit`: Max results (default 1000)
/// - `offset`: Pagination offset
/// - `order`: "asc" or "desc" (default desc)
pub async fn get_sessions(Query(query): Query<SessionsQuery>) -> Json<SessionsResponse> {
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

    let Some(db_arc) = DATABASE.as_ref() else {
        return Json(SessionsResponse {
            sessions: vec![],
            total: 0,
            limit,
            offset,
        });
    };

    let Ok(db) = db_arc.lock() else {
        return Json(SessionsResponse {
            sessions: vec![],
            total: 0,
            limit,
            offset,
        });
    };

    // Query sessions using the flexible method
    let (sessions, total) = match db.query_sessions_flexible(
        default_date.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
        query.app.as_deref(),
        limit,
        offset,
        order_desc,
    ) {
        Ok(result) => result,
        Err(_) => {
            return Json(SessionsResponse {
                sessions: vec![],
                total: 0,
                limit,
                offset,
            });
        }
    };

    // Get category for each session
    let sessions_with_categories: Vec<SessionWithCategory> = sessions
        .into_iter()
        .map(|session| {
            let category = db.get_category_for_app(&session.process_name).ok();
            SessionWithCategory { session, category }
        })
        .collect();

    // Filter by category if specified
    let sessions = if let Some(cat_id) = query.category {
        sessions_with_categories
            .into_iter()
            .filter(|s| s.category.as_ref().map(|c| c.id) == Some(cat_id))
            .collect()
    } else {
        sessions_with_categories
    };

    Json(SessionsResponse {
        sessions,
        total,
        limit,
        offset,
    })
}
