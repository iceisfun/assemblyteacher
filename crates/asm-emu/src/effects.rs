//! The record of what one instruction did.
//!
//! This is the reason the crate exists. A normal interpreter mutates state and
//! moves on; this one additionally emits an [`Effects`] describing every
//! observable change — which registers moved and from what to what, which bytes
//! of memory were read or written, how the flags shifted, and whether a syscall
//! happened. The browser UI replays these to animate the machine.

use crate::fault::Fault;
use crate::flags::Flags;
use asm_core::Insn;
use serde::Serialize;

/// A single register's before/after values. `reg` is the 64-bit architectural
/// name (`"rax"`), and the values are the *full* 64-bit contents even when the
/// instruction only named a narrower view — so the UI can show the ripple, e.g.
/// that writing `eax` also cleared the upper half of `rax`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RegWrite {
    pub reg: &'static str,
    pub before: u64,
    pub after: u64,
}

/// A range of memory that was written, with both images so a diff can be shown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MemWrite {
    pub addr: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

/// A range of memory that was read. Recorded so the UI can highlight the source
/// of a load, not just the destination.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MemRead {
    pub addr: u64,
    pub bytes: Vec<u8>,
}

/// A syscall that the interpreter serviced. `result` is what was placed in
/// `rax`; `args` are the six System V syscall registers at entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SyscallEvent {
    pub number: u64,
    pub name: String,
    pub args: [u64; 6],
    pub result: u64,
}

/// Everything one [`Cpu::step`](crate::Cpu::step) changed.
#[derive(Clone, Debug, Serialize)]
pub struct Effects {
    /// The instruction that ran, decoded — carries its own bytes and encoding.
    pub insn: Insn,
    pub rip_before: u64,
    pub rip_after: u64,
    pub reg_writes: Vec<RegWrite>,
    pub mem_writes: Vec<MemWrite>,
    pub mem_reads: Vec<MemRead>,
    pub flags_before: Flags,
    pub flags_after: Flags,
    pub syscall: Option<SyscallEvent>,
}

impl Effects {
    pub(crate) fn new(insn: Insn, rip_before: u64, flags: Flags) -> Effects {
        Effects {
            insn,
            rip_before,
            rip_after: rip_before,
            reg_writes: Vec::new(),
            mem_writes: Vec::new(),
            mem_reads: Vec::new(),
            flags_before: flags,
            flags_after: flags,
            syscall: None,
        }
    }
}

/// Why a [`run`](crate::Cpu::run) ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Stop {
    /// A `hlt` executed.
    Halted,
    /// An `exit`/`exit_group` syscall, carrying its status code.
    Exited(i32),
    /// The step budget ran out before the program finished.
    StepLimit,
    /// A fault ended the run. The program "crashed".
    Fault(Fault),
    /// An `int3` was hit, carrying the address it sat at. A debugger's bread
    /// and butter: one byte that traps.
    Breakpoint(u64),
}

/// The outcome of running many steps: why it stopped, how many steps it took,
/// and a bounded trace of the effects along the way.
#[derive(Clone, Debug, Serialize)]
pub struct Run {
    pub stop: Stop,
    pub steps: u64,
    pub trace: Vec<Effects>,
}
