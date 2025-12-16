//! Configuration endpoint.

use axum::{http::StatusCode, Json};
use serde::Serialize;

use crate::store::DATABASE;

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub settings: Vec<ConfigSetting>,
}

#[derive(Debug, Serialize)]
pub struct ConfigSetting {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}

/// GET /api/config - Get all configuration settings
pub async fn get_config() -> Result<Json<ConfigResponse>, StatusCode> {
    let db = DATABASE
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match db.get_all_config() {
        Ok(config) => {
            let settings = config
                .into_iter()
                .map(|(key, value, description)| ConfigSetting {
                    key,
                    value,
                    description,
                })
                .collect();

            Ok(Json(ConfigResponse { settings }))
        }
        Err(e) => {
            tracing::error!(?e, "Failed to fetch config");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
