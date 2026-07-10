+++
id = "optimization-patterns"
title = "Optimization Patterns"
order = 1
estimated_minutes = 45
objectives = [
  "Explain why optimized code may not resemble the source line-for-line",
  "Recognise inlining, omitted frame pointers, strength reduction and tail calls",
  "Separate source-level variables from registers and stack slots chosen by allocation",
]
prerequisites = ["reading-compiled-code", "containers-in-memory"]

[[exercises]]
id = "q-inlining"
kind = "quiz"
prompt = "What does inlining do to a call in optimized code?"
choices = ["It replaces the call with the callee's body at the call site", "It moves the call to the heap", "It forces a stack frame", "It disables register allocation"]
answer = 0
explanation = "Inlining trades code size for fewer calls and more local optimization opportunities."

[[exercises]]
id = "q-frame-pointer"
kind = "quiz"
prompt = "Why might optimized x86-64 code not use `rbp` as a frame pointer?"
choices = ["The ABI forbids rbp", "The compiler can address locals from rsp or keep them in registers, freeing rbp for other use", "Optimized code has no stack", "Debuggers require it"]
answer = 1
explanation = "Frame-pointer omission makes `rbp` available as another general register when unwind info can describe the frame another way."

[[exercises]]
id = "e-strength-reduction"
kind = "emulate"
prompt = "Compute 9 * rdi for rdi = 7 using LEA-style arithmetic: rax = rdi + rdi*8, then halt with rax = 63."
starter = """
    mov rdi, 7
    ; compute rdi * 9 without imul
    hlt
"""
solution = """
    mov rdi, 7
    lea rax, [rdi + rdi*8]
    hlt
"""
expect_registers = { rax = 63 }
hints = ["9*x is x + 8*x, and the addressing unit can compute that without touching memory."]

[[exercises]]
id = "q-tail-call"
kind = "quiz"
prompt = "Why can a tail call become a jump?"
choices = ["The caller has no work left after the callee returns, so the callee can return directly to the caller's caller", "Jumps are always safer than calls", "The stack grows upward", "The function has no arguments"]
answer = 0
explanation = "A tail call can reuse the current frame instead of creating a new return address that would immediately be returned through."
+++

# Optimization Patterns

Unoptimized code is often a kindness to debuggers. Optimized code is a kindness
to the machine. It preserves observable behavior, not source shape.

## Inlining

Inlining copies a callee's body into the caller. The call instruction vanishes,
but the callee's logic remains. After inlining, constants propagate, branches
collapse, and temporary values may never touch memory.

This is why reverse engineers should identify behavior, not just function
boundaries.

## Register allocation

Source variables are names. Registers are storage choices. One variable can move
between registers, two variables can share a register at different times, and a
variable can disappear if its value is constant or unused.

Stack slots are no more sacred. They may be reused for unrelated lifetimes.

## Strength reduction

Compilers replace expensive operations with cheaper equivalent ones. Multiplying
by 9 may become `lea rax, [rdi+rdi*8]`. Multiplying by a power of two may become
a shift. Division by a constant may become multiply-by-magic plus shift.

The result is not hand-written weirdness. It is algebra plus instruction costs.

## Tail calls

If a function's final action is returning another function's result, the caller
does not need to keep its own frame around. The call can become a jump.

That saves stack space and changes backtraces: a source-level call may not leave
a normal return-address chain.

## Key points

- Optimization preserves behavior, not source layout.
- Inlining removes call boundaries and enables more simplification.
- Register allocation breaks the one-variable-one-location intuition.
- Strength reduction explains many surprising `lea`, shift and multiply patterns.
- Tail calls can remove frames from the call stack.
