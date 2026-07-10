---
name: authoring-a-lesson
description: How to add a lesson to the Assembly Teacher curriculum — the directory layout, the lesson.md front matter, the four exercise types, and how the test suite executes every example and answer so the lesson cannot silently rot.
---

# Authoring a Lesson

A lesson is a self-contained directory. Everything it needs — prose, runnable
examples, exercises, reference answers, images — lives inside it. There is no
central registry to edit and no database to update. **You add a lesson by adding
a directory, and the test suite proves it correct.**

This is the guide to doing that. Read one existing lesson first; the reference
implementations are `lessons/02-cpu-architecture/01-registers/` (rich prose, all
four exercise types nearby) and `lessons/03-assembly-language/01-first-instructions/`.

## The one rule that matters

Every claim a lesson makes that *can* be checked mechanically, *is* checked
mechanically, by `cargo test`:

- Every `examples/*.asm` file must assemble.
- Every exercise carries its own reference solution, and the test suite grades
  that solution with the **same code that grades a student**. If the stated
  answer does not actually pass, the build fails.

So you cannot write a lesson that claims `mov rax, 1` is six bytes when it is
seven, or an exercise whose "correct" answer is wrong. The tooling will not let
you. This is what keeps the curriculum honest as the assembler and emulator
evolve. Write your examples and answers as real code, not as illustrations.

Run `cargo test -p lesson` after every change. It loads the whole `lessons/`
tree, assembles every example, and runs every reference answer.

## Directory layout

```
lessons/
  03-assembly-language/            ← a PART directory
    part.toml                        number + title for this part
    01-first-instructions/         ← a LESSON directory
      lesson.md                      front matter + prose (required)
      README.md                      what this lesson is, for repo browsers (required)
      examples/                      runnable .asm files (assembled by the tests)
        sum_loop.asm
      solutions/                     reference material; never served to clients
      assets/                        images referenced by lesson.md
      tests/                         extra fixtures, if a lesson needs them
```

The numeric prefixes (`03-`, `01-`) exist so `ls` matches the reading order.
They are **not** what determines order — the `order` and `number` fields are.
Keep them in sync anyway, for the humans.

Empty directories are fine; commit a `.gitkeep` so git tracks them. The
validator requires `README.md` and a non-empty `lesson.md`; `examples/`,
`solutions/`, `assets/` and `tests/` are optional.

## `part.toml`

One per part directory:

```toml
number = 3
title = "Assembly Language"
```

Parts are ordered by `number`. Two parts may not share a number.

## `lesson.md`

TOML front matter between `+++` fences, then Markdown. The front matter carries
the metadata *and the exercises*; the body is pure prose.

```markdown
+++
id = "first-instructions"
title = "Your First Instructions"
order = 1
estimated_minutes = 40
objectives = [
  "Read and write the mov, add, sub, cmp and jcc instructions",
  "Explain why cmp computes a result it immediately discards",
]
prerequisites = ["registers"]

[[exercises]]
id = "q-cmp"
kind = "quiz"
prompt = "What does `cmp rax, rbx` actually compute?"
choices = ["Nothing", "rax - rbx, kept in rax", "rax - rbx, discarded, flags kept"]
answer = 2
explanation = "cmp is sub with the write-back suppressed. The flags are the point."
+++

# Your First Instructions

Prose goes here. Use fenced code blocks for assembly...
```

### Front-matter fields

| field               | required | notes                                                       |
|---------------------|----------|-------------------------------------------------------------|
| `id`                | yes      | globally unique, kebab-case, stable — it appears in URLs     |
| `title`             | yes      | shown everywhere                                             |
| `order`             | yes      | position within the part; unique within a part              |
| `objectives`        | yes      | what the reader can do afterwards; at least one              |
| `prerequisites`     | no       | ids of lessons that must come **earlier**; no cycles         |
| `estimated_minutes` | no       | a hint for the reader                                        |
| `exercises`         | no       | an array of `[[exercises]]` tables (below)                   |

Prerequisites are enforced: each must name a real lesson that appears earlier in
the curriculum. `cargo test -p lesson` fails on a forward reference or a cycle,
and on a lesson that lists itself.

### Writing the body

The house style is *explain why, not just what*. Every existing lesson derives
its facts from a cause rather than asserting them:

- Don't write "the stack grows down." Write *why* it grows down (so the stack
  and heap can share one pool of free space from opposite ends), then show it.
- Don't write "use `sar` for signed division." Show what `shr` does to a
  negative number, and let the reader see why it is wrong.

Connect levels. The best lessons tie a hardware fact to something the reader has
seen in a higher-level language: the `INT_MIN` asymmetry to a real `abs()` bug,
`ret` being `pop rip` to the stack buffer overflow, the zero-extension rule to
why compiled code is full of 32-bit instructions on 64-bit values.

End with a short **Key points** list.

Fenced code blocks are rendered but not executed. Anything you want the test
suite to *verify* goes in `examples/` or in an exercise, not in a prose block —
that way it cannot drift out of date.

## Examples

Drop `.asm` files in `examples/`. Each one is assembled by the test suite (via
`asm-core::assemble`), so it must be valid. Use them for the demonstrations the
reader will load into the Playground. Comment them heavily — they are teaching
material, and the reader will step through them instruction by instruction.

The assembler is NASM-flavoured Intel syntax: labels, `db`/`dw`/`dd`/`dq` data,
`org`, `[rip+label]`, size keywords, segment overrides. It intentionally covers
the integer subset only. If an example uses an instruction the assembler does
not know, the test fails and tells you which line.

## Interactive helpers in prose

Numbers, mnemonics and registers in your lesson body become interactive
automatically — **anything inside `` `backticks` `` lights up**. A reader can
hover (or tap, on a phone) a number to see its binary/decimal/hex readings and a
nibble-by-nibble breakdown with per-bit place values; a mnemonic to see what it
does, which flags it touches, and its byte encoding; or a register to see its
four-width family, the bytes it covers, its role, and whether it is callee- or
caller-saved. You write nothing special: `` `mov al, 0x2a` `` already makes
`mov`, `al` and `0x2a` all interactive. All existing lessons got this for free.

Two explicit forms give you finer control:

- **Forced inline chips**, for a token *outside* code, or to be deliberate:
  `:num[0x2a]`, `:insn[lea]`, `:reg[rax]`.
- **Inline embedded cards**, the always-open teaching form — the full
  decomposition sitting in the page rather than behind a hover:

  ```text
  :::number 0x2a
  :::instruction lea rax, [rbx+rcx*4]
  :::register al
  ```

  The register card is a full interactive family explorer: it shows the whole
  hierarchy (RAX ⊃ EAX ⊃ AX ⊃ AH/AL), hovering any relative previews the bits it
  owns on a 64-bit strip, clicking navigates to it, and it explains the write
  semantics (merge vs zero-extend) with example machine code. Embedding it with
  `:::register` in the Registers lesson lets a reader learn the register file by
  exploration.

The intended progression is **scaffolding that fades**: in an early lesson, embed
the full card with `:::number` so the decomposition is unavoidable; in later
lessons, rely on the automatic hover chips once the reader has internalised it.
Same data, two levels of prominence, your choice per lesson.

## Exercises

Exercises live in the front matter as `[[exercises]]` tables. Every exercise
has `id` (unique within the lesson), `prompt`, optional `hints` (revealed
progressively by the UI on repeated failure), and a `kind` that selects the
remaining fields. There are four kinds.

Grading is by **effect, not text**, everywhere it can be. That is the principle:
a student who solves the problem a different way is right.

### `quiz` — multiple choice

```toml
[[exercises]]
id = "q-zero-extension"
kind = "quiz"
prompt = "`rax` is 0xffffffffffffffff. After `mov eax, 5`, what is in `rax`?"
choices = ["0xffffffff00000005", "0x0000000000000005", "0xffffffffffffff05"]
answer = 1                       # 0-based index into `choices`
explanation = "A 32-bit write zero-extends; only this width does."
```

`answer` is the 0-based index. `explanation` is shown on both success and
failure. At least two choices; `answer` must be in range.

### `assemble` — write code that produces specific bytes

```toml
[[exercises]]
id = "a-zero-eax"
kind = "assemble"
prompt = "Write the shortest instruction that sets `rax` to zero."
starter = ""                     # pre-filled in the editor (optional)
solution = "xor eax, eax"        # the reference answer (required)
expect_hex = "31c0"              # the bytes the answer must assemble to
```

Graded by assembling the submission and comparing **machine code** to
`expect_hex`. Any source that produces those exact bytes passes, so
`xor eax, eax` and `XOR EAX,EAX` are both accepted. The failure message tells
the student what their code assembled to and what was wanted — write `prompt` so
that message makes sense.

Choose `expect_hex` to pin the *encoding* you are teaching (here, the two-byte
form), not merely the effect — `mov eax, 0` has the same effect but different
bytes, and is correctly rejected.

### `disassemble` — read bytes, name the instruction

```toml
[[exercises]]
id = "d-read-mov"
kind = "disassemble"
prompt = "These bytes are one instruction: `b8 2a 00 00 00`. What is it?"
hex = "b82a000000"
expect_text = "mov eax, 0x2a"    # compared by meaning where possible
```

Graded leniently: the student's answer is first *assembled* and compared to
`hex` by bytes, so `jz 0x2` is accepted for `je 0x2`. Only if their text will
not assemble does it fall back to a normalised text comparison against
`expect_text`. The `hex` must itself disassemble cleanly (the validator checks).

### `emulate` — write a program with a required result

```toml
[[exercises]]
id = "e-factorial"
kind = "emulate"
prompt = "Compute 5! recursively; halt with rax = 120."
starter = "mov rdi, 5\ncall fact\nhlt\nfact:\n  ret"
solution = "..."                 # a program that actually produces the result
expect_registers = { rax = 120 }  # register name -> required final value
# expect_stdout = "hello"        # optional: required stdout (from write syscalls)
max_steps = 10000                # optional; default 100000
```

Graded by *running* the submission on `asm-emu` and comparing the final machine
state. Assert on `expect_registers` (a map of register name to value),
`expect_stdout`, or both — at least one. A submission that never terminates is a
*wrong answer* (it hits `max_steps`), not a hang; a submission that faults is
told where. The emulator has no host access: `write` to fd 1/2 and `exit` are
the only syscalls.

Register values are plain integers in TOML. For a value like `0xffffff00`, write
the decimal (`4294967040`) or let TOML's hex work if your parser accepts it —
the existing lessons use decimal to be safe.

## Checklist for a new lesson

1. `mkdir lessons/<NN>-<part>/<NN>-<slug>/` with `examples/ solutions/ assets/`.
2. Write `lesson.md` (front matter + prose) and `README.md`.
3. Add `examples/*.asm` for anything you want executed and step-through-able.
4. Add exercises to the front matter, each with a working `solution`.
5. `cargo test -p lesson` — this assembles every example and runs every answer.
6. `cargo test -p server` if you referenced the lesson from a server test.
7. `contrib/test.sh --rust` before you commit.

If step 5 fails, it will name the lesson, the exercise, and what went wrong —
for example, *"the reference solution does not pass its own exercise: that
assembles to b800000000, but the exercise wants 31c0"*. That message is the
tooling doing its job: your stated answer and your stated bytes disagreed, and
it caught it before a student did.

## Adding a whole new Part

Create `lessons/<NN>-<part-slug>/part.toml` with a fresh `number` and `title`,
then add lessons under it. The proposed curriculum in
[`docs/architecture.md`](docs/architecture.md#curriculum) lists the parts and
their intended topics, but it is a floor, not a ceiling — new parts, lessons and
laboratories are welcome wherever they improve the platform.
