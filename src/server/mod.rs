//! HTTP server module for API and WebSocket endpoints.
//!
//! Provides a REST API and WebSocket for real-time updates to frontends.

pub mod routes;
pub mod state;
pub mod ws;

use crate::server::routes::{health, media, sessions, stats};
use crate::server::state::AppState;
use crate::server::ws::ws_handler;

use axum::{routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

/// Default server port.
pub const DEFAULT_PORT: u16 = 13234;

/// Starts the HTTP server on a background thread.
///
/// Returns a handle to the broadcast sender for pushing updates.
pub fn start_server() -> broadcast::Sender<String> {
    let (tx, _) = broadcast::channel::<String>(100);
    let tx_clone = tx.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async {
            run_server(tx_clone).await;
        });
    });

    tracing::info!(port = DEFAULT_PORT, "HTTP server starting");
    tx
}

/// Runs the axum server.
async fn run_server(broadcast_tx: broadcast::Sender<String>) {
    let state = Arc::new(AppState::new(broadcast_tx));

    // CORS layer for frontend
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Health check
        .route("/health", get(health::health_check))
        // Stats API
        .route("/api/stats", get(stats::get_stats))
        .route("/api/stats/daily", get(stats::get_daily_stats))
        .route("/api/stats/hourly", get(stats::get_hourly_stats))
        .route("/api/stats/timeline", get(stats::get_timeline))
        // Data API
        .route("/api/sessions", get(sessions::get_sessions))
        .route("/api/media", get(media::get_media))
        .route("/api/apps", get(stats::get_top_apps))
        // Categories API
        .route("/api/categories", get(routes::categories::get_categories))
        .route(
            "/api/apps/:name/category",
            get(routes::categories::get_app_category),
        )
        // Config API
        .route("/api/config", get(routes::config::get_config))
        // WebSocket
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT));
    tracing::info!("HTTP server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
