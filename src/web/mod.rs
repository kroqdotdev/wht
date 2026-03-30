pub mod handlers;
pub mod state;

use axum::routing::{get, post};
use axum::Router;
use state::{AppState, SharedState};
use std::sync::Arc;

pub async fn start_server(port: u16) {
    let state: SharedState = Arc::new(AppState::new());

    let app = Router::new()
        .route("/", get(handlers::index))
        .route("/api/templates", get(handlers::list_templates))
        .route("/api/templates/{name}", get(handlers::get_template))
        .route("/api/send", post(handlers::start_send))
        .route("/api/jobs/{id}/events", get(handlers::job_events))
        .route("/api/jobs/{id}/cancel", post(handlers::cancel_job))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    println!("wht web GUI running at http://localhost:{port}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
