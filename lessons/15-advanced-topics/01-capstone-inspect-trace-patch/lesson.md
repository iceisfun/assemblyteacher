+++
id = "capstone-inspect-trace-patch"
title = "Capstone: Inspect, Trace, Patch"
order = 1
estimated_minutes = 60
objectives = [
  "Apply the Inspector, Playground and debugger mental model to one binary-analysis workflow",
  "Trace a decision from ELF layout through dynamic calls and runtime addresses",
  "Patch a branch and then evaluate which mitigations did and did not matter",
]
prerequisites = ["checking-mitigations", "syscalls-signals", "optimization-patterns"]

[[exercises]]
id = "q-order"
kind = "quiz"
prompt = "Which workflow is the safest first pass on an unfamiliar binary?"
choices = ["Patch bytes first, then ask what changed", "Inspect format and mitigations, identify imports and entry points, trace behavior, then patch a minimal decision", "Disable ASLR before reading anything", "Assume every call is statically linked"]
answer = 1
explanation = "Good binary work narrows uncertainty before editing bytes. Inspection tells you what kind of target you have."

[[exercises]]
id = "q-mitigation-limits"
kind = "quiz"
prompt = "A one-byte branch patch changes `jne denied` to `je denied`. Which mitigation most directly prevents writing that patch back into a signed production artifact?"
choices = ["NX", "A code-signing or integrity check outside the raw instruction stream", "The red zone", "The carry flag"]
answer = 1
explanation = "NX affects executing writable data, not offline byte editing. Integrity checks and signing are what make patched artifacts fail verification."

[[exercises]]
id = "e-final-patch"
kind = "emulate"
prompt = "Patch the final decision so a failed comparison still reaches success. Keep rdi = 0 and ecx = 7, but halt with rax = 1."
starter = """
    mov rdi, 0
    mov ecx, 7
    mov eax, 0
    cmp rdi, rcx
    jne denied
    mov eax, 1
denied:
    hlt
"""
solution = """
    mov rdi, 0
    mov ecx, 7
    mov eax, 0
    cmp rdi, rcx
    je denied
    mov eax, 1
denied:
    hlt
"""
expect_registers = { rax = 1 }
hints = ["The wrong input currently takes `jne denied`. Invert that condition so it falls through instead."]

[[exercises]]
id = "q-report"
kind = "quiz"
prompt = "What should a final reversing note record?"
choices = ["Only the byte patch", "The target facts, assumptions, addresses/offsets, observed behavior, patch, and residual risks", "Only screenshots", "Only the compiler version"]
answer = 1
explanation = "A useful handoff is reproducible: what you saw, how you know, what changed, and what could still invalidate the conclusion."
+++

# Capstone: Inspect, Trace, Patch

This capstone is the whole course in miniature. The goal is not to memorize a
tool sequence. The goal is to keep a chain of evidence from file bytes to runtime
behavior to the final patch.

## Inspect first

Start with the file format. Is it ELF or PE? Is it PIE? Which segments are
executable or writable? Which imports exist? Are symbols present? Which
mitigations are visible?

Those answers set expectations before you disassemble anything.

## Trace behavior

Find the decision you care about, but do not immediately patch it. Work backward
to the values that feed it and forward to the effects it controls. If the binary
calls imported functions, remember that the call may pass through GOT/PLT or IAT
machinery before reaching the real implementation.

At runtime, ASLR changes addresses. File offsets, virtual addresses and loaded
addresses are related, but not interchangeable.

## Patch minimally

The best patch changes the smallest thing that proves the hypothesis: invert a
branch, NOP a jump, or change an immediate. If the patch works, you learned the
decision mattered. If it does not, your model was incomplete.

Then ask which defenses mattered. NX does not stop a branch patch. RELRO does
not stop editing `.text` on disk. A signature or integrity check might.

## Report the chain

Write down the binary identity, offsets, original bytes, patched bytes, runtime
observations, mitigations, and assumptions. A patch without a reproducible note
is just a rumor encoded in hex.

## Key points

- Inspect format, imports and mitigations before patching.
- Keep file offsets, RVAs and loaded addresses separate.
- Patch the smallest decision that tests your model.
- Explain which mitigations apply to the patch and which do not.
- A good final note is reproducible evidence, not just a byte sequence.
