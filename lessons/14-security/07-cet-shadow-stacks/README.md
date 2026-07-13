# CET, Shadow Stacks, and Indirect Branch Tracking

A security lesson that closes the return-address thread after ROP, caller
validation, and tail calls. It explains Intel CET as two separate mechanisms:
shadow stacks for `ret` edges, and indirect branch tracking (IBT) for indirect
`call`/`jmp` edges.

The lesson traces how a shadow stack keeps a protected second copy of return
addresses, why a corrupted ordinary stack return faults before `ret` can jump to a
gadget, and why ordinary compiler tail calls remain valid: a tail call is a `jmp`,
not a `call`, so it does not create an extra return entry. It then introduces
`endbr64` as the IBT landing pad used to restrict indirect branch targets.

The emulator exercise simulates the core shadow-stack comparison: corrupt the
normal return slot at `[rsp]`, compare it against a protected expected return, and
halt with detection before executing `ret`.

Prerequisites: **Exploit Mitigations**, **Return-Oriented Programming**, and
**Tail Calls and the Vanishing Frame**.
