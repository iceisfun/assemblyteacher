+++
id = "aob-scanning"
title = "AOB Scanning: Finding Code by Its Bytes"
order = 3
estimated_minutes = 40
objectives = [
  "Explain what a byte signature (AOB) is and why wildcards let one pattern match the same code across builds and load addresses",
  "Split an instruction into its stable opcode bytes and its volatile operand bytes, and place wildcards on exactly the volatile ones",
  "Explain why a relative call or jump (E8/E9) is the archetypal wildcard site, and how short (rel8) vs near (rel32) forms change a pattern's length",
  "Account for variable-length encoding: why a byte match must land on an instruction boundary, and how surrounding stable bytes anchor a signature",
]
prerequisites = ["reading-compiled-code", "addressing-modes"]

[[exercises]]
id = "q-what-aob"
kind = "quiz"
prompt = "You found an interesting function in one build of a program. In the next build it has moved to a different address. Why is a byte signature (an 'array of bytes', AOB) a more durable way to find it again than its address?"
choices = [
  "Because addresses are encrypted but bytes are not",
  "Because the instruction bytes of the function stay largely the same across builds, while its absolute address changes with layout and ASLR — so a pattern of those bytes still matches",
  "Because the function's name is stored next to its bytes",
  "Because a signature disables ASLR",
]
answer = 1
explanation = "An address is a fact about *where* the code landed, which layout changes, ASLR randomizes, and a recompile shuffles. The *bytes* of the instructions are a fact about *what the code is*, and they stay mostly constant. A signature scans memory for that byte pattern and finds the function wherever it ended up — which is how AV/YARA rules, anti-cheat, and reverse engineers re-locate code without symbols."

[[exercises]]
id = "q-wildcard-what"
kind = "quiz"
prompt = "A signature is written as concrete bytes plus `??` wildcards, e.g. `E8 ?? ?? ?? ??`. To make a pattern that survives across builds, which bytes should be the wildcards?"
choices = [
  "The opcode bytes, because they identify the instruction",
  "The volatile operand bytes — displacements, relative branch targets, and layout-dependent immediates — while the stable opcode/ModRM bytes stay concrete",
  "Every other byte, in a checkerboard, to be safe",
  "The first and last byte of the pattern",
]
answer = 1
explanation = "An instruction splits into a stable part (the opcode and ModRM: *what* it does) and a volatile part (the operand encoding: *which* address or constant). The volatile bytes — a RIP-relative displacement, a call's relative target, an absolute address — change when code or data moves. Wildcard exactly those, and pin the opcode bytes. Too many wildcards and the pattern matches everywhere; too few and it breaks on the next build."

[[exercises]]
id = "q-rel-branch"
kind = "quiz"
prompt = "In the signature `E8 ?? ?? ?? ??` (a near `call`), what do the four wildcarded bytes actually encode, and why do they change every build?"
choices = [
  "The function's name, which is recompiled each build",
  "A rel32 displacement — the signed distance from this call to its target — so moving either the call or the target changes it, while the E8 opcode stays put",
  "A checksum of the called function",
  "The number of arguments passed to the call",
]
answer = 1
explanation = "`E8` is `call rel32`: the opcode is followed by a 4-byte signed displacement measured from the end of the call to the target. That distance depends on where both the call and its target landed, so any layout change (a recompile, an inserted instruction upstream, a different base) rewrites those four bytes — while `E8` never moves. That is why a run of `?? ?? ?? ??` after a branch opcode 'means something': it is the local branch target within the module. The same is true of a RIP-relative load like `48 8B 05 ?? ?? ?? ??`."

[[exercises]]
id = "q-short-vs-near"
kind = "quiz"
prompt = "A `jmp` to a nearby label assembles to `EB cb` (2 bytes, an 8-bit relative reach of about ±127). A `jmp` to a far target assembles to `E9 cd` (5 bytes, a 32-bit reach). What does this mean for a signature that spans a jump?"
choices = [
  "Nothing; every jump is always the same length",
  "The same source jump can be 2 bytes in one build and 5 in another if its target drifts in or out of short range, so the signature's length — and the offsets after it — can shift between builds",
  "Short jumps cannot be wildcarded",
  "The CPU rejects near jumps in 64-bit mode",
]
answer = 1
explanation = "The assembler picks the shortest branch encoding that still reaches the target: `EB`/`7x` short forms (rel8) when it is close, `E9`/`0F 8x` near forms (rel32) when it is not. So a jump is not a fixed-width thing — if a later edit pushes the target out of ±127, that one instruction grows from 2 bytes to 5, and everything you were counting on after it shifts. Anchor signatures on stable opcodes and never assume a fixed instruction width. (See the instruction reference for the full jmp/jcc family and their encodings.)"

[[exercises]]
id = "q-alignment"
kind = "quiz"
prompt = "A byte scanner slides your pattern over memory and reports any offset where the concrete bytes match. Since x86-64 instructions are 1–15 bytes and variable-length, what is the classic false-positive hazard?"
choices = [
  "The scanner is too slow to finish",
  "A match can start in the middle of an instruction, or straddle the tail of one instruction and the head of the next — the bytes match but they are not the instruction you meant",
  "The scanner can only match at 16-byte boundaries",
  "Wildcards make every pattern match everywhere",
]
answer = 1
explanation = "The scanner has no idea where instructions begin; it matches raw bytes. Because instructions vary in width, your pattern can land mid-instruction (matching an operand's bytes) or span a boundary, giving a hit that disassembles into nonsense. Two defences: include enough surrounding *stable opcode* bytes that the pattern is unique and starts on a real instruction boundary, and confirm any hit by disassembling from that offset. When a signature 'does not line up', you are matching off-boundary — re-anchor it on an opcode."

[[exercises]]
id = "d-rip-load"
kind = "disassemble"
prompt = "The bytes `48 8b 05 10 00 00 00` are the load half of a common RIP-relative access — the shape whose last four bytes a signature would wildcard. What instruction is it?"
hex = "488b0510000000"
expect_text = "mov rax, qword [rip+0x10]"
hints = [
  "`48` is REX.W, `8b` is the load form of `mov`, and ModRM `05` selects RIP-relative addressing.",
  "The `10 00 00 00` is the 32-bit displacement to a global — placement-specific, so `48 8B 05 ?? ?? ?? ??` is how you would write this as a signature.",
]
+++

# AOB Scanning: Finding Code by Its Bytes

You are reversing a program and you find the function you care about — the license
check, the packet handler, the routine you want to watch. You note its address.
The next version ships, or you recompile, or ASLR rolls a new base, and that
address points at something else entirely. The *address* was never a property of
the function; it was a property of this one layout.

The instruction **bytes**, on the other hand, barely changed. The same source
compiles to nearly the same machine code, wherever it lands. **Signature
scanning** — matching an *array of bytes* (an "AOB") with wildcards — is how you
name a piece of code by what it *is* rather than where it happens to be. It is
the mechanism behind AV and YARA rules, anti-cheat and instrumentation locating a
function to hook, and a reverser re-finding a routine after an update.

## A pattern with holes

A signature is a run of byte values, some of which are wildcards:

```text
  01 02 03 04 ?? ?? ?? ?? 05
```

The concrete bytes must match exactly; each `??` matches *any* single byte. A
scanner slides this over the module's memory and reports every offset where the
non-wildcard bytes line up. The whole craft is in choosing *which* bytes are
holes — and to choose well you have to know what each byte of an instruction is
doing.

## Stable opcodes, volatile operands

Split any instruction in two. There is the **opcode** (with its ModRM byte): the
part that says *what operation* — call, load, add — and *what shape* of operand.
And there is the **operand encoding**: the bytes that say *which* address or
*which* constant. The first part is stable across builds. The second is not,
because addresses and layout-dependent constants move.

So the rule is simple: **wildcard the volatile operand bytes; pin the opcode
bytes.**

The archetypal volatile site is a **relative branch**. Consider a near call:

```text
  E8 ?? ?? ?? ??      call rel32
```

`E8` is the opcode for "call, target given as a 32-bit relative displacement."
The four bytes after it are that displacement — a *signed distance from this
call to the function it calls*, measured from the end of the instruction. Move
the call, move the target, insert an instruction anywhere upstream, or load the
module at a different base, and that distance changes: the four bytes are
rewritten every build. `E8` is not. That is precisely why `?? ?? ?? ??` after a
branch opcode "means something" — it is the branch's local target inside the
module, the thing that legitimately varies.

RIP-relative data loads are the same story. `48 8B 05 ?? ?? ?? ??` is `mov rax,
[rip+disp32]`: the `48 8B 05` (REX.W, the `mov` load form, ModRM selecting
RIP-relative) is fixed, and the 32-bit displacement to the global is the part
that slides. The exercise below is exactly this instruction. Absolute immediates
that depend on load layout get wildcarded for the same reason.

## Short versus near: the length is not fixed

A branch has more than one encoding, and the assembler chooses by **reach**:

```text
  EB cb          short jmp   2 bytes, rel8   (~±127 bytes)
  E9 cd          near  jmp   5 bytes, rel32  (~±2 GB)
  74 cb          short je    2 bytes, rel8
  0F 84 cd       near  je     6 bytes, rel32
```

The assembler emits the *shortest form that still reaches the target*. This has a
consequence people trip over: the same source jump can be **2 bytes in one build
and 5 in another** if an edit pushes its target out of short range. An
instruction you thought had a fixed width silently grew, and every offset you
were counting after it shifted. Anchor signatures on the stable opcodes, keep
them short enough to sit within one stable stretch, and never assume a fixed
instruction size. (The jmp/jcc family and its encodings are laid out in full in
the instruction reference.)

## Alignment: instructions are variable-length

Here is the subtlety that separates a working signature from a flaky one. x86-64
instructions are **1 to 15 bytes**, variable-length, packed with no padding. A
byte scanner does not know or care where an instruction begins — it matches raw
bytes at every offset. Two hazards follow:

- A pattern can match starting **in the middle** of an instruction (inside an
  operand), or **straddle a boundary** — the tail of one instruction plus the
  head of the next. The bytes match; the "instruction" is an illusion. That is a
  false positive.
- Because widths vary, you **cannot** assume "advance 4 bytes = skip one
  instruction." There is no fixed stride to step by.

The defence is to build the signature so it *starts on a real instruction
boundary* and carries enough surrounding **stable opcode** bytes to be unique in
the module — then confirm every hit by disassembling from that offset and
checking it decodes to the instruction you meant. When a signature "does not line
up," you are almost always matching off-boundary; re-anchor it on an opcode.

## What surrounds the hole

Good signatures are opcode-anchored on both sides of their wildcards. You keep
the stable opcode/ModRM *before* the volatile operand, and often the opcode of
the *next* instruction *after* it:

```text
  E8 ?? ?? ?? ?? 48 8B ...
  └─ the call    └─ the next instruction's stable opcode
```

The concrete bytes around the holes are what give the pattern both its
**uniqueness** (so it does not match a thousand places) and its **anchoring** (so
it starts where you think it does). The wildcards are only the parts that
genuinely vary. The art is to make the holes *just* the volatile operand bytes —
no wider — so the signature is as specific as it can be while still surviving the
next build.

## Analysis value

Signature scanning is a neutral, foundational technique: a way to refer to code
by its stable bytes when its address will not hold still. It is how a malware
family is recognised across samples, how a monitor finds the function it means to
observe, and how you pick your routine back up after an update moved everything.
It builds directly on reading compiled code — you are choosing your anchor bytes
by knowing which part of each instruction is opcode and which is operand.

## Key points

- An **AOB / signature** is concrete bytes plus `??` wildcards that match code
  wherever it loaded. Wildcards go on the **volatile operand bytes**; concrete
  bytes stay on the **stable opcodes**.
- **Relative branches** (`E8`/`E9`, `74`/`0F 84`, …) are the archetypal wildcard
  site: the rel32/rel8 displacement is a distance from the instruction, so it
  changes whenever code moves.
- Branches have **short (rel8)** and **near (rel32)** forms; the shortest
  reaching form is chosen, so a jump — and any signature spanning it — can change
  length between builds.
- x86 instructions are **variable-length**, so a byte match can land
  mid-instruction or straddle a boundary. Anchor signatures on instruction
  boundaries with stable opcode bytes, and confirm every hit by disassembling.
