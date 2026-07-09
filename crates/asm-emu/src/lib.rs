//! # asm-emu
//!
//! A step-by-step x86_64 interpreter that records *every* effect of each
//! instruction it runs.
//!
//! Where [`asm_core`] answers "what does this instruction *mean*?", this crate
//! answers "what did it *do*?" — precisely enough to drive a live visualisation
//! of the registers, flags, stack and memory as a program executes. Each
//! [`Cpu::step`] returns an [`Effects`] describing the register and memory
//! writes it made (with before/after values), the memory it read, how the flags
//! changed, and any syscall it serviced.
//!
//! ## Design commitments
//!
//! * **No host access.** The only syscalls implemented are `write` to fds 1/2
//!   (into an internal buffer) and `exit`/`exit_group`. Everything else faults.
//! * **No panics, ever.** The interpreter is fed untrusted bytes. Every memory
//!   access, divide and decode returns a [`Fault`]; there is no indexing that
//!   can go out of bounds and no `unwrap` on program-controlled data.
//! * **Hardware-faithful corner cases.** 32-bit writes zero-extend but 8/16-bit
//!   writes do not; `inc`/`dec` leave CF alone; a shift count that masks to zero
//!   changes no flags; `div` faults on both a zero divisor and a quotient that
//!   overflows. These are exactly the behaviours the lessons teach, so they are
//!   modelled rather than approximated.
//!
//! ## Example
//!
//! ```
//! use asm_emu::{Cpu, Stop};
//!
//! // xor eax, eax ; hlt   — the idiomatic "zero a register".
//! let code = [0x31, 0xc0, 0xf4];
//! let mut cpu = Cpu::with_code(&code, 0x1000);
//! let run = cpu.run(100);
//! assert_eq!(run.stop, Stop::Halted);
//! assert_eq!(cpu.regs.read_full(0), 0); // all of rax cleared
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod alu;
mod cpu;
mod effects;
mod fault;
mod flags;
mod mem;
mod regs;

pub use cpu::Cpu;
pub use effects::{Effects, MemRead, MemWrite, RegWrite, Run, Stop, SyscallEvent};
pub use fault::{Access, Fault};
pub use flags::Flags;
pub use mem::{Memory, Perms, Region};
pub use regs::Regs;
