# Your Code vs the Compiler's

Answers the common question "why does my three-line function disassemble into
twenty-five instructions?" by teaching the reader to separate their own logic
from the scaffolding the toolchain adds: ABI bookkeeping (frame prologue, stack
alignment, shadow space, callee-saved saves), security instrumentation
(`endbr64` CET landing pad, the `fs:[0x28]` / `__security_cookie` stack canary,
CFG guard thunks), and runtime plumbing (PLT/IAT thunks, CRT startup frames,
`__chkstk` probes). The method is "subtract the scaffolding, then read the
residue with the previous lesson's field guide," and it closes on compiler
fingerprints (GCC/Clang vs MSVC) so a reader knows what boilerplate to expect.

It is the complement to Reading Compiled Code: that lesson teaches the shapes of
your logic, this one teaches the shapes of everything around it. The security
instrumentation it names (canaries, CET, CFG) is the reading-side view of the
Exploit Mitigations lesson in Part XIV, but it explains each inline so it stands
alone.

Prerequisites: **Reading Compiled Code** and **Calling Conventions**.
