//! Site search.
//!
//! This endpoint searches the one corpus the server holds — the curriculum —
//! and tags each result with its `kind`. The frontend federates these lesson
//! results with the register and instruction catalogs it already carries, so
//! `/api/search` is the lesson slice of a multi-kind search rather than a
//! lessons-only route hidden under `/lessons`. Every result is `kind: "lesson"`
//! today; the discriminator is what lets more kinds join later without a new
//! shape.

use crate::error::{ApiError, ApiResult};
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use lesson::SearchHit;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct Hit {
    /// The result kind, so a federated client can group by it. Always "lesson".
    kind: &'static str,
    #[serde(flatten)]
    inner: SearchHit,
}

/// Full-text-ish search across the curriculum. The corpus is small and resident,
/// so this scans in memory on every call — no index, no keyword metadata.
pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> ApiResult<Json<Vec<Hit>>> {
    if params.q.len() > 128 {
        return Err(ApiError::too_large("query is longer than 128 bytes"));
    }
    let limit = params.limit.unwrap_or(12).clamp(1, 50);
    let hits = lesson::search(state.curriculum, &params.q, limit)
        .into_iter()
        .map(|inner| Hit { kind: "lesson", inner })
        .collect();
    Ok(Json(hits))
}
