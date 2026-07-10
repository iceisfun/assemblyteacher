+++
id = "calling-conventions"
title = "Calling Conventions"
order = 2
estimated_minutes = 40
objectives = [
  "Pass and return values by the System V AMD64 ABI without guessing",
  "Explain the red zone, and when a function may and may not use it",
  "State the 16-byte stack-alignment rule and why it exists",
  "Contrast the System V and Microsoft x64 conventions, and say why calling across them corrupts data",
]
prerequisites = ["the-stack", "addressing-modes"]

[[exercises]]
id = "q-first-arg"
kind = "quiz"
prompt = "Under the System V AMD64 ABI (Linux, macOS), which register holds the *first* integer or pointer argument to a function?"
choices = ["rax", "rdi", "rcx", "the top of the stack"]
answer = 1
explanation = "System V passes the first six integer arguments in rdi, rsi, rdx, rcx, r8, r9 — in that order. The return value comes back in rax."

[[exercises]]
id = "q-red-zone"
kind = "quiz"
prompt = "What is the 'red zone' in the System V ABI?"
choices = [
  "A region of the stack a leaf function may use without moving rsp",
  "The area where arguments beyond the sixth are passed",
  "Memory that is always zero at process start",
  "The gap between the stack and the heap",
]
answer = 0
explanation = "The 128 bytes *below* rsp are guaranteed not to be clobbered by signal handlers, so a leaf function (one that calls nothing) can use them for scratch without a `sub rsp` / `add rsp` pair. A function that makes a call cannot — the call would overwrite it."

[[exercises]]
id = "q-alignment"
kind = "quiz"
prompt = "The ABI requires rsp to be 16-byte aligned at the point of a `call`. Since `call` pushes an 8-byte return address, what is rsp's alignment on the first instruction of the callee?"
choices = ["16-byte aligned", "8 bytes off a 16-byte boundary", "4-byte aligned", "undefined"]
answer = 1
explanation = "Aligned-16 before the call, minus 8 for the pushed return address, leaves rsp ≡ 8 (mod 16) inside the callee. This is why a standard prologue's `push rbp` — another 8 bytes — brings it back to 16, and why SSE code that needs aligned loads cares about getting the prologue right."

[[exercises]]
id = "e-add3"
kind = "emulate"
prompt = "Write a function `add3` that returns the sum of its first three integer arguments, following the System V ABI. Call it with 10, 20 and 30, and halt with the result (60) in rax."
starter = """
    mov rdi, 10
    mov rsi, 20
    mov rdx, 30
    call add3
    hlt
add3:
    ; return rdi + rsi + rdx in rax
    ret
"""
solution = """
    mov rdi, 10
    mov rsi, 20
    mov rdx, 30
    call add3
    hlt
add3:
    lea rax, [rdi+rsi]
    add rax, rdx
    ret
"""
expect_registers = { rax = 60 }
hints = [
  "The three arguments arrive in rdi, rsi, rdx. The result must leave in rax.",
  "`lea rax, [rdi+rsi]` adds two of them without touching flags; then `add rax, rdx`.",
]

[[exercises]]
id = "e-max2"
kind = "emulate"
prompt = "Write a function `max2(a, b)` that returns the larger of its two *signed* arguments in rax. Call it with 7 and 3, and halt with 7 in rax. (A branchless solution using `cmovg` is elegant, but any correct one passes.)"
starter = """
    mov rdi, 7
    mov rsi, 3
    call max2
    hlt
max2:
    ; return the larger of rdi and rsi
    ret
"""
solution = """
    mov rdi, 7
    mov rsi, 3
    call max2
    hlt
max2:
    mov rax, rdi
    cmp rsi, rax
    cmovg rax, rsi
    ret
"""
expect_registers = { rax = 7 }
hints = [
  "Start with rax = rdi, then conditionally replace it with rsi if rsi is greater.",
  "`cmovg rax, rsi` copies rsi into rax only when the signed comparison says rsi > rax — no branch needed.",
]
+++

# Calling Conventions

A `call` instruction pushes a return address and jumps. It says nothing about
where the arguments are, where the result comes back, or which registers the
callee is allowed to destroy. Those rules are not in the hardware — they are an
*agreement*, the **Application Binary Interface**, and every function in a
program must follow the same one or the calls between them turn to noise.

There is more than one ABI. The two that matter for x86_64 are **System V
AMD64** (Linux, macOS, the BSDs) and **Microsoft x64** (Windows). They disagree
in exactly the ways that make a value passed under one and read under the other
come out wrong.

## System V AMD64: arguments in registers

The first six integer or pointer arguments go in registers, in this order:

```text
   arg1   arg2   arg3   arg4   arg5   arg6
   rdi    rsi    rdx    rcx    r8     r9
```

The return value comes back in **rax** (and rdx for the upper half of a 128-bit
result). Floating-point arguments use `xmm0`–`xmm7`, which are outside this
course's integer core, but the idea is identical.

So a call to `f(10, 20, 30)` is:

```asm
    mov edi, 10         ; arg1  (edi zero-extends to rdi)
    mov esi, 20         ; arg2
    mov edx, 30         ; arg3
    call f
    ; result is now in rax
```

Recognising this pattern is half of reading compiled code. A run of `mov`s into
`rdi`, `rsi`, `rdx`… immediately before a `call` *is* the argument list. When
you disassemble a function and see it read its inputs from `rdi` and `rsi`, you
are seeing its first two parameters.

Arguments seven and beyond are pushed onto the stack, right-to-left, so that the
seventh sits just above the return address. Most functions never need them.

## Who owns which register

A callee may freely clobber some registers and must preserve others. If it
wants to use a preserved ("callee-saved") register, it must save and restore it.

| callee-saved (preserve) | caller-saved (free to clobber) |
|-------------------------|--------------------------------|
| `rbx` `rbp` `r12`–`r15` `rsp` | `rax` `rcx` `rdx` `rsi` `rdi` `r8`–`r11` |

This is why a function that needs a lot of registers opens with `push rbx` /
`push r12` and closes by popping them: it borrowed callee-saved registers and
must hand them back untouched. And it is why, across a `call`, you cannot assume
`rcx` survives — if you need a value kept, put it in `rbx` or spill it to the
stack.

## The red zone

System V hands leaf functions a small gift. The **128 bytes immediately below
rsp** — the "red zone" — are guaranteed not to be disturbed by anything the
program does not do itself (the kernel promises not to place signal frames
there). So a function that calls nothing else can use that space as scratch
*without adjusting rsp at all*:

```asm
square:
    mov qword [rsp-8], rdi    ; scratch in the red zone, no `sub rsp` needed
    mov rax, qword [rsp-8]
    imul rax, rax
    ret
```

No prologue, no epilogue, no stack bookkeeping — two instructions saved on every
call. The moment the function makes a `call`, though, the red zone is off
limits: the call pushes a return address right into it. This is why you will see
the red zone used in tiny leaf helpers and never in functions that call out.
(Windows has no red zone; kernel code disables it with a compiler flag.)

## Stack alignment

The ABI requires that **rsp be 16-byte aligned at the moment a `call`
executes**. The reason is downstream: SSE and AVX instructions can load 16 bytes
at once, and the aligned form faults on a misaligned address, so the ABI
guarantees alignment rather than making every function check.

Follow the arithmetic. Aligned to 16 before the call; the `call` pushes 8 bytes
of return address; so on the callee's first instruction rsp is 8 *off* a 16-byte
boundary. A standard prologue's `push rbp` pushes another 8 and restores
alignment to 16 — which is one reason the prologue looks the way it does. If you
hand-write a function that calls into libc and forget this, the crash often lands
inside a perfectly innocent `movaps` deep in the library, and it is baffling
until you count the pushes.

## Microsoft x64: the same idea, different slots

Windows made different choices, and the differences are exactly the ones that
bite:

| | System V AMD64 | Microsoft x64 |
|---|---|---|
| first four int args | `rdi rsi rdx rcx` | `rcx rdx r8 r9` |
| args 5+ | on the stack | on the stack |
| return value | `rax` | `rax` |
| red zone | 128 bytes | none |
| shadow space | none | **32 bytes** the caller reserves |
| callee-saved | `rbx rbp r12–r15` | `rbx rbp rdi rsi r12–r15` |

Two traps stand out. First, the argument registers *overlap but are reordered*:
the first argument is `rdi` on Linux and `rcx` on Windows. Call a System V
function with Windows conventions and it reads its first argument out of the
wrong register — no crash, just a wrong number. Second, Windows requires the
caller to reserve **32 bytes of "shadow space"** on the stack above the return
address, even when all arguments went in registers, as a place the callee may
spill them. Forget it and the callee scribbles on your stack.

Note also that `rsi` and `rdi` are *callee-saved* on Windows but *caller-saved*
on Linux. The same instruction sequence is correct under one ABI and a
corruption bug under the other. This is why you cannot link objects built for
different ABIs, and why calling a DLL requires the compiler to know it is
Windows.

## Observe it

Assemble and run the two functions in the exercises below in the Playground,
open the register view, and step across the `call`. Watch the arguments go into
`rdi`/`rsi` before the call and the result appear in `rax` after the `ret`. Then
step *into* the callee and watch `rsp` drop by 8 as the return address is pushed
— the alignment arithmetic from above, happening in front of you.

## Key points

- The ABI, not the hardware, decides where arguments and results live. System V
  passes the first six integer args in `rdi, rsi, rdx, rcx, r8, r9` and returns
  in `rax`.
- Some registers are callee-saved and must be preserved across a call; the rest
  are fair game. A value you need kept across a `call` goes in a callee-saved
  register or on the stack.
- The red zone lets a leaf function use 128 bytes below `rsp` for free; a
  function that calls out cannot.
- `rsp` is 16-byte aligned at a `call`; miss it and aligned SSE loads fault far
  from the real mistake.
- Microsoft x64 uses `rcx, rdx, r8, r9`, has shadow space, no red zone, and a
  different callee-saved set — which is why the two ABIs cannot be mixed.
