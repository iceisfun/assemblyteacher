//! Errors produced by decoding, encoding and assembling.

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The byte slice ended in the middle of an instruction. `need` is how many
    /// more bytes the decoder wanted at the point it gave up.
    #[error("truncated instruction: needed {need} more byte(s) at offset {at}")]
    Truncated { at: usize, need: usize },

    /// The bytes are not a valid instruction, or not one this decoder knows.
    #[error("unsupported or invalid opcode {opcode:#04x} at offset {at}")]
    BadOpcode { at: usize, opcode: u8 },

    /// An encoding that is legal on 32-bit x86 but illegal in 64-bit mode.
    #[error("opcode {opcode:#04x} is invalid in 64-bit mode (offset {at})")]
    InvalidInLongMode { at: usize, opcode: u8 },

    /// More prefix bytes than any real instruction has. Guards against a
    /// crafted input making the decoder loop.
    #[error("instruction exceeds the 15-byte architectural limit")]
    TooLong,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EncodeError {
    #[error("{mnemonic} does not accept operands {operands}")]
    BadOperands { mnemonic: String, operands: String },

    #[error("operand size mismatch: {0}")]
    SizeMismatch(String),

    #[error("immediate {value:#x} does not fit in {bytes} byte(s)")]
    ImmediateOutOfRange { value: i64, bytes: u8 },

    #[error("cannot encode {reg}: it requires a REX prefix, but {other} forbids one")]
    RexConflict { reg: String, other: String },

    #[error("{0} is not supported by the encoder yet")]
    Unsupported(String),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("line {line}: {kind}")]
pub struct AsmError {
    pub line: usize,
    pub kind: AsmErrorKind,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AsmErrorKind {
    #[error("unknown mnemonic `{0}`")]
    UnknownMnemonic(String),

    #[error("unknown register `{0}`")]
    UnknownRegister(String),

    #[error("undefined label `{0}`")]
    UndefinedLabel(String),

    #[error("label `{0}` is defined more than once")]
    DuplicateLabel(String),

    #[error(
        "branch to `{label}` is {distance} bytes away, too far for a {bytes}-byte displacement"
    )]
    BranchOutOfRange { label: String, distance: i64, bytes: u8 },

    #[error("expected {expected}, found `{found}`")]
    Expected { expected: &'static str, found: String },

    #[error("bad number `{0}`")]
    BadNumber(String),

    #[error("bad memory operand: {0}")]
    BadMemory(String),

    #[error("scale must be 1, 2, 4 or 8, found {0}")]
    BadScale(u64),

    #[error("could not encode: {0}")]
    Encode(#[from] EncodeError),
}
