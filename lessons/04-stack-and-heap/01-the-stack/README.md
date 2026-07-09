# The Stack and Call Frames

`push`/`pop`/`call`/`ret` reduced to their definitions (`call` = push rip + jmp;
`ret` = pop rip), the standard prologue and epilogue, and the frame-pointer
chain a debugger walks. Ends by deriving the stack buffer overflow from the fact
that a return address is ordinary writable memory — motivating every mitigation
in Part XIV.

- `examples/push_pop.asm` — `push` is `sub`+`mov`, proved side by side.
- `examples/frame.asm` — prologue, locals at `[rbp-8]`, `leave`, `ret`.
- `examples/factorial.asm` — recursion; the reference solution.
- `examples/unbalanced.asm` — a `push` without its `pop`, and the crash it causes.
