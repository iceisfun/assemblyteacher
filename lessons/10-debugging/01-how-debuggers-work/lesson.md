+++
id = "how-debuggers-work"
title = "How Debuggers Work"
order = 1
estimated_minutes = 50
objectives = [
  "Explain how a software breakpoint replaces one byte with 0xCC and restores it to continue",
  "Explain why INT3 is a deliberately one-byte instruction",
  "Describe single-stepping as the Trap Flag raising a trap after every instruction",
  "Describe hardware breakpoints and watchpoints via the debug registers, and why there are only four",
  "Say how the OS delivers a breakpoint to the debugger (SIGTRAP on Linux, a debug exception on Windows)",
]
prerequisites = ["first-instructions", "the-stack"]

[[exercises]]
id = "q-cc-first-byte"
kind = "quiz"
prompt = "To set a software breakpoint on an instruction at address `A`, what does a debugger actually do to the program's memory?"
choices = [
  "It deletes the instruction at `A` and shifts everything after it up",
  "It overwrites the first byte at `A` with `0xCC`, saving the original byte to restore later",
  "It writes `0xCC` over the whole instruction so nothing after it can run",
  "It sets a flag in the instruction's encoding that the CPU checks",
]
answer = 1
explanation = "A software breakpoint patches a single byte: the first byte of the target instruction becomes 0xCC (INT3). The original byte is saved so the debugger can put it back. Nothing is deleted or shifted — the surrounding bytes are untouched."

[[exercises]]
id = "q-int3-one-byte"
kind = "quiz"
prompt = "Why is it important that `int3` encodes as a single byte (`0xCC`) rather than two or more?"
choices = [
  "Because one-byte instructions run faster",
  "So it can replace the first byte of ANY instruction without disturbing the bytes that follow",
  "Because the CPU only reads one byte at a time",
  "So the debugger uses less memory to store breakpoints",
]
answer = 1
explanation = "Instructions vary in length. A one-byte trap can overwrite the first byte of a target of any length and leave every following byte in place, so when the original byte is restored the instruction is intact. A multi-byte trap could spill past a short instruction into the next one."

[[exercises]]
id = "q-trap-flag"
kind = "quiz"
prompt = "What does setting the Trap Flag (TF) in RFLAGS cause the CPU to do?"
choices = [
  "Halt the program until the debugger resumes it",
  "Raise a debug trap after every single instruction executes",
  "Trap only when the instruction pointer reaches a breakpoint",
  "Disable interrupts so the debugger has exclusive control",
]
answer = 1
explanation = "TF makes the CPU raise a debug trap after each instruction retires. That per-instruction trap is exactly the mechanism 'single-step' is built on: set TF, let one instruction run, catch the trap."

[[exercises]]
id = "q-hw-watchpoint"
kind = "quiz"
prompt = "A watchpoint — 'stop when this variable changes' — is most naturally implemented with which mechanism, and why?"
choices = [
  "A software breakpoint, because 0xCC works on any address",
  "The Trap Flag, because it traps on every instruction",
  "A hardware breakpoint in a debug register set to trap on writes to the variable's address, because it needs no code modification and can watch data",
  "A conditional breakpoint that re-reads the variable each step",
]
answer = 2
explanation = "Software breakpoints only work by rewriting executable code, so they cannot watch a data location. A debug register (DR0-DR3, armed via DR7) is watched by the silicon and can trap on a write to a data address — with no code modification at all. That is what a watchpoint is. And there are only four such registers, so beyond four you fall back to software."
+++

# How Debuggers Work

A debugger looks like magic the first time you use one. You click next to a line,
the program runs at full speed, and then it stops *exactly* there, frozen, with
every register and every byte of memory available to inspect. You can step one
instruction at a time. You can ask it to stop the moment a variable changes.

None of this is magic, and almost none of it is software cleverness. Nearly
every capability a debugger has is a hardware feature of the CPU, put there
deliberately so that a second program can supervise the first. This lesson works
from the processor up: the one-byte trap instruction, the flag that steps, the
four registers that watch, and how the operating system hands all of it to the
debugger.

## The problem a breakpoint solves

You want the CPU to run your program at full native speed — not interpreted, not
slowed down — and yet come to a dead stop when it reaches one particular
instruction. The instruction pointer is not something a normal program can watch;
it advances billions of times a second inside the silicon. Polling it from
outside would be hopelessly slow and would never catch the exact moment.

So the trick is inverted. Instead of watching for the CPU to arrive somewhere, we
plant something *at* that somewhere that makes the CPU stop itself.

## Software breakpoints: the `0xCC` trick

x86 has a one-byte instruction whose entire purpose is "stop here and tell the
debugger": `int3`, which encodes as the single byte `0xCC`.

To set a breakpoint on the instruction at address `A`, a debugger does something
that sounds violent but is completely reversible:

1. Read and **save** the byte currently at `A`.
2. **Overwrite** that one byte with `0xCC`.
3. Let the program run.

```text
   before:   ... 48 89 e5  ...        mov rbp, rsp   (the target instruction)
                  ^ address A

   armed:    ... CC 89 e5  ...        int3 ; 89 e5   (first byte replaced)
                  ^ saved: 0x48
```

Now the program runs at full speed until control reaches `A`. The CPU fetches
`0xCC`, executes `int3`, and raises a **breakpoint trap**. The program freezes and
the debugger wakes up. The CPU never saw `mov rbp, rsp` — it saw the trap that
took its place.

### Continuing past the breakpoint

The breakpoint is still sitting in memory as `0xCC`, and the real instruction —
`mov rbp, rsp` — has not run yet. To continue correctly the debugger performs a
careful little dance:

1. **Restore** the saved byte (`0x48`), so the memory again reads `mov rbp, rsp`.
2. Back the instruction pointer up by one (it advanced past the `0xCC`).
3. **Single-step** one instruction, so the real `mov rbp, rsp` executes exactly once.
4. **Re-plant** the `0xCC`, so the breakpoint survives for the next time execution
   reaches `A` — which matters enormously inside a loop.
5. Let the program run again.

That restore-step-replant sequence is why a breakpoint inside a tight loop can be
noticeably slower than the same loop without one: every single pass through it
pays for the dance.

### Why `int3` is deliberately one byte

This is the detail worth dwelling on. Instructions on x86 are variable-length: a
`push rbp` is one byte (`55`), a `mov rbp, rsp` is three (`48 89 e5`), a
`call rel32` is five and starts with `e8`. A breakpoint has to be able to land on
*any* of them.

Because `int3` is a single byte, overwriting the **first** byte of the target is
enough to guarantee the trap fires — the CPU decodes instructions front to back,
so it hits the `0xCC` before it can misread anything else — and, crucially, every
byte *after* the first is left exactly as it was. When the debugger restores that
one saved byte, the original instruction is whole again. There was never any risk
of clobbering the instruction that follows.

Contrast that with a hypothetical two-byte trap. To break on `push rbp` (one byte,
`55`) you would have to write your two bytes over `55` *and* the first byte of the
next instruction. Restoring cleanly would mean tracking and rebuilding two
instructions instead of one. The one-byte design sidesteps the whole problem.
There is a two-byte "undefined instruction" `ud2` (`0f 0b`) used to mark code that
must never execute, but as a breakpoint primitive the single byte is what you
want.

> **Try it in the Playground.** This project's emulator treats `int3` as a
> breakpoint stop. Assemble a short program, place an `int3` in the middle of it,
> and run: execution halts right at that byte, and you can inspect registers and
> memory at the stop — the same thing a debugger's software breakpoint does, minus
> the restore-step-replant machinery that a full debugger adds to *continue*.

## Single-stepping: the Trap Flag

Step 3 above quietly used "single-step". Where does *that* come from? Another
hardware feature, and a beautifully simple one.

`RFLAGS` — the flags register you already met with `cmp` and the conditional
jumps — has a bit called the **Trap Flag (TF)**. When TF is set, the CPU raises a
debug trap **after every single instruction retires**. Set TF, let one instruction
run, and the CPU immediately traps back to the debugger. That is the whole of
"step one instruction".

```text
   TF = 0:   run ... run ... run ... run           (normal, full speed)
   TF = 1:   run → trap, run → trap, run → trap     (one instruction at a time)
```

So "step" is not the debugger interpreting your code — the real CPU runs your
real instruction natively, then TF forces it to hand control back. It is the same
trap-and-return machinery as a breakpoint, just triggered by a flag instead of by
a planted byte. And as you saw, the debugger reuses it internally to step over its
own `0xCC` when continuing.

## Hardware breakpoints: the debug registers

Software breakpoints have two limits that come straight from how they work:

- They **modify the program's code**. If the code is in read-only memory, or the
  program checksums itself (some anti-tamper and malware does), a planted `0xCC`
  either fails or is detected.
- They can only break on **code**. You cannot plant `0xCC` "on a variable" — a
  variable is data, and executing data is not what you want.

The CPU solves both with a separate mechanism: the **debug registers**. There are
four address registers, **DR0, DR1, DR2, DR3**, plus a control register **DR7**
that arms them. You load an address into, say, DR0, configure DR7 to say what to
watch for at that address, and from then on the *silicon itself* compares every
access against those four addresses. No byte of the program is touched.

DR7 lets each of the four be configured to trap on:

| condition          | fires when the CPU...                         | typical use          |
|--------------------|-----------------------------------------------|----------------------|
| execute            | fetches an instruction at that address        | breakpoint on ROcode |
| write              | writes to that address                        | **watchpoint**       |
| read or write      | reads or writes that address                  | watch reads too      |

Because nothing is modified, hardware breakpoints work on read-only code, on
self-checking code, and — the big one — on **data**.

### Watchpoints are hardware breakpoints on data

"Break when this variable changes" — a **watchpoint** — is exactly a hardware
breakpoint set to trap on a *write* to the variable's address. You put the
variable's address in DR0 and tell DR7 "trap on write, this many bytes". Now the
moment any instruction, anywhere in the program, stores to that address, the CPU
traps and the debugger stops you — and can show you precisely which instruction
did the write.

There is no software equivalent that is cheap. Without debug registers, the only
way to catch a write to an arbitrary address is to single-step the *entire*
program and re-read the location after every instruction, which can slow it down
by a factor of thousands. That contrast is the whole reason the debug registers
exist.

### Why only four

There are exactly **four** address debug registers, so a program can have at most
four hardware breakpoints or watchpoints armed at once. That is a fixed cost in
the silicon — comparators that check every instruction fetch and every memory
access against a stored address are not free, and four was the number Intel
committed to.

When you ask for a fifth, the debugger has to fall back to a software technique:
a `0xCC` if it is a code breakpoint, or the brutally slow single-step-and-check
loop if it is a watchpoint on data. This is why debuggers quietly distinguish the
two, and why setting many watchpoints can make a program crawl while setting a few
does not: the first four ride the hardware, and the rest do not. It is also why
`gdb` sometimes warns "cannot set hardware watchpoint" — you have run out of debug
registers.

## How the OS delivers it to the debugger

One piece remains. The `int3` trap, the TF trap, and the debug-register trap all
happen inside the CPU, in whatever program is running — not in the debugger. Some
software has to route that event across to the debugger process. That software is
the operating system.

On both major platforms the CPU first vectors the trap into the kernel, and the
kernel then delivers it to whoever registered as the debugger:

- **Linux:** the kernel turns the debug trap into a **`SIGTRAP`** signal delivered
  to the traced process. A debugger attaches with the `ptrace` system call, which
  makes the kernel stop the tracee and report — via `wait` — every `SIGTRAP` (and
  every other signal) to the debugger instead of to the program. `ptrace` is also
  how the debugger reads and writes the tracee's registers and memory, including
  poking in that `0xCC` and pulling it back out.
- **Windows:** the CPU raises a **debug exception** (`STATUS_BREAKPOINT` for
  `int3`, `STATUS_SINGLE_STEP` for a TF or debug-register trap), and the kernel
  reports it to the attached debugger through the debug event / exception
  dispatch mechanism, which the debugger services with the Debugging API.

Either way the shape is identical: **CPU traps → kernel catches it → kernel hands
it to the debugger, freezing the target until the debugger says continue.** The
hardware provides the stop; the OS provides the delivery; the debugger provides
the intelligence about what to do next.

## Putting it together

Setting a breakpoint and hitting it, end to end:

```text
   1. debugger reads the byte at A, saves it, writes 0xCC        (ptrace / WriteMemory)
   2. debugger tells the OS "continue"
   3. program runs at full speed ... reaches A ... executes int3
   4. CPU raises breakpoint trap  →  kernel  →  SIGTRAP / debug exception
   5. OS freezes the program, wakes the debugger
   6. debugger inspects registers & memory, shows you the stop
   7. to continue: restore saved byte, back up rip, set TF, step once,
      re-plant 0xCC, clear TF, continue          (the restore-step-replant dance)
```

Every numbered step is one of the primitives above. There is nothing else in it.
A debugger is a program that arranges these hardware traps and interprets them for
a human — patching one byte here, flipping one flag there, and loading four
addresses into registers the CPU watches on its behalf.

## Key points

- A **software breakpoint** overwrites the *first byte* of the target instruction
  with `0xCC` (`int3`), saving the original. Hitting it raises a breakpoint trap.
- `int3` is **one byte on purpose**: it can replace the first byte of any
  instruction, of any length, without disturbing the bytes that follow — so
  restoring the saved byte always rebuilds the original instruction.
- To continue, the debugger does the **restore-step-replant** dance: put the real
  byte back, single-step it once, then re-plant `0xCC` for next time.
- **Single-stepping** is the **Trap Flag (TF)** in RFLAGS: with TF set the CPU
  traps after every instruction. "Step" and the continue-dance both ride it.
- **Hardware breakpoints** use the four debug registers **DR0-DR3** (armed by
  **DR7**). They modify no code, so they work on read-only/self-checking code and
  on *data*. A **watchpoint** is a hardware breakpoint that traps on a *write*.
- There are only **four** debug registers, so beyond four you fall back to software
  breakpoints or slow single-step-and-check.
- The OS delivers the trap: **`SIGTRAP` via `ptrace` on Linux**, a **debug
  exception on Windows**. CPU traps, kernel catches, debugger decides.
