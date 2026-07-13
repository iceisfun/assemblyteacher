+++
id = "caller-validation"
title = "Caller Validation and Trusted Control Flow"
order = 5
estimated_minutes = 40
objectives = [
  "Recover the caller's return address from inside a callee, and say where it sits for a leaf vs a framed function",
  "Explain why an internal routine may assume specific callers, and what invariants a bypass can violate",
  "Compare exact-call-site, function-range and module validation, and their trade-offs",
  "State clearly why caller validation is a robustness signal, not a security boundary, and how it fits a defense-in-depth design",
]
prerequisites = ["the-stack", "calling-conventions", "return-oriented-programming"]

[[exercises]]
id = "q-where-is-rip"
kind = "quiz"
prompt = "A function is entered by `call`. On the callee's *first* instruction, before it touches the stack, where is the caller's return address?"
choices = [
  "In `rip`",
  "At `[rsp]` — `call` pushed it, so it is the value on top of the stack",
  "In `rax`",
  "It is not stored anywhere the callee can read",
]
answer = 1
explanation = "`call` pushes the return address (the instruction after the call) and jumps. So on entry, before the callee pushes anything, the return address is exactly the 8 bytes at `[rsp]`. After a `push rbp` prologue it moves to `[rsp+8]`, and with a frame pointer it is `[rbp+8]`. It is ordinary memory, and the callee can read it like any other."

[[exercises]]
id = "q-range-vs-exact"
kind = "quiz"
prompt = "A helper validates its caller by checking the return address. Why is checking that the address falls within a known *function's range* often preferable to matching one *exact* call-site address?"
choices = [
  "Range checks are impossible to bypass",
  "An exact address breaks whenever the caller is recompiled and its instructions move; a range survives edits inside the calling function, so it needs less maintenance",
  "Exact matching is slower at runtime",
  "Range checks do not need the return address",
]
answer = 1
explanation = "An exact call-site address is brittle: recompile the caller, add a line above the call, and the address shifts, so the check must be updated. Validating that the return address lies within the *bounds* of the expected function tolerates instructions moving around inside it. It is coarser — anything in that function now passes — but far easier to keep working across builds."

[[exercises]]
id = "q-invariant"
kind = "quiz"
prompt = "An internal `commit_transaction` helper is only ever called from `run_transaction`, which has already taken a lock and validated state. What is the real risk if some other code path reaches `commit_transaction` directly?"
choices = [
  "The CPU will refuse to execute it",
  "The helper runs with its assumed invariants unmet — the lock may not be held and the state may be invalid — so it corrupts data while believing everything was checked upstream",
  "Nothing; every function is self-contained",
  "The return address will be null",
]
answer = 1
explanation = "Internal helpers routinely assume the work their sole caller already did: a lock is held, thread-local context is initialised, an object's lifetime is verified, parameters are in range. Reaching the helper outside that path leaves those invariants unmet, and the helper proceeds as if they held. Validating the caller is one way to notice 'I was reached from somewhere that cannot have established my preconditions.'"

[[exercises]]
id = "q-not-a-boundary"
kind = "quiz"
prompt = "Why is caller validation NOT a security boundary against code running in the same process?"
choices = [
  "Because it is too slow to run in production",
  "Because the return address is just data on a writable stack, and same-process code shares one trust domain — it can arrange the stack to present any return address it likes, so the check is a signal, not a guarantee",
  "Because the CPU encrypts the stack",
  "Because only the kernel can read the stack",
]
answer = 1
explanation = "Everything the check relies on — the return address — is writable memory under the control of any code in the same address space, which all runs at the same privilege. Such code can place a legitimate-looking return address on the stack before reaching the helper. So caller validation raises the cost of *accidental* misuse and catches *unsophisticated* unexpected callers, but it cannot be trusted as a wall against same-process code. It is one layer among many, never the boundary itself."

[[exercises]]
id = "e-validate-caller"
kind = "emulate"
prompt = "Write `internal` so it validates its caller. Read the return address from the stack, compare it to the address of `site` (the one whitelisted call site), and leave `rax = 1` on a match (else `rax = 0`) before returning. The program halts at `site`, so a correct check ends with `rax = 1`."
starter = """
    call internal
site:
    hlt
internal:
    ; 1) read the caller's return RIP from [rsp] into rax
    ; 2) load the address of `site` (the allowed caller) into rcx
    ; 3) compare; set rax = 1 if equal, else rax = 0
    ret
"""
solution = """
    call internal
site:
    hlt
internal:
    mov rax, [rsp]
    lea rcx, [rip+site]
    cmp rax, rcx
    jne bad
    mov rax, 1
    ret
bad:
    xor eax, eax
    ret
"""
expect_registers = { rax = 1 }
hints = [
  "On `internal`'s first instruction the return address is at `[rsp]` — `call` just pushed it.",
  "`lea rcx, [rip+site]` computes the runtime address of the `site` label; the return address `call` pushed is exactly that address.",
  "Compare with `cmp`, branch with `jne`, and remember `ret` returns to `site`, where the program halts with your `rax`.",
]
+++

# Caller Validation and Trusted Control Flow

Most of this course reads control flow forwards: a `call` goes *there*, a `ret`
comes *back*. This lesson turns around and looks the other way. When a function
is running, can it tell **where it was called from** — and why would it want to?

The answer to the first question you already have the pieces for: the `call`
instruction left a breadcrumb on the stack. The second question is the
interesting one. Software sometimes needs to reason about *how execution arrived*
at a routine — to protect an internal API, to notice misuse, to feed diagnostics.
This is **caller validation**, and understanding it means understanding both what
it can do and, just as importantly, what it cannot.

> **Read this first.** Caller validation is a technique for *robustness and
> diagnostics*, not a security boundary. Everything it inspects is ordinary
> memory shared by all code in the process, which runs in one trust domain. Treat
> it as one signal among many in a defense-in-depth design — never as a wall.

## The breadcrumb: the return address

Recall what `call` does. It pushes the address of the next instruction — the
**return address** — and jumps to the target. `ret` later pops that address back
into `rip`. So immediately after a `call`, the stack looks like this:

```text
        higher addresses
        ┌────────────────────┐
        │  caller's frame    │
        ├────────────────────┤
  rsp → │  return RIP        │  ← the address of the instruction after `call`
        └────────────────────┘
        lower addresses
```

That return RIP is not special memory. It is a 64-bit value sitting on the
stack, and the callee can read it exactly like any other. `ret` trusts it
completely — which is the whole basis of both this lesson and the last one.

## Reading your caller's RIP

On a callee's very first instruction, before it has pushed anything, the return
address is on top of the stack:

```asm
internal:
    mov rax, [rsp]      ; rax = the caller's return address
```

Where it lives shifts as the function sets up. A standard `push rbp` prologue
pushes 8 bytes, so the return address moves to `[rsp+8]`; once `rbp` is
established it is at `[rbp+8]`. A **leaf** function that builds no frame keeps it
at `[rsp]`. (Such a leaf may still use the System V *red zone* — the 128 bytes
*below* `rsp` — for scratch without moving `rsp`, but that space is below the
return address, so `[rsp]` is unaffected.) Reading it is trivial; the subtlety is
entirely in what you *do* with the value.

The value you just read *is* a `rip` — an address in the code — even though you
never name `rip` as an operand:

:::register rip

## What the value can tell you

An internal routine often has, in practice, only a handful of legitimate
callers. A VM helper, an allocator fast-path, a renderer's internal step, a
reference-count adjust, a scheduler internal — these are not public API. They are
meant to be reached from specific places that have already done specific work.

So the helper can compare the return RIP it read against what it *expects*:

```text
    caller_rip = [rsp]
    if caller_rip is one of the addresses we trust:
        proceed
    else:
        refuse / log / take the slow, fully-checked path
```

There are three common ways to define "the addresses we trust", trading
precision for durability:

| strategy | checks that the return RIP… | precise? | survives a recompile? |
|---|---|---|---|
| **exact call-site** | equals one specific address | very | no — the address moves |
| **function-range** | lies within a known function's bounds | medium | mostly |
| **module** | belongs to the expected image (exe/DLL) | coarse | yes |

Exact matching pins one instruction and breaks the moment the caller is rebuilt.
Function-range tolerates edits inside the caller. Module validation only asks
"did this come from our own code at all?" — useful against calls arriving from an
injected or unexpected library, useless at distinguishing internal callers from
each other. Real systems pick per situation, and often combine them.

The exercise at the end implements the exact-call-site form: read `[rsp]`, compare
it to the address of the one whitelisted site, and report the result.

## Why internal APIs care

The reason a helper wants to know its caller is that it usually *assumes* things
its caller established:

- a mutex is already held,
- the scheduler's state is consistent,
- thread-local context is initialised,
- an object's lifetime has been verified,
- parameters have been range-checked upstream.

Bypass the intended call graph and those assumptions are silently false. The
helper does its job believing the groundwork was laid, and corrupts state
instead.

```text
   expected                         unexpected
   ────────                         ──────────
   main                             (some other thread / component)
    └ update                              └ internal()      ← invariants NOT established
        └ physics
            └ internal()   ← lock held, state valid
```

Caller validation is a way for the helper to *notice* the second shape: "I was
reached from a place that cannot have set up my preconditions."

## Unexpected callers, in practice

"Reached from somewhere unexpected" is not always an attack. Software in the
field meets execution from outside its original assumptions all the time:

- debuggers and profilers redirecting or single-stepping,
- instrumentation and tracing frameworks,
- plugin and extension architectures,
- injected libraries,
- and plain internal misuse — a new call added without the invariant work.

The helper cannot tell *intent* from a return address. It can only tell that the
path differs from the design, and decide whether to proceed, refuse, or log. This
is why the technique shows up in **anti-cheat, DRM, tamper detection and hardened
applications**: internal-only routines that guard state transitions want to
notice when they are reached abnormally, and to emit telemetry when they are. It
bears repeating that none of that is a complete anti-cheat or anti-tamper
solution — it is one detector feeding a larger system.

## Springboarding, and why the immediate caller is not enough

Here is the crack in relying on the return address alone. The previous lesson
introduced **gadgets** — short sequences of existing instructions, usually ending
in `ret`, that were never meant to be an entry point but exist all through
compiled code. Reuse of existing code means execution can be steered to a
sensitive routine *through* a location that looks entirely legitimate: the return
address on the stack can name a trusted call site while the control flow that got
there was anything but normal.

So a check that only inspects the immediate caller can be satisfied by a return
address that was arranged to look right. That is precisely why real defenses do
not stop at the immediate caller — they add stack integrity, control-flow
integrity, and hardware help on top. We keep this architectural, not
exploit-oriented: the takeaway is *why one signal is insufficient*, which leads
straight to the next two sections.

## Limitations

Say it plainly, because it is the most important part of the lesson:

**Caller validation is not a security boundary.**

- The return address is **data** on a **writable** stack.
- Same-process code shares **one trust domain** — it runs at the same privilege
  as the code doing the checking, and can shape the stack it sees.
- A legitimate call site can itself be compromised, so "the caller looks correct"
  can be true and meaningless at once.
- Any single signal can be satisfied; a design that *trusts* one signal has a
  single point of failure.

What it genuinely buys you is real but modest: it catches accidental misuse and
broken invariants during development, it raises the cost of *casual* unexpected
callers, and it produces useful diagnostics when the control flow is abnormal.
Those are robustness and observability wins — not a wall.

## Defense in depth

Because no single check is sufficient, hardened software layers independent ones,
so that defeating any one is not enough:

- **validate object and thread-local state**, not just the caller;
- **module ownership** — did this come from our own image at all?
- **control-flow integrity (CFI)** — indirect transfers must reach valid targets;
- **stack integrity** — e.g. a CET shadow stack the attacker cannot forge;
- **invariant checks and assertions** at the boundaries;
- **hardware-assisted protections** where available.

Caller validation sits inside that stack as a cheap, useful, *fallible* layer.
Understanding both halves — that software *can* reason about how execution
arrived at a function, and that this reasoning is a signal rather than a
guarantee — is the point. The following lessons on CFI and OS mitigations pick up
the layers that a return-address check alone cannot provide.

## Key points

- `call` leaves the **return RIP** at `[rsp]`; a callee can read it there (leaf),
  at `[rsp+8]` after a `push rbp`, or at `[rbp+8]` with a frame pointer.
- A helper can **validate its caller** by comparing that address to trusted
  call-sites, function ranges, or a whole module — precision traded for
  durability across recompiles.
- Internal routines assume work their callers did (locks, state, lifetimes);
  validation helps notice a path that never established those invariants.
- It is **not a security boundary**: the return address is writable data in a
  single same-process trust domain, so caller validation is one fallible layer of
  a **defense-in-depth** design, alongside CFI, stack integrity and state checks.
