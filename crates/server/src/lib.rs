//! The Assembly Teacher web server.
//!
//! The server holds no logic of its own. Every endpoint is a shape conversion
//! over `asm-core`, `asm-emu`, `binfmt` or `lesson`. If a handler starts
//! wanting to know something about x86, that knowledge belongs in a crate,
//! where it can be unit-tested without an HTTP client.
//!
//! TLS is assumed to be terminated by a reverse proxy. This process speaks
//! plain HTTP and never reads a certificate.
//!
//! # Exposure model
//!
//! This is safe to face the public internet *behind a reverse proxy that does
//! rate limiting* (see `docs/deployment.md`). The application's own defences:
//!
//! - **No host access.** The emulator is a pure interpreter; the only syscalls
//!   it implements are `write` to fds 1–2 (into a buffer) and `exit`. Submitted
//!   assembly cannot touch the filesystem, the network, or the process.
//! - **No `unsafe`** anywhere in the workspace, and the executable parser is
//!   fuzzed to never panic on hostile input.
//! - **Bounded work.** Every interpreter run is capped in steps *and* in trace
//!   memory; every input is size-limited; the assembler's relaxation loop is
//!   bounded.
//! - **Off the async runtime.** All CPU-bound work (interpret, assemble,
//!   disassemble, parse, grade) runs on a blocking thread, so no single request
//!   can stall the workers that keep the server responsive.
//! - **A wall-clock timeout** and **panic isolation** on every request.
//!
//! What it deliberately leaves to the proxy: rate limiting and connection caps.
//! Those belong at the edge, where they can be tuned without a redeploy.

#![forbid(unsafe_code)]

pub mod error;
pub mod hexnum;
pub mod routes;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

/// Hard wall-clock cap on any single request. A backstop: the per-endpoint step
/// and size limits already bound the real work to well under this, so a request
/// that hits it is pathological (or a slow client), and gets a 408.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub struct AppState {
    /// Loaded once at startup, never mutated. `&'static` so that responses can
    /// borrow lesson bodies rather than cloning them per request.
    pub curriculum: &'static lesson::Curriculum,
    pub web_dir: Option<PathBuf>,
    /// Allowed browser origins for cross-origin API calls. **Empty means
    /// same-origin only** — the production default, since the binary serves the
    /// frontend from the same origin, so no CORS header is needed or wanted. A
    /// split dev setup (Vite on :5173 → API on :8080) passes the dev origin
    /// here. We never emit a wildcard `Access-Control-Allow-Origin`.
    pub cors_origins: Vec<String>,
}

impl AppState {
    /// The common case for tests and API-only servers: a curriculum, no static
    /// files, same-origin only.
    pub fn api_only(curriculum: &'static lesson::Curriculum) -> AppState {
        AppState { curriculum, web_dir: None, cors_origins: Vec::new() }
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Build a CORS layer restricted to an explicit origin allowlist. Returns
/// `None` when the list is empty (same-origin only) so no CORS headers are sent
/// at all. Invalid origin strings are dropped with a warning rather than
/// panicking the server at startup.
fn cors_layer(origins: &[String]) -> Option<CorsLayer> {
    if origins.is_empty() {
        return None;
    }
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(v) => Some(v),
            Err(_) => {
                tracing::warn!("ignoring invalid CORS origin `{o}`");
                None
            }
        })
        .collect();
    if allowed.is_empty() {
        return None;
    }
    Some(
        CorsLayer::new()
            .allow_origin(allowed)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([header::CONTENT_TYPE]),
    )
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
        .route("/search", get(routes::search::search))
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

    if let Some(cors) = cors_layer(&state.cors_origins) {
        router = router.layer(cors);
    }

    router
        // A panicking handler must not drop the connection or take the server
        // down; turn it into a clean 500. Nothing here is expected to panic —
        // there is no `unsafe` and the parser is fuzzed — but this is cheap.
        .layer(CatchPanicLayer::new())
        // A hard per-request wall-clock cap, above the real work's own limits.
        // A request that hits it gets a 408.
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
        .layer(TraceLayer::new_for_http())
}
