//! Categories endpoint.

use axum::{extract::Path, Json};
use serde::Serialize;

use crate::database::Category;
use crate::store::DATABASE;

#[derive(Serialize)]
pub struct CategoryWithApps {
    #[serde(flatten)]
    pub category: Category,
    pub apps: Vec<String>,
}

/// GET /api/categories - List all categories.
pub async fn get_categories() -> Json<Vec<Category>> {
    let Some(db_arc) = DATABASE.as_ref() else {
        return Json(vec![]);
    };

    let Ok(db) = db_arc.lock() else {
        return Json(vec![]);
    };

    match db.get_categories() {
        Ok(categories) => Json(categories),
        Err(_) => Json(vec![]),
    }
}

/// GET /api/apps/:name/category - Get category for an app.
pub async fn get_app_category(Path(name): Path<String>) -> Json<Option<Category>> {
    let Some(db_arc) = DATABASE.as_ref() else {
        return Json(None);
    };

    let Ok(db) = db_arc.lock() else {
        return Json(None);
    };

    match db.get_category_for_app(&name) {
        Ok(cat) => Json(Some(cat)),
        Err(_) => Json(None),
    }
}
