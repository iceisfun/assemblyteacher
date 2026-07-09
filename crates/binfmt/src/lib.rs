//! `binfmt` — teaching-grade ELF64 and PE32+ parsers.
//!
//! This crate is written to be *read*. It powers the executable-inspector in the
//! "Assembly Teacher" project, but its real job is to be a legible, honest,
//! panic-free reference implementation of the two executable formats a student
//! of x86-64 reverse engineering will meet: **ELF** on Linux and **PE** on
//! Windows.
//!
//! # Design rules
//! * `#![forbid(unsafe_code)]` — there is no `unsafe` here and never will be.
//! * **Never panic on hostile input.** Every byte access is bounds-checked (see
//!   [`reader`]), every offset arithmetic uses `checked_add`/`checked_mul`, and
//!   a corrupt field yields an [`Err`] or an empty result, never an abort.
//! * **No third-party format crates.** No `goblin`, `object`, `elf`, or `pe` —
//!   the whole point is that *this* code is the lesson. Only `serde` (for the
//!   JSON the server emits) and `thiserror` (for [`BinError`]) are used.
//!
//! # Two views of a program
//! Both formats distinguish the *linker's* view (named sections) from the
//! *loader's* view (memory segments). Keep that split in mind while reading:
//! [`Section`] is what the linker arranged; [`Segment`] is what actually gets
//! mapped and executed.
//!
//! # Quick start
//! ```no_run
//! let bytes = std::fs::read("/bin/ls").unwrap();
//! if let Some(fmt) = binfmt::detect(&bytes) {
//!     println!("format: {fmt}");
//!     let image = binfmt::parse(&bytes).unwrap();
//!     if let Some((addr, code)) = image.text(&bytes) {
//!         println!(".text loads at {addr:#x}, {} bytes", code.len());
//!     }
//! }
//! ```
#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]

mod elf;
mod error;
mod pe;
mod reader;
mod types;

pub use error::BinError;
pub use types::{
    Arch, Export, Format, Image, Import, ImportKind, Mitigations, Reloc, Relro, Section,
    SectionFlags, Segment, Symbol, SymbolBinding, SymbolKind,
};

/// Identify the container format of `bytes` without fully parsing it.
///
/// This only inspects magic numbers, so it is cheap and total: it returns
/// `Some(Format)` for anything that *looks* like an ELF or PE (and would then be
/// worth handing to [`parse`]), and `None` otherwise. It never allocates and
/// never fails.
pub fn detect(bytes: &[u8]) -> Option<Format> {
    if elf::is_elf(bytes) {
        Some(Format::Elf)
    } else if pe::is_pe(bytes) {
        Some(Format::Pe)
    } else {
        None
    }
}

/// Parse `bytes` into a fully populated [`Image`].
///
/// Dispatches on the detected format. Returns [`BinError::Unknown`] if the bytes
/// are neither ELF nor PE, or a more specific variant if the header for the
/// detected format is malformed. **This function never panics**, regardless of
/// how corrupt or truncated the input is.
pub fn parse(bytes: &[u8]) -> Result<Image, BinError> {
    match detect(bytes) {
        Some(Format::Elf) => elf::parse(bytes),
        Some(Format::Pe) => pe::parse(bytes),
        None => Err(BinError::Unknown),
    }
}

#[cfg(test)]
mod tests;
