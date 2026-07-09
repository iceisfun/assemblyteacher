//! The Assembly Teacher web server.
//!
//! The server holds no logic of its own. Every endpoint is a shape conversion
//! over `asm-core`, `asm-emu`, `binfmt` or `lesson`. If a handler starts
//! wanting to know something about x86, that knowledge belongs in a crate,
//! where it can be unit-tested without an HTTP client.
//!
//! TLS is assumed to be terminated by a reverse proxy. This process speaks
//! plain HTTP and never reads a certificate.

#![forbid(unsafe_code)]

pub mod error;
pub mod hexnum;
pub mod routes;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::{Json, Router};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub struct AppState {
    /// Loaded once at startup, never mutated. `&'static` so that responses can
    /// borrow lesson bodies rather than cloning them per request.
    pub curriculum: &'static lesson::Curriculum,
    pub web_dir: Option<PathBuf>,
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Build the application. Separated from `main` so the tests can drive it
/// through `tower::ServiceExt::oneshot` without binding a port.
pub fn app(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/asm/assemble", post(routes::asm::assemble))
        .route("/asm/disassemble", post(routes::asm::disassemble))
        .route("/asm/explain", post(routes::asm::explain))
        .route("/emu/run", post(routes::emu::run))
        .route("/emu/step", post(routes::emu::step))
        .route(
            "/binfmt/inspect",
            post(routes::inspect::inspect)
                .layer(DefaultBodyLimit::max(routes::inspect::MAX_UPLOAD)),
        )
        .route("/lessons", get(routes::lessons::index))
        .route("/lessons/{id}", get(routes::lessons::get))
        .route("/lessons/{id}/exercises/{exercise_id}/check", post(routes::lessons::check))
        .with_state(state.clone());

    let mut router = Router::new().nest("/api", api);

    // Serve the built frontend, falling back to index.html so the hash router's
    // deep links resolve.
    if let Some(dir) = &state.web_dir {
        let index = dir.join("index.html");
        router = router.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }

    router
        // A dev frontend on :5173 talks to this server on :8080.
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
