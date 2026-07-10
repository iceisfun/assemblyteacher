+++
id = "checking-mitigations"
title = "Checking a Binary's Defenses"
order = 2
estimated_minutes = 30
objectives = [
  "Use this platform's Inspector to read the NX, PIE, RELRO, stack-canary, CFG and CET status of an uploaded ELF or PE",
  "Interpret a missing mitigation, and explain why the chain is only as strong as its weakest missing layer",
  "Distinguish format-specific mitigations: RELRO is ELF-only, CFG is a PE feature, and read an Inspector 'n/a' correctly",
  "Relate the Inspector's panel to what checksec and dumpbin report on the command line",
]
prerequisites = ["exploit-mitigations"]

[[exercises]]
id = "q-inspector-panel"
kind = "quiz"
prompt = "You upload an ELF to the Inspector and its mitigations panel shows: nx = yes, pie = yes, relro = partial, stackCanary = no, cet = partial. Which single finding most directly re-enables the classic 'overflow the return address and jump' attack that a canary is meant to catch?"
choices = [
  "relro = partial",
  "stackCanary = no",
  "pie = yes",
  "nx = yes",
]
answer = 1
explanation = "With no stack canary, a linear overflow can reach and overwrite the saved return address without tripping any epilogue check. The other fields are either protections that are present (nx, pie) or unrelated to the linear stack overwrite (relro guards the GOT). Note the partial CET: without a full shadow stack the hardware is not cross-checking `ret` either."

[[exercises]]
id = "q-elf-vs-pe"
kind = "quiz"
prompt = "The Inspector reports `relro = n/a` for a Windows PE and `cfg = n/a` for a Linux ELF. What does an 'n/a' mean here?"
choices = [
  "The Inspector failed to parse that field and you should re-upload",
  "The mitigation exists in that format but was disabled at build time",
  "The mitigation does not exist in that binary format at all — RELRO is an ELF concept, CFG is a PE feature — so its absence is expected, not a weakness",
  "The binary is corrupted",
]
answer = 2
explanation = "'n/a' means the concept does not apply to that format. RELRO is about the ELF GOT and dynamic-linking layout, so a PE has no RELRO to report. CFG (Control-Flow Guard) is a Microsoft/PE mechanism, so an ELF has no CFG. Reading 'n/a' as 'missing protection' would be a mistake — it is a fact about the format, and knowing which mitigations even exist per format is itself the lesson."

[[exercises]]
id = "q-chain-weakest"
kind = "quiz"
prompt = "A binary reports: nx = yes, canary = yes, pie = yes, full RELRO — a strong set. The attacker also has a bug that leaks a live code pointer and a separate buffer overflow. Why can the strong mitigation set still be defeated?"
choices = [
  "The leaked pointer defeats ASLR/PIE, and with addresses known, ROP defeats NX; a canary only guards contiguous stack writes, so the layers do not add up to safety",
  "NX = yes actually makes the binary less safe",
  "Full RELRO cancels out the stack canary",
  "PIE = yes means the canary is never checked",
]
answer = 0
explanation = "The panel shows which layers are present, not whether the attacker's specific bug slips between them. The info leak collapses ASLR/PIE by revealing a base address; ROP then reuses existing code, so NX is satisfied; the canary only catches linear stack overwrites and may be leaked or bypassed. Security is the weakest *missing or bypassable* layer combined with the bug on hand — which is why you read the whole panel, not one row."

[[exercises]]
id = "q-checksec"
kind = "quiz"
prompt = "On the command line, which tool is the conventional way to print the mitigation flags (NX, PIE, RELRO, canary) of an ELF, mirroring what the Inspector's panel shows?"
choices = [
  "checksec",
  "chmod",
  "strace",
  "ldd",
]
answer = 0
explanation = "`checksec` reads the ELF headers and reports NX, PIE, RELRO, stack canary and more — the same facts the Inspector surfaces in its panel. On Windows the equivalent details come from tools like `dumpbin /headers /loadconfig` or PE-analysis utilities. `ldd` lists shared-library dependencies, `strace` traces syscalls, and `chmod` changes permissions — none report mitigations."
+++

# Checking a Binary's Defenses

The previous lesson argued that a program's security is the security of its
weakest *missing* layer. That claim is only useful if you can find out which
layers a given binary actually has. You usually cannot ask the source — you have a
compiled artifact. Fortunately the mitigations leave fingerprints in the file:
NX is a program-header flag, PIE is an ELF type, a canary shows up as calls to
`__stack_chk_fail`, RELRO is a segment plus a dynamic-table entry, and so on. A
tool that parses those headers can tell you the whole posture without running
anything.

This platform ships one such tool: the **Inspector**.

## The Inspector's mitigations panel

Upload an ELF or a PE and the Inspector parses its headers and shows a
**mitigations panel** with one row per defense. It reports:

| Field           | Question it answers                                                    |
|-----------------|-----------------------------------------------------------------------|
| **nx**          | Are data pages (stack, heap) marked non-executable?                   |
| **pie**         | Is this a position-independent executable, so ASLR can randomize the main image? |
| **relro**       | Is the GOT made read-only after linking — `none`, `partial`, or `full`? |
| **stackCanary** | Was the binary compiled with stack-protector canaries?               |
| **cfg**         | (PE) Is Control-Flow Guard present?                                    |
| **cet**         | Is CET present — shadow stack and/or IBT (`endbr64` landing pads)?     |

Each row is a plain fact read from the file, and each maps directly onto a
mechanism from the previous lesson. `nx = yes` means the shellcode-injection path
is closed. `pie = yes` means ASLR covers the program's own code, not just its
libraries. `relro = full` means a GOT-overwrite will fault. `stackCanary = yes`
means a linear stack overflow trips `__stack_chk_fail` before `ret`. `cet` tells
you whether the hardware is cross-checking returns and indirect branches.

### What a missing row means

A `no` is not automatically a vulnerability — a program with no memory-safety bug
is safe with every mitigation off. What a `no` tells you is which *class of bug
becomes exploitable* if one exists:

- **nx = no** → an overflow can inject and run shellcode directly; no ROP needed.
- **pie = no** → the main image sits at a fixed address, so its functions and
  gadgets are at predictable locations even with ASLR on.
- **relro = partial / none** → the GOT is writable at runtime; a GOT-overwrite can
  redirect a library call without ever touching a return address.
- **stackCanary = no** → a linear stack overflow reaches the saved return address
  unnoticed.
- **cet = no** and **cfg = no** → nothing constrains a hijacked indirect call or a
  forged return, so ROP/JOP chains face one fewer obstacle.

## ELF vs PE: the panel is format-aware

Not every mitigation exists in every format, and the Inspector is honest about
that by showing **n/a** rather than `no`:

- **RELRO is an ELF concept.** It is defined in terms of the ELF GOT and the
  dynamic-linking layout. A Windows **PE** has no RELRO, so the Inspector shows
  `relro = n/a` for a PE. This is not a missing defense — PE handles its import
  table differently.
- **CFG is a PE feature.** Control-Flow Guard is a Microsoft mechanism recorded in
  the PE load-config directory. A Linux **ELF** has no CFG, so the Inspector shows
  `cfg = n/a` for an ELF. (The ELF world reaches similar goals through
  compiler-level CFI and CET/IBT instead.)

```text
             ELF                         PE (Windows)
   nx        yes                         nx        yes
   pie       yes                         pie       yes   (ASLR/DYNAMICBASE)
   relro     full                        relro     n/a   ← concept is ELF-only
   canary    yes                         canary    yes   (/GS)
   cfg       n/a  ← concept is PE-only   cfg       yes
   cet       partial                     cet       partial
```

Reading `n/a` as "unprotected" is a classic beginner mistake. It means *this
mitigation does not exist in this format* — and internalizing which mitigations
even belong to which format is a real part of understanding a binary. NX, PIE/ASLR
and stack canaries are cross-platform ideas (with different names — DEP, ASLR /
`DYNAMICBASE`, `/GS`); RELRO is ELF-only; CFG is PE-only; CET is a hardware
feature either format can opt into.

## The same facts on the command line

The Inspector is the convenient path, but the same fingerprints are readable with
standard tools, and it is worth knowing their names:

- **`checksec`** — the conventional Linux utility. Point it at an ELF and it prints
  NX, PIE, RELRO, stack canary, fortify status and more, in a table that lines up
  almost row-for-row with the Inspector's panel.
- **`readelf -l` / `readelf -d`** — shows the `GNU_STACK` program header (its
  missing execute flag *is* NX) and the `GNU_RELRO` segment plus `BIND_NOW`
  (together, full RELRO).
- **`nm` / `objdump -d`** — a reference to `__stack_chk_fail` is the giveaway that
  a canary is compiled in.
- **Windows:** `dumpbin /headers /loadconfig` and PE-analysis tools report
  `DYNAMICBASE` (ASLR), `NX_COMPAT` (DEP), Control-Flow Guard and CET fields.

The Inspector is doing what these tools do — parsing headers — and presenting it
uniformly across ELF and PE so you do not have to remember two toolchains.

## Why read the whole panel

The reason to look at every row, and not stop at the first reassuring `yes`, is
the compounding argument from the last lesson. A canary is little comfort if the
attacker has an info leak (to defeat ASLR) and a way to reach the return address
without a linear write. Full RELRO does nothing if the bug is a stack overflow and
there is no canary. NX is bypassed by ROP the moment addresses are known. The
panel does not tell you the binary is safe; it tells you *which attacks are cheap*
and which are expensive against this particular build. That is exactly the
information you want before deciding a bug is "just a crash" — or before shipping.

## Key points

- The Inspector parses an uploaded ELF or PE and reports **nx, pie, relro,
  stackCanary, cfg and cet** — the header fingerprints of each mitigation, no
  execution required.
- A `no` is not a bug by itself; it identifies the *class of bug* that would become
  exploitable, so read every row.
- **n/a** means the mitigation does not exist in that format: **RELRO is ELF-only,
  CFG is PE-only**. Do not read it as "missing."
- `checksec` (ELF) and `dumpbin` / PE tools (Windows) report the same facts on the
  command line; the Inspector just unifies them.
- Security is the weakest missing or bypassable layer combined with the attacker's
  actual bug — a strong panel with an info leak and an overflow is still
  exploitable.
