+++
id = "reading-compiled-code"
title = "Reading Compiled Code"
order = 1
estimated_minutes = 40
objectives = [
  "Recognise a function prologue and epilogue on sight",
  "Map a for-loop and an if-statement back to their compiled shape",
  "Read struct field access and array indexing out of addressing modes",
  "Recover the signedness of a comparison from the branch that follows it",
]
prerequisites = ["addressing-modes", "first-instructions", "the-stack"]

[[exercises]]
id = "q-recognise-loop"
kind = "quiz"
prompt = "You see this at the bottom of a block: `add eax, ecx` / `inc ecx` / `cmp ecx, 10` / `jl <top of block>`. What source construct is this?"
choices = [
  "A function call",
  "A counted loop (a for-loop) accumulating into eax",
  "A switch statement",
  "A recursive call",
]
answer = 1
explanation = "A counter (ecx) incremented each iteration, compared against a bound (10), with a conditional branch back to the top — that is a for-loop. `eax` accumulating each pass is the loop body's work. Recognising this shape is the core skill of reading disassembly."

[[exercises]]
id = "q-signedness"
kind = "quiz"
prompt = "After a `cmp eax, ebx` you see `jl target`. What does the `jl` (rather than `jb`) tell you about the *source* variables?"
choices = [
  "They were unsigned",
  "They were signed",
  "They were pointers",
  "Nothing — the branch mnemonic carries no type information",
]
answer = 1
explanation = "`jl` (jump if less) is the *signed* conditional; `jb` (jump if below) is the unsigned one. The compiler chose `jl`, so it was comparing signed integers. The comparison itself (`cmp`) is identical either way — the branch is where the erased type shows through, and recovering it is a standard technique."

[[exercises]]
id = "q-struct-access"
kind = "quiz"
prompt = "In compiled code you see `mov rax, qword [rdi+0x18]`. If `rdi` is a pointer to a struct, what is this most likely doing?"
choices = [
  "Reading the whole struct",
  "Reading the 8-byte field at offset 0x18 (24 bytes) into the struct",
  "Writing to the struct",
  "Computing the struct's size",
]
answer = 1
explanation = "A pointer in a register plus a constant displacement is a struct field access: `rdi` is the object, `0x18` is the field's offset within it, and the `qword` says the field is 8 bytes. Field offsets recovered this way let you reconstruct a struct's layout from code alone."

[[exercises]]
id = "d-prologue"
kind = "disassemble"
prompt = "These four bytes open a great many functions: `55 48 89 e5`. Disassemble them — there are two instructions."
hex = "554889e5"
expect_text = """
push rbp
mov rbp, rsp"""
hints = [
  "`55` is a one-byte `push` of a register; `48 89 e5` is a `mov` between two 64-bit registers.",
  "This is the standard frame-pointer prologue: save the caller's frame pointer, then establish your own.",
]

[[exercises]]
id = "d-loop-compare"
kind = "disassemble"
prompt = "The loop-condition check `48 83 f9 0a`. What is it?"
hex = "4883f90a"
expect_text = "cmp rcx, 0xa"
hints = [
  "`48` REX.W, `83` is arithmetic-with-a-sign-extended-imm8, and the ModRM `f9` selects rcx with the `cmp` operation.",
  "It compares the counter against ten — the exit test of a `for (i = 0; i < 10; i++)` loop.",
]
+++

# Reading Compiled Code

A compiler erases almost everything. Variable names, types, comments, the
boundaries between statements — none of it survives into the machine code. What
*does* survive is structure: a loop still loops, a struct field is still at a
fixed offset, a signed comparison still uses a signed branch. Reverse
engineering is the craft of reading that structure back out. This lesson is a
field guide to the shapes.

Everything here you can reproduce: assemble a small C-like routine by hand in the
**Playground**, disassemble it, and match it against the patterns below.

## The prologue and epilogue

Functions built with a frame pointer bracket their body with a fixed pair of
sequences:

```asm
    push rbp            ; 55           ── prologue: save caller's frame pointer
    mov  rbp, rsp       ; 48 89 e5        establish ours
    ...                 ;                 the body
    leave               ; c9           ── epilogue: undo the frame
    ret                 ; c3
```

`55 48 89 e5` at the start of a run of code is almost always a function
beginning — so reliably that disassemblers use it to *find* functions in a
stripped binary. Seeing it, you have found a boundary. (Optimised code often
omits the frame pointer, addressing locals from `rsp` directly; then you lean on
the `call` targets and `ret`s instead.)

## Loops

A counted loop compiles to a recognisable skeleton: initialise a counter, do the
body, step the counter, test it, branch back.

```asm
        xor eax, eax        ; sum = 0
        xor ecx, ecx        ; i = 0
    top:
        add eax, ecx        ; sum += i          ── the body
        inc ecx             ; i++               ── the step
        cmp ecx, 10         ; i < 10 ?          ── the test
        jl  top             ;                   ── the back-edge
```

The **back-edge** — a conditional branch to an *earlier* address — is the
signature of a loop. When you see a `jcc` whose target is above it, you are
looking at a loop, and the instructions between the target and the branch are
the body. The `cmp` just before the back-edge is the loop condition, and the
register it tests is the induction variable.

## if / else

An `if` is a comparison and a forward branch that skips the "taken" block:

```asm
        cmp edi, 0
        jle .else           ; if (x > 0) is false, skip to else
        ...                 ; the if-body
        jmp .end
    .else:
        ...                 ; the else-body
    .end:
```

Note the branch is *inverted*: source `if (x > 0)` compiles to "jump away if
`x <= 0`". The compiler branches on the condition being false so the common path
falls straight through, which is faster. Un-inverting the branch in your head is
half of reading an `if`.

## Signedness leaks through the branch

`cmp` sets flags and says nothing about types. The **branch** that reads those
flags does:

| branch | meaning | the variables were |
|--------|---------|--------------------|
| `jl` / `jg` / `jle` / `jge` | less / greater (signed) | **signed** |
| `jb` / `ja` / `jbe` / `jae` | below / above (unsigned) | **unsigned** |
| `je` / `jne` | equal / not equal | either |

So `jl` after a `cmp` tells you the source compared signed integers; `jb` tells
you unsigned. The compiler knew the types and chose the branch accordingly, and
it left that choice in the binary for you to read back. This is how you recover
whether a length field was `int` or `size_t` without any symbols at all.

## Structs and arrays live in the addressing modes

The addressing lesson's `[base + index*scale + disp]` is where data structure
shows up:

```asm
    mov rax, qword [rdi+0x18]      ; obj->field_at_offset_0x18   (struct member)
    mov eax, dword [rsi+rcx*4]     ; arr[i]  for a 4-byte element type (array index)
    lea rax, [rdi+0x10]            ; &obj->field_at_0x10          (address-of a member)
```

- A **constant displacement** off a pointer is a struct field; the displacement
  is the field's byte offset. Collect them and you have reconstructed the struct.
- A **scaled index** is array subscripting; the scale (1/2/4/8) is the element
  size, which tells you the element type's width.
- An `lea` computing such an address, with no memory access, is taking the
  *address* of a field or element — often to pass it by reference.

## The method

Reading disassembly is pattern-matching, top-down:

1. Find function boundaries (prologues, `ret`s, `call` targets).
2. Find loops (back-edges) and mark their bodies.
3. Turn each `cmp`+`jcc` back into its `if`, un-inverting the branch, and read
   the signedness off the mnemonic.
4. Read struct offsets and array scales out of the memory operands.
5. Name things as their purpose becomes clear, and iterate.

You are not decompiling line by line; you are recovering *shape*, and the shape
is enough to understand what the code does. The next lesson puts it to work on a
real check.

## Key points

- `55 48 89 e5` is the frame-pointer prologue and marks a function start.
- A conditional branch to an earlier address is a loop's back-edge; the `cmp`
  before it is the condition, the register it tests is the counter.
- An `if` compiles to a comparison and a forward branch on the *inverted*
  condition.
- The branch mnemonic recovers signedness the compiler erased: `jl`/`jg` signed,
  `jb`/`ja` unsigned.
- Constant displacements off a pointer are struct fields; scaled indices are
  array subscripts, and the scale is the element size.
