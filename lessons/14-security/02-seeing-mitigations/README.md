# Checking a Binary's Defenses

A short, practical companion to the mitigations lesson: how to read which defenses
a compiled binary was built with. Centers on this platform's **Inspector**, which
parses an uploaded ELF or PE and reports nx, pie, relro, stackCanary, cfg and cet,
and relates the panel to command-line tools (`checksec`, `readelf`, `dumpbin`).

- How to interpret each panel row and what a missing mitigation exposes.
- ELF vs PE: RELRO is ELF-only, CFG is PE-only, and an Inspector `n/a` means the
  concept does not exist in that format — not that a protection is missing.
- Why the whole chain matters: a canary is little help against an info leak plus
  an NX bypass.

Four `quiz` exercises. No `examples/*.asm` — all illustration is in fenced blocks.
