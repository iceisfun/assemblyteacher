# How Debuggers Work

The flagship debugging lesson, built from the processor up. Every debugger
capability reduced to the hardware primitive behind it:

- **Software breakpoints** — overwrite the first byte of the target instruction
  with `0xCC` (`int3`), save the original, and do the restore-step-replant dance
  to continue. Why `int3` is deliberately one byte.
- **Single-stepping** — the Trap Flag (TF) in RFLAGS, which traps after every
  instruction.
- **Hardware breakpoints and watchpoints** — the four debug registers DR0-DR3
  armed by DR7, watched by the silicon, working on read-only code and on data. A
  watchpoint is a hardware breakpoint that traps on a write.
- **Why there are only four**, and when you fall back to software.
- **OS delivery** — `SIGTRAP` via `ptrace` on Linux, a debug exception on Windows.

References the Playground: the emulator treats `int3` as a breakpoint stop, so a
student can plant `0xcc` and watch execution halt.

Four quiz exercises; no runnable examples (all assembly is in rendered prose).
