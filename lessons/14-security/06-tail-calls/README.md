# Tail Calls and the Vanishing Frame

A control-flow lesson on tail-call optimization, told as the everyday compiler
optimization that breaks the assumption the Caller Validation lesson leaned on.
It explains why a call in tail position becomes a `jmp` (the current frame is
dead, so the callee reuses it and returns to the original caller — constant stack
space for tail recursion), traces how that makes the intermediate function vanish
from the return chain so the tail-called function sees its *caller's caller* at
`[rsp]`, shows the disassembly signature (a `jmp` to another function's entry with
no `ret`, backtraces one frame short), and closes on why TCO quietly defeats
return-address caller validation — a concrete case of the single-signal fragility
the previous lesson warned about.

The marquee exercise demonstrates the vanishing frame on the emulator: `outer`
tail-jumps to `inner`, and `inner` proves its on-stack return address is outer's
caller, not a return into outer.

Prerequisites: **The Stack and Call Frames**, **Calling Conventions**, and
**Caller Validation and Trusted Control Flow**.
