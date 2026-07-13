# Architecture

Assembly Teacher is a Rust workspace of library crates, a thin web server over
them, and a browser frontend. This document explains how the pieces fit and, more
usefully, *why the boundaries fall where they do*.

## The shape

```
                         ┌─────────────────────────────┐
                         │   web/  (TypeScript + Vite)  │
                         │  memory-viewer, insn-explain │
                         │  register/stack views, Monaco│
                         └──────────────┬──────────────┘
                                        │  HTTP / JSON
                         ┌──────────────┴──────────────┐
                         │        server  (axum)        │
                         │  handlers = shape conversion │
                         └───┬───────┬────────┬─────────┘
                             │       │        │
              ┌──────────────┘       │        └──────────────┐
              │                      │                       │
      ┌───────┴───────┐     ┌────────┴────────┐     ┌────────┴────────┐
      │    lesson     │     │     asm-emu     │     │     binfmt      │
      │ load/validate │     │  interpret +    │     │  ELF64 / PE32+  │
      │    /grade     │     │  record effects │     │     parsers     │
      └───┬───────┬───┘     └────────┬────────┘     └─────────────────┘
          │       │                  │
          │       └──────────┐       │
          │                  ▼       ▼
          │           ┌──────────────────────┐
          └──────────▶│       asm-core        │
                      │ decode / encode / asm │
                      └──────────────────────┘
```

The dependency arrow runs one way, toward `asm-core`. The core crates
(`asm-core`, `asm-emu`, `binfmt`) depend on nothing but the standard library,
`serde` and `thiserror`. `lesson` builds on `asm-core` and `asm-emu`. `server`
builds on all four. Nothing points back up.

## The load-bearing principle: logic lives in crates, not in handlers

The server contains no assembly knowledge, no parsing, no emulation. Every
endpoint is a shape conversion: take JSON in, call a library, serialise the
result out. `crates/server/src/routes/asm.rs` is illustrative — the `explain`
handler is twenty lines, and all twenty are about JSON, because the actual
work (`Encoding::explain()`) is in `asm-core` where it can be unit-tested
without an HTTP client.

This is not tidiness for its own sake. It is what makes the material trustworthy.
The claim "this is how a `mov` is decoded" is only worth making if the code that
decodes it is the code the lesson points at, and that code is testable in
isolation. A handler that grew its own decoding would be a second, untested
implementation that could disagree with the first.

The practical test: if a handler starts wanting to *know* something about x86
— an operand width, an addressing rule, a flag — that knowledge belongs in a
crate. Push it down.

## The crates

### `asm-core` — the machine model

The integer core of x86_64: decode, encode, assemble, format, and the types
(`Reg`, `Operand`, `Insn`, `Mnemonic`, `Cond`) that every other crate speaks.

The decisive design choice was to write our own decoder rather than wrap
`iced-x86` or `capstone`. The cost is real — we cover a subset, and reaching full
ISA coverage is slow. The benefit is the whole point of the project: when a
lesson explains why `rsp` cannot be an index register, it links to the seven
lines in `decode.rs` that enforce it. The decoder *is* teaching material.

Two properties make it trustworthy:

- **Every decoded instruction keeps its encoding.** An `Insn` carries both the
  semantic view (mnemonic + operands) and the syntactic view (`Encoding`: which
  byte was the REX prefix, which was ModRM, which was the displacement).
  Reassembling the `Encoding`'s fields reproduces the input bytes exactly, and a
  test asserts it. This is what feeds the byte-by-byte breakdown in the UI.

- **It is validated against other people's tools.** A decoder checked only by
  its own encoder is checking its assumptions against themselves. `asm-core`'s
  differential tests assemble a corpus with `nasm` and compare, and disassemble
  a flat binary and compare *instruction boundaries* against `objdump` — the
  property that matters, because one wrong length desynchronises everything
  after it. (See "A note on differential tests" below.)

The assembler resolves branches by relaxation: it assumes every branch is short
and only ever lets branches grow. Growth is monotonic, so the fixed-point
iteration cannot oscillate and must terminate — the direction of the assumption
is the correctness argument.

### `asm-emu` — execution you can watch

A plain interpreter over `asm-core`'s decoder. Its distinguishing feature is not
speed but *observability*: `step()` returns an `Effects` value recording every
consequence of the instruction — each register write with its before and after,
each memory read and write, the flags on both sides. That record is what the
browser renders and what grades an `emulate` exercise.

The semantics it gets exactly right are the ones the lessons teach: 32-bit
writes zero-extend and narrower ones merge; `inc`/`dec` leave CF alone; `div`
faults on both a zero divisor and quotient overflow; `lea` never touches memory.
Memory is permission-checked, so a lesson can demonstrate a real fault on a
non-executable page. Syscalls are limited to `write` (fd 1 and 2) and `exit`;
everything else faults, on purpose — a lesson about syscalls must not be able to
open a file.

### `binfmt` — executables, laid open

First-party ELF64 and PE32+ parsers, no `goblin` or `object`, because this code
is the teaching material for the executable-formats chapters. It reports
sections, segments and their permissions, symbols, imports (including PE
delay-loads), exports (including forwarders), relocations, and a
security-mitigations summary.

Because it parses uploads, it must never panic on hostile input: one module owns
all byte indexing, offsets use `checked_add`, counts use `checked_mul`, and a
mutation-fuzz test throws thousands of corrupted files at it asserting `Ok` or
`Err` but never a crash.

### `lesson` — the framework that keeps the prose honest

Loads the `lessons/` tree, serves it (with answers stripped), grades
submissions, and — the important part — *validates* the curriculum: it runs
every exercise's own reference answer through the same grader a student's answer
goes through, and assembles every example. Documentation rots silently because
prose has no test suite; here it does. See [the lesson README](../lessons/README.md)
and [SKILL.md](../SKILL.md).

Grading is by effect, not text: `assemble` compares machine code, `emulate`
compares final machine state, so any correct solution passes.

### `server` — the interface

axum. Stateless. TLS is assumed to be terminated by a reverse proxy; the process
speaks plain HTTP and never reads a certificate. It refuses to start if the
curriculum does not validate. See [the API reference](api.md).

## Two decisions worth their own paragraph

**Machine words cross the wire as hex strings.** JSON numbers are IEEE doubles,
exact only to 2⁵³; a register holds any of 2⁶⁴ patterns. `mov rax, -1` as a JSON
number would arrive as `18446744073709552000`. A platform whose entire premise
is showing exact bytes cannot afford that, so register values, addresses in an
execution trace, and emulator state are `"0x..."` strings end to end, `bigint`
in the frontend. Executable addresses from `binfmt` stay JSON numbers, because
every address in a real image is far below 2⁵³ — the exception is deliberate and
documented in `api.md`.

**`/emu/step` is stateless.** The client holds the machine state and sends it
back to advance one instruction. No server sessions, no expiry, nothing to
garbage-collect — and any point in an execution can be captured in a URL and
shared. It costs a little bandwidth per step and buys a great deal of
simplicity.

## A note on differential tests

The tests that compare against `nasm` and `objdump` *skip themselves* when those
tools are absent, so `cargo test` still works on a bare machine. The consequence
is that a green run proves much less on a machine without the tools than on one
with them. `contrib/test.sh` prints a loud warning when a tool is missing, and
`contrib/Dockerfile` provides an environment where the tools are always present
and the differential tests always run. Treat the Docker run as the authoritative
one.

## Deployment

`contrib/build.sh` produces a single binary, `target/release/asmteacher`, and a
static bundle in `web/dist`. The binary serves both the API and the bundle:

```sh
asmteacher --listen 127.0.0.1:8080 --web web/dist --lessons lessons
```

Put a TLS-terminating reverse proxy in front of it. The application is not
coupled to TLS configuration by design.

## Curriculum

The curriculum is intended to grow lesson by lesson on the spine above. The
proposed parts — a floor to build on, not an exhaustive list:

1. **Computer Fundamentals** — binary, hex, signed integers, floating point,
   endianness, bytes and words, memory addressing, bit operations
2. **CPU Architecture** — registers, the instruction pointer, flags, the
   pipeline, decoding, the ALU, SIMD, cache
3. **Assembly Language** — syntax, operands, addressing, arithmetic, logic,
   branches, loops, calls, the stack
4. **Stack and Heap** — stack growth, heap allocation, locals, recursion,
   calling conventions, frames, alignment, unwinding
5. **Memory** — virtual memory, MMUs, page tables, permissions, COW, ASLR
6. **Processes** — loading, creation, threads, scheduling, synchronisation
7. **Executable Formats** — PE, ELF, Mach-O, sections, relocations, symbols,
   imports, exports
8. **Dynamic Linking** — shared libraries, GOT, PLT, IAT, delay loading,
   `LoadLibrary`/`GetProcAddress`, `dlopen`/`dlsym`
9. **Reverse Engineering** — disassembly, control and data flow, function
   discovery, calling-convention recovery, optimisation, decompilation
10. **Debugging** — software and hardware breakpoints, watchpoints, stepping,
    register/memory/stack/thread inspection
11. **Memory Structures** — arrays, strings, lists, trees, hash tables, and the
    real layouts of `std::vector`, `std::string`, `std::map`, Rust `Vec`/`String`/`HashMap`
12. **Compiler Behavior** — optimisation, inlining, register allocation,
    prologues, epilogues, ABI
13. **OS Interaction** — syscalls, kernel transitions, context switches,
    interrupts, exceptions, signals, page faults
14. **Security** — DEP/NX, ASLR, RELRO, stack cookies, CFG, CET, shadow stacks
15. **Advanced Topics** — JIT, dynamic instrumentation, binary rewriting,
    trampolines, hooks, virtualisation, emulation

Implemented so far: 39 lessons across Parts I–XV — computer fundamentals, the
stack and heap, processes, memory and ASLR, ELF, PE, dynamic linking, reverse
engineering, debugging internals, memory structures, compiler behavior, OS
interaction, exploit mitigations, and a capstone workflow. Each new lesson is a
directory and a few passing tests away.
