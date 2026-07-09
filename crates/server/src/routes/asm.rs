//! Assemble, disassemble, and explain.
//!
//! Every handler here is a shape conversion. The logic lives in `asm-core`.

use crate::error::{ApiError, ApiResult};
use crate::hexnum::{from_hex, to_hex, U64};
use asm_core::{decode, format, Decoder};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Refuse absurd inputs before doing any work.
const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_CODE_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssembleRequest {
    pub source: String,
    #[serde(default)]
    pub origin: Option<U64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssembleResponse {
    pub hex: String,
    pub origin: U64,
    pub labels: BTreeMap<String, U64>,
    pub lines: Vec<AsmLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsmLine {
    pub line: usize,
    pub address: U64,
    pub hex: String,
    pub text: String,
}

pub async fn assemble(Json(req): Json<AssembleRequest>) -> ApiResult<Json<AssembleResponse>> {
    if req.source.len() > MAX_SOURCE_BYTES {
        return Err(ApiError::too_large("source is larger than 256 KiB"));
    }

    let out = asm_core::asm::assemble_at(&req.source, req.origin.unwrap_or_default().0)?;

    Ok(Json(AssembleResponse {
        hex: to_hex(&out.bytes),
        origin: U64(out.origin),
        labels: out.labels.into_iter().map(|(k, v)| (k, U64(v))).collect(),
        lines: out
            .lines
            .into_iter()
            .map(|l| AsmLine {
                line: l.line,
                address: U64(l.address),
                hex: to_hex(&l.bytes),
                text: l.text,
            })
            .collect(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisassembleRequest {
    pub hex: String,
    #[serde(default)]
    pub base: Option<U64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisassembleResponse {
    pub instructions: Vec<InsnDto>,
    /// Where the linear sweep gave up, if it did. A partial answer plus the
    /// reason is more useful than an error — and *where* a sweep derails is
    /// itself one of the lessons.
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsnDto {
    pub ip: U64,
    pub hex: String,
    pub text: String,
    pub description: String,
    pub length: usize,
    pub mnemonic: String,
    pub branch_target: Option<U64>,
    pub falls_through: bool,
}

impl From<asm_core::Insn> for InsnDto {
    fn from(i: asm_core::Insn) -> InsnDto {
        InsnDto {
            ip: U64(i.ip),
            hex: to_hex(&i.bytes()),
            text: format::to_string(&i),
            description: format::describe(&i),
            length: i.len(),
            mnemonic: i.mnemonic.name(),
            branch_target: i.branch_target().map(U64),
            falls_through: i.mnemonic.falls_through(),
        }
    }
}

fn decode_hex(hex: &str, field: &str) -> ApiResult<Vec<u8>> {
    let bytes = from_hex(hex)
        .ok_or_else(|| ApiError::bad("hex", format!("`{field}` is not a hex byte string")))?;
    if bytes.len() > MAX_CODE_BYTES {
        return Err(ApiError::too_large("more than 1 MiB of machine code"));
    }
    Ok(bytes)
}

pub async fn disassemble(
    Json(req): Json<DisassembleRequest>,
) -> ApiResult<Json<DisassembleResponse>> {
    let bytes = decode_hex(&req.hex, "hex")?;
    let base = req.base.unwrap_or_default().0;

    let mut instructions = Vec::new();
    let mut error = None;
    for result in Decoder::new(&bytes, base) {
        match result {
            Ok(insn) => instructions.push(insn.into()),
            Err(e) => {
                error = Some(e.to_string());
                break;
            }
        }
    }

    Ok(Json(DisassembleResponse { instructions, error }))
}

#[derive(Deserialize)]
pub struct ExplainRequest {
    pub hex: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResponse {
    pub text: String,
    pub length: usize,
    pub fields: Vec<Field>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: &'static str,
    pub hex: String,
    /// Byte offset of this field within the instruction.
    pub offset: usize,
    pub explanation: String,
}

pub async fn explain(Json(req): Json<ExplainRequest>) -> ApiResult<Json<ExplainResponse>> {
    let bytes = decode_hex(&req.hex, "hex")?;
    let insn = decode(&bytes, 0).map_err(|e| ApiError::bad("decode", e.to_string()))?;

    // `explain()` walks the fields in encoding order, so running the offset
    // forward as we go is exactly right.
    let mut offset = 0;
    let fields = insn
        .encoding
        .explain()
        .into_iter()
        .map(|(name, bytes, explanation)| {
            let at = offset;
            offset += bytes.len();
            Field { name, hex: to_hex(&bytes), offset: at, explanation }
        })
        .collect();

    Ok(Json(ExplainResponse { text: format::to_string(&insn), length: insn.len(), fields }))
}
