//! The curriculum, and grading.

use crate::error::{ApiError, ApiResult};
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use lesson::{PublicLesson, Verdict};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub parts: Vec<PartSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartSummary {
    pub number: u32,
    pub title: String,
    pub lessons: Vec<LessonSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonSummary {
    pub id: String,
    pub title: String,
    pub order: u32,
    pub objectives: Vec<String>,
    pub prerequisites: Vec<String>,
    pub estimated_minutes: Option<u32>,
    pub exercise_count: usize,
}

pub async fn index(State(state): State<Arc<AppState>>) -> Json<Index> {
    Json(Index {
        parts: state
            .curriculum
            .parts
            .iter()
            .map(|p| PartSummary {
                number: p.number,
                title: p.title.clone(),
                lessons: p
                    .lessons
                    .iter()
                    .map(|l| LessonSummary {
                        id: l.id.clone(),
                        title: l.title.clone(),
                        order: l.order,
                        objectives: l.objectives.clone(),
                        prerequisites: l.prerequisites.clone(),
                        estimated_minutes: l.estimated_minutes,
                        exercise_count: l.exercises.len(),
                    })
                    .collect(),
            })
            .collect(),
    })
}

/// One lesson, with its exercises' answers stripped.
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<PublicLesson<'static>>> {
    // `curriculum` is `&'static`: it is loaded once at startup and never
    // mutated, so the response can borrow it instead of cloning a lesson body
    // on every request.
    let lesson = state
        .curriculum
        .lesson(&id)
        .ok_or_else(|| ApiError::not_found(format!("no lesson `{id}`")))?;
    Ok(Json(lesson.public()))
}

#[derive(Deserialize)]
pub struct CheckRequest {
    /// The choice index for a quiz; source text otherwise.
    pub answer: String,
}

pub async fn check(
    State(state): State<Arc<AppState>>,
    Path((id, exercise_id)): Path<(String, String)>,
    Json(req): Json<CheckRequest>,
) -> ApiResult<Json<Verdict>> {
    if req.answer.len() > 64 * 1024 {
        return Err(ApiError::too_large("answer is larger than 64 KiB"));
    }

    let lesson = state
        .curriculum
        .lesson(&id)
        .ok_or_else(|| ApiError::not_found(format!("no lesson `{id}`")))?;
    let exercise = lesson
        .exercise(&exercise_id)
        .ok_or_else(|| ApiError::not_found(format!("no exercise `{exercise_id}` in `{id}`")))?;

    // Grading runs the submission on the emulator, under a step limit set by
    // the exercise. It cannot touch the host.
    Ok(Json(lesson::check(exercise, &req.answer)))
}
