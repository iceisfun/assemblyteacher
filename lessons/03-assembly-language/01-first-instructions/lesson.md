+++
id = "first-instructions"
title = "Your First Instructions"
order = 1
estimated_minutes = 40
objectives = [
  "Read and write the mov, add, sub, cmp and jcc instructions",
  "Explain why `cmp` and `test` compute a result they immediately discard",
  "Build a loop out of a comparison and a conditional jump, and reason about its exit condition",
  "Choose between signed and unsigned conditional jumps correctly",
]
prerequisites = ["registers"]

[[exercises]]
id = "q-cmp-does-what"
kind = "quiz"
prompt = "What does `cmp rax, rbx` actually compute?"
choices = [
  "Nothing; it only compares",
  "rax - rbx, storing the result in rax",
  "rax - rbx, discarding the result but keeping the flags",
  "rax XOR rbx, keeping the flags",
]
answer = 2
explanation = "`cmp` is `sub` with the destination write suppressed. All the information about the comparison ends up in the flags."

[[exercises]]
id = "q-signed-jump"
kind = "quiz"
prompt = "`rax` holds 1 and `rbx` holds -1. After `cmp rax, rbx`, which of these jumps is taken?"
choices = [
  "jl  (signed less)",
  "jb  (unsigned below)",
  "both",
  "neither",
]
answer = 1
explanation = "Signed: 1 > -1, so `jl` is not taken. Unsigned: -1 is 0xffffffffffffffff, an enormous number, so 1 is below it and `jb` is taken. One `cmp`, two answers — the branch is where the signedness lives."

[[exercises]]
id = "a-add-one"
kind = "assemble"
prompt = "Add 1 to `rax` using the `add` instruction. (The assembler will pick the four-byte sign-extended-immediate form for you.)"
starter = ""
solution = "add rax, 1"
expect_hex = "4883c001"
hints = [
  "`48` is the REX.W prefix that makes this a 64-bit operation.",
  "`83` is the opcode for 'arithmetic on r/m with a sign-extended 8-bit immediate' — that is why the immediate is one byte, not four.",
]

[[exercises]]
id = "d-add-rsp"
kind = "disassemble"
prompt = "These four bytes end a great many functions: `48 83 c4 08`. What instruction are they?"
hex = "4883c408"
expect_text = "add rsp, 0x8"
hints = [
  "`48` = REX.W. `83` = arithmetic with an imm8. `c4` is the ModRM byte.",
  "It is undoing a `push`.",
]

[[exercises]]
id = "e-sum-loop"
kind = "emulate"
prompt = "Write a loop that adds the integers 1 through 10 and halts with the sum (55) in `rax`. Use `eax` as the accumulator and `ecx` as the counter. End with `hlt`."
starter = """
    xor eax, eax
    mov ecx, 1
loop_top:
    ; add ecx to eax, step ecx, and loop while ecx <= 10
    hlt
"""
solution = """
    xor eax, eax
    mov ecx, 1
loop_top:
    add eax, ecx
    inc ecx
    cmp ecx, 10
    jle loop_top
    hlt
"""
expect_registers = { rax = 55 }
hints = [
  "`add eax, ecx` accumulates; `inc ecx` steps the counter.",
  "Compare the counter against 10 and jump back while it is still less-or-equal.",
  "`jle` is the signed form. `jbe` would also work here since nothing is negative — but say what you mean.",
]
+++

# Your First Instructions

Assembly has no statements, no expressions, and no types. It has a list of
instructions, each of which does one small thing to registers, memory, or the
instruction pointer. Programs are built by arranging those effects in order.

This lesson covers enough to write a loop.

## Moving data

```asm
    mov rax, 42          ; rax = 42                 (immediate -> register)
    mov rbx, rax         ; rbx = rax                (register  -> register)
    mov rcx, qword [rsp] ; rcx = *(u64*)rsp         (memory    -> register)
    mov qword [rsp], rdx ; *(u64*)rsp = rdx         (register  -> memory)
```

Intel syntax is **destination first**. `mov rax, rbx` copies `rbx` into `rax`,
the same order as `rax = rbx`. (AT&T syntax, which you will meet in `gdb` and
in GCC's default output, reverses it. Both are the same machine code.)

Square brackets mean *dereference*. `rax` is the register; `[rax]` is the eight
bytes of memory at the address in `rax`. Confusing these two is the single most
common beginner mistake, and it is worth saying out loud every time you read a
line: "brackets means go to memory."

**There is no memory-to-memory `mov`.** The encoding has room for one memory
operand, not two. Copying `[rsi]` to `[rdi]` takes two instructions and a
register in between. That is not a limitation of the assembler; it is what the
ModRM byte can express.

## Arithmetic

```asm
    add rax, rbx         ; rax += rbx
    sub rax, 8           ; rax -= 8
    inc rax              ; rax += 1     (but leaves CF alone)
    neg rax              ; rax = -rax
    imul rax, rbx        ; rax *= rbx   (signed)
```

Two-operand form throughout: the destination is also the first source. `add rax,
rbx` is `rax += rbx`, never `rax = rax + rbx` with a third register. If you want
`c = a + b` without destroying `a`, you copy first — or you use `lea`, which we
meet in the next lesson.

Every one of these sets flags as a side effect.

## Comparison is subtraction

Here is the part that surprises people:

```asm
    cmp rax, rbx         ; compute rax - rbx, set the flags, THROW THE RESULT AWAY
    test rax, rax        ; compute rax & rax, set the flags, throw the result away
```

`cmp` is `sub` with the write-back disabled. `test` is `and` with the write-back
disabled. Nothing is stored. The *only* output is the flags register.

Why bother? Because the flags contain everything you might want to know:

- Is `rax == rbx`? Then `rax - rbx == 0`, so **ZF = 1**.
- Is `rax < rbx` unsigned? Then the subtraction borrowed, so **CF = 1**.
- Is `rax < rbx` signed? Then **SF ≠ OF**.

One subtraction answers every comparison. The conditional jump that follows
simply reads the flag combination it cares about.

`test rax, rax` is the idiomatic "is it zero?": `x & x` is `x`, so ZF is set
exactly when `rax` is zero. It is shorter than `cmp rax, 0` and sets no
immediate. When you see `test reg, reg` followed by `je`, read it as `if (!reg)`.

## Conditional jumps

```asm
    cmp rax, rbx
    je  equal            ; jump if ZF=1
    jne not_equal        ; jump if ZF=0

    jl  less             ; SIGNED   less than       (SF != OF)
    jg  greater          ; SIGNED   greater than
    jb  below            ; UNSIGNED less than       (CF = 1)
    ja  above            ; UNSIGNED greater than
```

The mnemonics are deliberately different words: *less/greater* for signed,
*below/above* for unsigned. Nothing in `cmp` says which you meant. **The branch
declares the type.**

```asm
    mov rax, 1
    mov rbx, -1
    cmp rax, rbx
    jl  somewhere        ; NOT taken:  1 > -1
    jb  elsewhere        ; TAKEN:      1 < 0xffffffffffffffff
```

Both jumps read flags from the *same* `cmp`. This is where the type information
the compiler erased leaks back out, and it is a workhorse of reverse
engineering: see `jb`, and you know the source variable was unsigned.

A jump's target is encoded as a **displacement from the end of the jump
instruction**, not as an absolute address. That is why `eb fe` — jump back two
bytes, over yourself — is the classic two-byte infinite loop, and why code
containing only relative jumps can be moved in memory without patching.

## A loop

Put it together. Sum 1 through 10:

```asm
        xor eax, eax        ; sum = 0
        mov ecx, 1          ; i = 1
    loop_top:
        add eax, ecx        ; sum += i
        inc ecx             ; i++
        cmp ecx, 10         ; compare i with 10
        jle loop_top        ; while (i <= 10) go again
        hlt
```

Read the exit condition carefully. `cmp ecx, 10` then `jle` jumps back while
`ecx <= 10`. So the body runs with `ecx` equal to 1..10, and falls through with
`ecx = 11`. Add `ecx` *before* incrementing and the sum is 55; swap those two
lines and it is 54. Off-by-one errors in assembly are not subtler than in C,
but there is nothing to hide behind.

Note `inc ecx` rather than `add ecx, 1`. They differ in one respect: `inc` does
not modify CF. In a loop that also carries a bit in CF, that distinction is the
entire reason `inc` exists.

Also note that the whole thing works on 32-bit registers, and `rax` still ends
up holding exactly 55 — because a 32-bit write zero-extends.

## Key points

- Intel syntax is `op dst, src`. Brackets mean a memory access. There is no
  memory-to-memory `mov`.
- Arithmetic is two-operand: the destination is also an operand.
- `cmp` is `sub` and `test` is `and`, both with the result discarded. The flags
  are the point.
- Signed comparisons use less/greater; unsigned use below/above. The `cmp` does
  not know which you meant — the jump does.
- Jump displacements are relative to the *end* of the jump instruction.
