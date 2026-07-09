//! Faults: the ways a step can fail without ever panicking.
//!
//! Every path that could touch memory, divide, or decode returns a [`Fault`]
//! instead of unwinding. The interpreter is driven by untrusted bytes from a
//! browser, so "never panic on any input" is a hard requirement, not an
//! aspiration — a panic here would take down the whole server.

use crate::mem::Perms;
use serde::Serialize;
use thiserror::Error;

/// Which kind of access provoked a memory fault. Kept distinct from [`Perms`]
/// because the *reason* ("I was fetching an instruction") is more useful to a
/// student than the raw permission bit that was missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Access {
    Read,
    Write,
    /// An instruction fetch. Separated from `Read` because a page can be
    /// readable but non-executable (W^X), and confusing the two is exactly the
    /// bug that data-execution-prevention exists to catch.
    Fetch,
}

impl core::fmt::Display for Access {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Access::Read => "read",
            Access::Write => "write",
            Access::Fetch => "fetch",
        })
    }
}

/// A recoverable execution failure. Corresponds roughly to the CPU exceptions a
/// user-mode program can raise (#PF, #DE, #UD) plus the emulator-specific cases
/// (a syscall we do not implement, bytes we cannot decode).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Error)]
pub enum Fault {
    /// The access touched an address in no mapped region — a page fault with no
    /// backing page at all.
    #[error("{access} of {len} byte(s) at {addr:#x} is not mapped")]
    NotMapped { addr: u64, len: usize, access: Access },

    /// The address is mapped, but the region lacks the permission this access
    /// needs. `have` is what the page actually grants.
    #[error("{access} at {addr:#x} denied: need {needed}, page is {have}")]
    Permission { addr: u64, len: usize, needed: Perms, have: Perms, access: Access },

    /// `div`/`idiv` with a zero divisor, or a quotient that does not fit the
    /// destination. On real hardware both raise the same #DE, which is why one
    /// stray `idiv` can crash a program two ways.
    #[error("division error (divide by zero or quotient overflow)")]
    DivideByZero,

    /// A decoded instruction this interpreter does not model.
    #[error("unsupported instruction: {0}")]
    UnsupportedInstruction(String),

    /// The bytes at `rip` could not be decoded at all.
    #[error("decode error: {0}")]
    Decode(String),

    /// A `syscall` with a number we do not emulate. The sandbox never reaches
    /// the host, so anything but `write`/`exit`/`exit_group` lands here.
    #[error("unimplemented syscall {0}")]
    UnknownSyscall(u64),

    /// Reserved for accesses that violate an alignment requirement. x86 permits
    /// unaligned integer access, so the interpreter does not currently raise
    /// this, but the variant exists so callers can pattern-match exhaustively
    /// against a stable API.
    #[error("misaligned access at {addr:#x} (needs {align}-byte alignment)")]
    Misaligned { addr: u64, align: u64 },
}
