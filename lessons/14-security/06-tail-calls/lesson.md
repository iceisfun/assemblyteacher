+++
id = "tail-calls"
title = "Tail Calls and the Vanishing Frame"
order = 6
estimated_minutes = 35
objectives = [
  "Explain why a call in tail position can become a jump, and what happens to the caller's frame",
  "Trace how a tail call rewrites the return-address picture so the tail-called function sees its caller's caller",
  "Recognise a tail call in disassembly — a jmp into another function where a call/ret was expected",
  "Explain why tail calls flatten backtraces and quietly defeat return-address caller validation",
]
prerequisites = ["the-stack", "calling-conventions", "caller-validation"]

[[exercises]]
id = "q-why-jump"
kind = "quiz"
prompt = "A function's very last action is `return other(x);` — it returns another function's result and does nothing after. Why can the compiler turn that `call other` into a plain `jmp other`?"
choices = [
  "Because jmp is faster than call",
  "Because the current function has no work left to do after the call, so it does not need its own frame kept around; `other` can reuse it and return directly to the original caller",
  "Because other is a leaf function",
  "Because jmp preserves the flags and call does not",
]
answer = 1
explanation = "In *tail position* — the call is the last thing the function does, and its result is returned unchanged — the current frame is dead weight. There is nothing to come back to. So instead of `call other` (which would push a return address into this function, only for `other` to `ret` here and then immediately `ret` again to the real caller), the compiler emits `jmp other`. `other` runs on the existing frame and its final `ret` goes straight back to whoever called the original function. This is **tail-call optimization**, and it makes deep or mutual recursion run in constant stack space."

[[exercises]]
id = "q-vanished-caller"
kind = "quiz"
prompt = "`A` tail-calls `B` (via `jmp B`). Inside `B`, what return address is on top of the stack?"
choices = [
  "A return address back into A",
  "The return address of A's *own* caller — because the jmp pushed nothing, B inherits the stack A was entered with, so A has vanished from the return chain",
  "Zero, until B pushes one",
  "B's own address",
]
answer = 1
explanation = "`call A` pushed a return address into A's caller and jumped to A. A then did `jmp B` — a jump pushes nothing — so B is running on exactly the stack A received, whose top is still the return address into A's caller. B's `ret` therefore returns straight past A to A's caller. A is gone from the return chain: it left no frame and no return address. B's immediate 'caller' on the stack looks like A's caller."

[[exercises]]
id = "q-disasm-signature"
kind = "quiz"
prompt = "Reading a disassembly, what does a tail call typically look like?"
choices = [
  "A `call` immediately followed by `ret`",
  "A `jmp` whose target is *another function's* entry point (not a local label), often with the argument registers set up just before it, and no `ret` after",
  "A `ret` with no matching `call`",
  "A `push` of the return address by hand",
]
answer = 1
explanation = "Where you expect a `call target` and then a `ret`, a tail call shows a single `jmp target` — and the target is the start of a *different function*, not a label inside this one. The arguments are staged in the ABI registers just before the jump, exactly as for a call. Seeing a function *end* in a `jmp` to another function, with no `ret`, is the signature: the control flow leaves and never comes back here."

[[exercises]]
id = "q-defeats-validation"
kind = "quiz"
prompt = "The previous lesson validated a caller by checking the return address on the stack. How does a tail call in the calling path defeat that check?"
choices = [
  "It encrypts the return address",
  "The tail-called function never sees the intermediate function as its caller — the on-stack return address points at some earlier legitimate site — so a whitelist keyed on 'who called me' sees the wrong (yet legitimate-looking) caller and the intermediate is invisible",
  "It disables the stack",
  "It makes the return address null",
]
answer = 1
explanation = "Caller validation reads the return address to answer 'who called me?'. But a tail call erases the intermediate frame: the function you land in sees the return address of some *earlier* caller, not the function that tail-jumped to it. So the check is answered with the wrong caller — one that may well be on the whitelist — and the tail-calling function leaves no trace. It is not an attack; it is an ordinary optimization that happens to invalidate the assumption 'the return address names my immediate caller.' This is exactly the kind of single-signal fragility the caller-validation lesson warned about."

[[exercises]]
id = "e-vanishing-frame"
kind = "emulate"
prompt = "Demonstrate the vanishing frame. `outer` is entered by `call`, then tail-calls `inner` with `jmp inner`. Write `inner` so it proves its on-stack return address is `done` (outer's caller), not a return into outer: read `[rsp]`, compare to the address of `done`, and halt with `rax = 1` if they match."
starter = """
    call outer
done:
    hlt
outer:
    jmp inner          ; tail call: reuse outer's frame, push nothing
inner:
    ; show that [rsp] is the return address into `done`, not into outer
    ret
"""
solution = """
    call outer
done:
    hlt
outer:
    jmp inner
inner:
    mov rax, [rsp]
    lea rcx, [rip+done]
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
  "`outer` did `jmp inner`, which pushes nothing, so `inner` runs on the stack `outer` was entered with — its top is still the return address `call outer` pushed.",
  "That address is `done`. Compare `[rsp]` with `lea rcx, [rip+done]`; equal means outer's frame vanished and inner returns straight to outer's caller.",
]
+++

# Tail Calls and the Vanishing Frame

The last lesson leaned on a comfortable assumption: the return address on the
stack tells you who called you. Here is a completely ordinary compiler
optimization that quietly breaks it — no attacker required. It is worth knowing
in its own right, because it changes what a call graph looks like in a
disassembly and in a debugger, and because it is the cleanest example of why
reasoning from the return address alone is fragile.

## A call in tail position is really a jump

Look at a function whose final act is to return another function's result:

```c
int wrapper(int x) {
    setup(x);
    return worker(x);     // nothing happens after this
}
```

After `worker` returns, `wrapper` does nothing but hand that value back. Its frame
— its slot on the stack, its saved registers — is dead the instant `worker` is
called. So why create a return address that points back into `wrapper`, only for
`worker` to `ret` there and have `wrapper` immediately `ret` again?

The compiler doesn't. In **tail position** — the call is the last thing the
function does, and its result is returned unchanged — it emits a **jump**, not a
call:

```asm
wrapper:
    ...                     ; setup(x)
    ...                     ; arguments for worker into edi etc.
    jmp worker              ; TAIL CALL: reuse this frame, don't push a return
```

`worker` runs on `wrapper`'s frame, and its `ret` returns straight to whoever
called `wrapper`. This is **tail-call optimization (TCO)**. Its headline benefit
is that a tail-recursive function — or a chain of mutually tail-calling ones —
runs in **constant stack space**, turning what looks like deep recursion into
something closer to a loop. Functional languages depend on it; C compilers do it
whenever they can at higher optimization levels.

## The vanishing frame

Now follow the stack, because this is the part that matters. `A` is called
normally, then tail-calls `B`:

```text
   call A            jmp B  (inside A)         B running
   ─────────         ────────────────          ─────────
   push ret→A's      (pushes nothing)          rsp → ret→A's caller
   caller; jmp A     rsp unchanged                   (A left no frame,
   rsp → ret→A's                                      no return address)
        caller
```

`call A` pushed a return address into **A's caller**. A's `jmp B` pushes nothing,
so `B` is running on exactly the stack A was handed — whose top is still the
return address into A's caller. `B`'s eventual `ret` sails straight past A and
back to A's caller.

**A has vanished.** It left no frame and no return address. From `B`'s point of
view, its caller — the thing whose address sits at `[rsp]` — is *A's caller*, not
A. The exercise below makes this concrete: a function reached by a tail jump
finds its own caller's caller on the stack.

## Reading it in a disassembly

The signature is simple once you know it. Where you expect a `call` followed by a
`ret`, you see a single `jmp` — and its target is the **entry point of another
function**, not a label inside the current one, with the argument registers set
up just before it:

```asm
    mov edi, ...            ; stage the argument, as if for a call
    jmp some_other_function ; ... but jump, and never return here
```

A function that *ends* in a `jmp` to another function, with no `ret`, has
tail-called it. In a debugger the effect is that the backtrace is **one frame
short**: `wrapper` does not appear between its caller and `worker`, because
`wrapper` left nothing on the stack to reconstruct a frame from. A stack trace
that "skips" a function you know was called is very often TCO at work.

## Why it defeats caller validation

Put the two facts together and the trap from the previous lesson is obvious. That
lesson checked the return address to answer "who called me?". But a tail call
erases the intermediate caller: the function you land in sees the return address
of some *earlier* function, not the one that jumped to it.

So a helper that validates its caller against a whitelist can be reached through a
tail call and see a caller that is *wrong but legitimate-looking* — an address
that may itself be on the whitelist — while the function that actually
transferred control leaves no trace at all. Nothing malicious happened; the
compiler just optimized a call into a jump. That is precisely the single-signal
fragility the caller-validation lesson warned about: "the return address names my
immediate caller" is an assumption, and an everyday optimization violates it.
Defense in depth is the answer here too — validate state and invariants, not only
the address that happens to be on the stack.

## Key points

- A call in **tail position** (the last act, result returned unchanged) becomes a
  **jmp**: the current frame is dead, so the callee reuses it and returns to the
  original caller. This is what makes tail recursion run in constant stack space.
- After a tail call the intermediate function **vanishes** — no frame, no return
  address — so the tail-called function sees its *caller's caller* at `[rsp]`.
- In a disassembly a tail call is a **`jmp` to another function's entry** (not a
  local label), args staged, no `ret`; in a debugger it makes a backtrace one
  frame short.
- Because it rewrites the return-address chain, TCO **defeats return-address
  caller validation** — an ordinary optimization, not an attack, and a concrete
  reason to never trust a single signal.
