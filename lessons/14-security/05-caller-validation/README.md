# Caller Validation and Trusted Control Flow

A defensive-architecture lesson on how software can reason about *where execution
came from* using the return address `call` leaves on the stack. It covers
recovering the caller's RIP from inside a callee (leaf vs framed), why internal
routines assume specific callers and what invariants a bypass violates, the
exact-call-site / function-range / module validation strategies and their
trade-offs, the conceptual anti-cheat/DRM context, and — via a cross-reference to
the ROP lesson — why the immediate caller alone is not enough.

Its centre of gravity is the **limitations**: caller validation is a robustness
and diagnostics signal, not a security boundary, because the return address is
writable data in a single same-process trust domain. It closes on defense in
depth (CFI, stack integrity, state checks). The marquee exercise has the student
implement the check on the emulator: read `[rsp]`, compare to a whitelisted
call-site label, report the result.

Prerequisites: **The Stack and Call Frames**, **Calling Conventions**, and
**Return-Oriented Programming**.
