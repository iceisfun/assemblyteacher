+++
id = "stepping-and-inspection"
title = "Stepping, Watchpoints and Inspection"
order = 2
estimated_minutes = 35
objectives = [
  "Distinguish stepping into a call from stepping over it, and explain how step-over is implemented",
  "Read and write registers and memory while a program is stopped",
  "Explain how a backtrace is walked via the saved-rbp chain, and what happens when frame pointers are omitted",
  "Explain how a conditional breakpoint works and why it can be slow",
]
prerequisites = ["how-debuggers-work"]

[[exercises]]
id = "q-step-over"
kind = "quiz"
prompt = "You are stopped on a `call fn` instruction and choose 'step over'. How is step-over typically implemented?"
choices = [
  "The debugger skips the call entirely and does not run `fn`",
  "The debugger sets a temporary breakpoint on the return address, lets the call run at full speed, and stops when it returns",
  "The debugger single-steps every instruction inside `fn` but hides them from you",
  "The debugger sets the Trap Flag and runs `fn` one instruction at a time",
]
answer = 1
explanation = "Step-over runs `fn` for real, at full speed, but plants a temporary breakpoint on the return address (the instruction right after the call). When `fn` returns and control reaches that address, the breakpoint fires and you stop — as if the call were a single step. Step-into, by contrast, single-steps and follows control into `fn`."

[[exercises]]
id = "q-step-into"
kind = "quiz"
prompt = "What is the difference between 'step into' and 'step over' at a `call`?"
choices = [
  "Step into runs the call; step over does not run it at all",
  "Step into follows control into the called function; step over runs the whole function and stops after it returns",
  "There is no difference for a `call`; they differ only on jumps",
  "Step over is faster because it uses hardware breakpoints for every instruction",
]
answer = 1
explanation = "Step into follows the flow of control into the callee and stops on its first instruction. Step over treats the entire call as one unit: it runs to completion and stops on the instruction after the call. Both actually execute the function — step over just doesn't stop inside it."

[[exercises]]
id = "q-backtrace"
kind = "quiz"
prompt = "With frame pointers in use, how does a debugger walk the call stack to print a backtrace?"
choices = [
  "It scans the whole stack for anything that looks like a code address",
  "It follows the saved-rbp chain: each frame's saved rbp points at the caller's frame, and next to it is the return address",
  "It reads a list the CPU keeps of every active function",
  "It re-runs the program and records each call",
]
answer = 1
explanation = "Each prologue pushes the caller's rbp and points rbp at it, so the saved rbp values form a linked list running back up the stack. The debugger follows that chain frame by frame, reading the return address stored next to each saved rbp. When frame pointers are omitted, that chain is gone and the debugger must use unwind tables (DWARF .eh_frame) instead."

[[exercises]]
id = "q-cond-bp"
kind = "quiz"
prompt = "A conditional breakpoint ('break here only when `i == 100`') can make a program run far slower even though it 'stops' only once. Why?"
choices = [
  "Conditional breakpoints disable the CPU cache",
  "The breakpoint fires every time the line is reached; the debugger stops, tests the condition, and silently continues if it is false — paying the full stop/continue cost on every hit",
  "The condition is recompiled from scratch on each pass",
  "The CPU evaluates the condition in microcode, which is slow",
]
answer = 1
explanation = "The hardware has no notion of the condition. The debugger plants an ordinary breakpoint that fires on every pass, then on each hit it stops the program, evaluates the condition itself, and if it is false does the whole restore-step-replant continue dance and runs on. A breakpoint hit thousands of times pays that cost thousands of times, even though it only shows you the one hit where the condition held."
+++

# Stepping, Watchpoints and Inspection

The previous lesson built the primitives: the `0xCC` software breakpoint, the
Trap Flag that steps, the debug registers that watch. This one is the practical
companion — how those primitives combine into the things you actually click on in
a debugger: stepping through a call, reading and writing state while stopped, the
backtrace, and conditional breakpoints. Nothing here is new hardware; it is all
the pieces you already have, arranged.

## Step into vs step over

You are stopped on a `call fn` instruction. There are two things you might mean by
"next":

- **Step into** — follow the flow of control *into* `fn` and stop on its first
  instruction. This is just a single step (set TF, run one instruction); because
  the instruction is a `call`, the CPU jumps into `fn`, and the trap lands you
  there.
- **Step over** — treat the whole call as one unit. Run `fn` to completion, at
  full native speed, and stop on the instruction *after* the call, as if the call
  were a single opcode.

Step over cannot just single-step, because that would land you inside `fn` — that
is step *into*. And it cannot skip the call, because `fn` might do real work the
rest of the program depends on. So it uses the breakpoint primitive:

```text
   0x401000  call fn          ← stopped here; "step over"
   0x401005  mov  rax, rbx    ← the return address: plant a temporary breakpoint here

   1. compute the return address (the instruction right after the call)
   2. plant a temporary 0xCC there
   3. continue at full speed; fn runs, deep call trees and all
   4. fn returns → control reaches 0x401005 → the temporary breakpoint fires
   5. remove the temporary breakpoint; you are stopped after the call
```

A `call rel32` is five bytes and starts with `e8`, so the return address the
debugger breaks on is simply the call's address plus five. Recognising `call`
instructions and their lengths is exactly why a debugger carries a small
disassembler.

Step over is therefore "run this call for real but wake me when it comes back". If
`fn` itself hits one of *your* breakpoints on the way, you still stop there — the
temporary breakpoint does not suppress the permanent ones.

## Inspecting and changing state while stopped

Once the program is frozen at a stop, the debugger has full access to its state,
because on Linux `ptrace` (and the equivalent Windows API) lets the debugger
process reach into the stopped one:

- **Registers.** The tracee's register file was saved by the kernel when it
  trapped. The debugger reads it to show you `rax`, `rsp`, `rip`, RFLAGS and the
  rest — and it can *write* it too. Setting `rip` to a different address is how you
  "jump to this line"; setting `rax` is how you fake a function's return value to
  test a branch.
- **Memory.** The debugger reads and writes the tracee's memory the same way it
  planted the `0xCC` in the first place. That is what a memory viewer shows: it
  reads a range of the stopped program's address space and lays out the bytes. It
  is also how you patch a variable live, or dump a buffer to see what a string
  parser actually received.

Reading memory is where a stop pays off: the whole point of freezing the program
*at* an instruction is that every byte is exactly as that instruction left it, and
nothing will move while you look.

> **Observe it in the Playground.** Plant an `int3` to stop the emulator mid-run,
> then open the memory viewer and the register panel. Everything is frozen at the
> stop, exactly as a debugger presents a real program — you are reading the same
> state the CPU had the instant it trapped.

## The call stack and the backtrace

A backtrace answers "how did I get here?" — the chain of callers, innermost first.
The debugger reconstructs it from the stack, and *how* depends on whether the
program kept frame pointers.

**With frame pointers.** Recall the prologue from the stack lesson: `push rbp`
then `mov rbp, rsp`. Every frame therefore stores the caller's `rbp` at a known
place, and right next to it (pushed by the `call`) is the return address. The
saved `rbp` values form a linked list running back up the stack:

```text
   rbp ─▶ ┌─────────────┐
          │  saved rbp  │──▶ ┌─────────────┐
          │  ret addr   │    │  saved rbp  │──▶ ... to main
          └─────────────┘    │  ret addr   │
            frame of foo      └─────────────┘
                                frame of bar
```

To walk it, the debugger reads the current `rbp` to find the saved `rbp` and the
return address stored beside it, then follows that saved `rbp` to the next frame,
and repeats until it reaches the top. Each return address, looked up in the
program's symbols, becomes one line of the backtrace. This is the same chain the
stack lesson pointed at — the backtrace is that linked list, printed.

**Without frame pointers.** Optimised builds usually omit the frame pointer to
free `rbp` as a general register, so there is no chain to follow. The compiler
instead emits **unwind tables** — DWARF `.eh_frame` on Linux — that describe, for
every code address, where the return address and saved registers are relative to
`rsp`. The debugger looks up the current `rip` in those tables to unwind one
frame, then repeats. It is more work and depends on the tables being present and
correct, which is why a stripped or badly built release binary can produce a
truncated or wrong backtrace, and why profilers often ask you to rebuild with
frame pointers.

## Conditional breakpoints

"Stop here, but only when `i == 100`." The hardware has no idea what `i` is, so the
debugger fakes the condition on top of an ordinary breakpoint:

```text
   1. plant a normal breakpoint at the line
   2. every time it fires:
        read the state, evaluate the condition (i == 100?)
        if TRUE  → stop and hand control to the user
        if FALSE → silently do the continue dance and run on
```

The important word is *every*. The breakpoint fires on **every** pass, not only
the interesting one. If that line runs a hundred thousand times before `i` reaches
100, the debugger stops, tests, and silently continues a hundred thousand times —
each one paying the full trap-into-kernel, evaluate, restore-step-replant,
continue cost from the previous lesson. You see a single stop, but the program may
crawl to get there.

This is why a conditional breakpoint on a hot line can feel like the program has
hung, and why, when the condition is "this data address got written", a
**hardware watchpoint** (a debug register trapping on write) is dramatically
faster: the silicon does the checking for free, and the debugger is only woken
when the write actually happens. Reach for a hardware watchpoint before a
condition-checked software breakpoint whenever you are watching data change — and
remember from the previous lesson that you only get four.

## Key points

- **Step into** single-steps and follows control into the callee. **Step over**
  plants a temporary breakpoint on the return address, runs the call at full
  speed, and stops when it returns.
- While stopped, the debugger reads and *writes* the tracee's registers and memory
  (via `ptrace` / the Windows API) — the same access it used to plant the
  breakpoint. That is what register panels and memory viewers show.
- A **backtrace** with frame pointers is the saved-`rbp` linked list walked frame
  by frame. Without frame pointers it is reconstructed from DWARF `.eh_frame`
  unwind tables, which is why release backtraces can be fragile.
- A **conditional breakpoint** fires on every pass; the debugger tests the
  condition and silently continues when it is false — so it can be very slow on a
  hot line. For watching data, a hardware watchpoint avoids that cost entirely.
