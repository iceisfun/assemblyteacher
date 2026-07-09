//! The single error shape every endpoint returns.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiError {
    #[serde(skip)]
    pub status: StatusCode,
    /// Human-readable, and specific. `"unknown mnemonic `frobnicate`"`, not
    /// `"bad request"`.
    pub error: String,
    /// The source line the error is attributable to, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Which subsystem rejected the request, so the UI can route the message.
    pub kind: &'static str,
}

impl ApiError {
    pub fn bad(kind: &'static str, error: impl Into<String>) -> ApiError {
        ApiError { status: StatusCode::BAD_REQUEST, error: error.into(), line: None, kind }
    }

    pub fn at_line(kind: &'static str, error: impl Into<String>, line: usize) -> ApiError {
        ApiError { status: StatusCode::BAD_REQUEST, error: error.into(), line: Some(line), kind }
    }

    pub fn not_found(error: impl Into<String>) -> ApiError {
        ApiError {
            status: StatusCode::NOT_FOUND,
            error: error.into(),
            line: None,
            kind: "not_found",
        }
    }

    pub fn too_large(error: impl Into<String>) -> ApiError {
        ApiError {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            error: error.into(),
            line: None,
            kind: "too_large",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self)).into_response()
    }
}

/// An assembler error already knows its line number.
impl From<asm_core::AsmError> for ApiError {
    fn from(e: asm_core::AsmError) -> ApiError {
        ApiError::at_line("assemble", e.to_string(), e.line)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
