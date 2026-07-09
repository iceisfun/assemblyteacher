//! # asm-core
//!
//! An x86_64 instruction decoder, encoder and assembler, written to be read.
//!
//! This crate is the foundation of Assembly Teacher. It is not a wrapper around
//! someone else's disassembler — every prefix, ModRM byte and displacement is
//! decoded by code in this crate, because the lessons point directly at that
//! code. When a lesson explains why `rsp` cannot be an index register, it links
//! to the seven lines in [`decode`] that enforce it.
//!
//! ## Scope
//!
//! The integer core of x86_64: moves, arithmetic, logic, shifts, branches,
//! calls, the stack, and `syscall`. No SSE, no AVX, no x87. That subset covers
//! the great majority of instructions in compiled straight-line code, and it
//! fits in a few thousand readable lines.
//!
//! ## Example
//!
//! ```
//! use asm_core::{decode, format};
//!
//! // 48 8b 44 24 08
//! let insn = decode(&[0x48, 0x8b, 0x44, 0x24, 0x08], 0x1000).unwrap();
//! assert_eq!(format::to_string(&insn), "mov rax, qword [rsp+0x8]");
//! assert_eq!(insn.len(), 5);
//!
//! // Every byte knows which field it belonged to.
//! let fields: Vec<&str> = insn.encoding.explain().iter().map(|(name, _, _)| *name).collect();
//! assert_eq!(fields, ["REX", "opcode", "ModRM", "SIB", "displacement"]);
//! ```
//!
//! Round-tripping is checked by the test suite: assembling the text form of any
//! instruction this crate decodes reproduces bytes that decode to the same
//! instruction.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod asm;
pub mod decode;
pub mod encode;
pub mod error;
pub mod format;
pub mod insn;
pub mod operand;
pub mod reg;

pub use asm::{assemble, Assembled};
pub use decode::{decode, Decoder};
pub use encode::encode;
pub use error::{AsmError, AsmErrorKind, DecodeError, EncodeError};
pub use insn::{Cond, Encoding, Insn, Mnemonic, RepPrefix};
pub use operand::{Mem, Operand};
pub use reg::{Reg, Seg, Size};
