# Return-Oriented Programming

A deeper look at the technique the Exploit Mitigations lesson introduces when NX
forces code reuse. It defines a gadget precisely (a short existing sequence
ending in `ret`, chained because `ret` is `pop rip`), covers the clobbering
discipline (every register and stack slot a gadget touches must be accounted
for), explains why variable-length/unaligned decoding exposes far more gadgets
than the compiler emitted (tying back to the AOB lesson), names the variants
(ret2libc, JOP, COP), and — the core payload — maps each mitigation to the step
of a reuse attack it breaks, singling out the CET shadow stack (against
`ret`-chaining) and IBT/`endbr64` (against jump-oriented reuse).

It is a conceptual, defense-oriented treatment: enough to recognise the pattern
in a disassembly and understand why a hardened binary is a hard target. It does
not contain exploit or chain-construction recipes.

Prerequisites: **Exploit Mitigations**, **The Stack and Call Frames**, and
**AOB Scanning** (for the variable-length-decoding connection).
