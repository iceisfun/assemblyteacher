//! Error type for the binary-format parsers.
//!
//! The guiding rule of this crate is: *never panic on hostile input*.  Every
//! failure that would otherwise be an out-of-bounds index, an overflow, or a
//! nonsensical field is turned into one of these variants.  A student reading a
//! stack trace should never see `binfmt` in it.

use core::fmt;

/// Everything that can go wrong while parsing an executable.
///
/// The variants are intentionally *specific* — "truncated at offset N, needed M
/// bytes" tells you far more than a generic "parse error", and that specificity
/// is itself part of the lesson: a file format is a contract, and each variant
/// names a clause of the contract that the input broke.
#[derive(Debug, thiserror::Error)]
pub enum BinError {
    /// The bytes are neither an ELF nor a PE image (bad or missing magic).
    #[error("unrecognized binary format (not ELF or PE)")]
    Unknown,

    /// A read ran off the end of the buffer.  This is by far the most common
    /// error on fuzzed/truncated input.
    #[error(
        "input truncated: needed {needed} byte(s) at offset {offset}, buffer is {len} byte(s)"
    )]
    Truncated {
        /// Offset the read started at.
        offset: usize,
        /// Number of bytes the read wanted.
        needed: usize,
        /// Actual length of the buffer.
        len: usize,
    },

    /// ELF `e_ident` did not start with `\x7fELF`.
    #[error("bad ELF magic")]
    BadElfMagic,

    /// This crate only decodes 64-bit ELF (`ELFCLASS64`).
    #[error("unsupported ELF class {0:#x} (only 64-bit ELF is supported)")]
    UnsupportedElfClass(u8),

    /// This crate only decodes little-endian ELF (`ELFDATA2LSB`).
    #[error("unsupported ELF data encoding {0:#x} (only little-endian is supported)")]
    UnsupportedElfData(u8),

    /// `e_version` / `EI_VERSION` was not `EV_CURRENT` (1).
    #[error("unsupported ELF version {0}")]
    UnsupportedElfVersion(u32),

    /// The DOS stub did not start with `MZ`.
    #[error("bad PE DOS magic (expected 'MZ')")]
    BadDosMagic,

    /// The `PE\0\0` signature was missing at `e_lfanew`.
    #[error("bad PE signature (expected 'PE\\0\\0')")]
    BadPeSignature,

    /// The optional-header magic was not `0x10b` (PE32) or `0x20b` (PE32+).
    #[error("unsupported PE optional-header magic {0:#x}")]
    UnsupportedPeMagic(u16),

    /// A count/size product or offset addition overflowed a `usize`/`u64`.
    #[error("integer overflow while computing {0}")]
    Overflow(&'static str),

    /// A field held a value that cannot possibly be valid (e.g. an
    /// out-of-range `.shstrtab` index, a section that overlaps the header).
    #[error("malformed structure: {0}")]
    Malformed(&'static str),

    /// A PE relative virtual address did not fall inside any section.
    #[error("RVA {rva:#x} does not map to any section")]
    UnmappedRva {
        /// The offending RVA.
        rva: u64,
    },
}

impl BinError {
    pub(crate) fn truncated(offset: usize, needed: usize, len: usize) -> Self {
        BinError::Truncated { offset, needed, len }
    }
}

// A tiny hand-written Serialize so the server can render errors as JSON strings
// without pulling the whole enum into the wire format.
impl serde::Serialize for BinError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&DisplayError(self))
    }
}

struct DisplayError<'a>(&'a BinError);
impl fmt::Display for DisplayError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
