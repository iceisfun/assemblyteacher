//! Executable inspection: ELF and PE.

use crate::error::{ApiError, ApiResult};
use crate::hexnum::from_hex;
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::header::CONTENT_TYPE;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

/// Matches the `DefaultBodyLimit` applied to this route.
pub const MAX_UPLOAD: usize = 16 * 1024 * 1024;

#[derive(Deserialize)]
pub struct InspectJson {
    pub hex: String,
}

/// Accepts either `multipart/form-data` with a `file` part, or
/// `application/json` with a `hex` field for small inputs.
///
/// The parser is total: `binfmt` is fuzzed and returns `Err` rather than
/// panicking on hostile input, so an uploaded file can be arbitrarily broken.
pub async fn inspect(req: Request) -> ApiResult<Json<Value>> {
    let is_multipart = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("multipart/form-data"));

    let bytes = if is_multipart { multipart_bytes(req).await? } else { json_bytes(req).await? };

    if bytes.is_empty() {
        return Err(ApiError::bad("request", "no file contents"));
    }

    // Parsing a 16 MiB upload is CPU work; keep it off the async runtime.
    // `binfmt` is fuzzed and returns `Err` rather than panicking on hostile
    // input, so the join below effectively never sees a panic.
    let parsed = tokio::task::spawn_blocking(move || binfmt::parse(&bytes))
        .await
        .map_err(|_| ApiError::internal("the parse task panicked"))?;
    let image = parsed.map_err(|e| ApiError::bad("binfmt", e.to_string()))?;

    // `binfmt`'s types serialise directly. Unlike a register's contents, every
    // address in a real executable is far below 2^53, so JSON numbers represent
    // them exactly here. See docs/api.md for why the emulator differs.
    let value = serde_json::to_value(&image)
        .map_err(|e| ApiError::bad("binfmt", format!("could not serialise: {e}")))?;
    Ok(Json(value))
}

async fn json_bytes(req: Request) -> ApiResult<Vec<u8>> {
    let collected = axum::body::to_bytes(req.into_body(), MAX_UPLOAD)
        .await
        .map_err(|_| ApiError::too_large("upload exceeds 16 MiB"))?;
    let parsed: InspectJson = serde_json::from_slice(&collected).map_err(|e| {
        ApiError::bad("request", format!("expected JSON `{{\"hex\": \"..\"}}`: {e}"))
    })?;
    from_hex(&parsed.hex).ok_or_else(|| ApiError::bad("hex", "`hex` is not a hex byte string"))
}

async fn multipart_bytes(req: Request) -> ApiResult<Vec<u8>> {
    let mut multipart = Multipart::from_request(req, &())
        .await
        .map_err(|e| ApiError::bad("request", format!("bad multipart body: {e}")))?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad("request", format!("bad multipart field: {e}")))?
    {
        if field.name() == Some("file") {
            let data =
                field.bytes().await.map_err(|_| ApiError::too_large("upload exceeds 16 MiB"))?;
            return Ok(data.to_vec());
        }
    }
    Err(ApiError::bad("request", "multipart body has no `file` part"))
}
