+++
id = "addressing-modes"
title = "Addressing Modes"
order = 2
estimated_minutes = 35
objectives = [
  "Read any x86_64 memory operand as base + index*scale + displacement",
  "Explain why the scale factor is limited to 1, 2, 4 and 8",
  "Use `lea` for arithmetic, and say why it never touches memory",
  "Explain why `rsp` cannot be an index register, from the encoding",
]
prerequisites = ["first-instructions"]

[[exercises]]
id = "q-scale"
kind = "quiz"
prompt = "Why can the scale factor only be 1, 2, 4 or 8?"
choices = [
  "Because larger multiplies would be too slow",
  "Because the SIB byte reserves exactly two bits for it, and those are the sizes of the primitive types",
  "Because Intel ran out of opcodes",
  "It can be any value up to 255",
]
answer = 1
explanation = "Two bits give four choices, and 1/2/4/8 are exactly the widths of a byte, short, int and pointer — so `arr[i]` needs no separate multiply."

[[exercises]]
id = "q-lea"
kind = "quiz"
prompt = "`lea rax, [rbx+8]` — what does it do?"
choices = [
  "Loads the 8 bytes at address rbx+8 into rax",
  "Sets rax to rbx+8, without reading memory",
  "Stores rax at address rbx+8",
  "Adds 8 to rbx",
]
answer = 1
explanation = "`lea` computes the address the brackets describe and hands it to you. It never dereferences, so it cannot fault — which is why compilers use it as a general-purpose three-operand add-and-shift."

[[exercises]]
id = "a-lea-index"
kind = "assemble"
prompt = "Compute the address of `rbx[rcx]` for a 4-byte element type, into `rax`. One instruction."
starter = "lea rax, "
solution = "lea rax, [rbx+rcx*4]"
expect_hex = "488d048b"
hints = [
  "Base + index*scale, with no displacement.",
  "The scale for a 4-byte element is 4.",
]

[[exercises]]
id = "d-load-arg"
kind = "disassemble"
prompt = "`48 8b 44 24 08` — five bytes, one instruction. There is a SIB byte in there. What is it?"
hex = "488b442408"
expect_text = "mov rax, qword [rsp+0x8]"
hints = [
  "`48` REX.W, `8b` is `mov r64, r/m64`, `44` is the ModRM byte, `24` is a SIB byte, `08` is a one-byte displacement.",
  "Using rsp as a base always costs a SIB byte.",
]
+++

# Addressing Modes

Every memory operand in x86_64, without exception, is a subset of one formula:

```text
    segment : [ base + index * scale + displacement ]
```

That is the whole addressing model. Learn to see this shape and every memory
access becomes readable.

| part           | what it can be                     |
|----------------|------------------------------------|
| `base`         | any of the 16 registers, or absent |
| `index`        | any register **except `rsp`**, or absent |
| `scale`        | 1, 2, 4 or 8 — nothing else        |
| `displacement` | a signed 8- or 32-bit constant     |

The address is computed at execution time by summing whichever parts are
present. There is no multiply instruction, no add instruction — the address
generation unit does it as part of the memory access, for free.

## The common shapes

```asm
    mov rax, qword [rbx]              ; *rbx                     -- a pointer dereference
    mov rax, qword [rbx+8]            ; rbx->field               -- a struct member
    mov rax, qword [rbp-8]            ; a local variable
    mov rax, qword [rbx+rcx*8]        ; rbx[rcx]                 -- an array of u64
    mov rax, qword [rbx+rcx*4+16]     ; rbx->arr[rcx]            -- an array inside a struct
    mov rax, qword [rip+0x1234]       ; a global, position-independently
```

Each of those is *one instruction*. `arr[i]` on an array of 8-byte elements
compiles to a single `mov` with `scale = 8`, because scaling is built into the
addressing mode. This is why the scale values are 1, 2, 4 and 8 and not, say, 3
and 5: they are the sizes of the primitive types, so the most common indexing
operation in all of programming needs no arithmetic instruction at all.

The mechanical reason is narrower still. The scale lives in the top two bits of
the SIB byte:

```text
     7   6   5   4   3   2   1   0
   +---+---+---+---+---+---+---+---+
   | scale |   index   |    base   |
   +---+---+---+---+---+---+---+---+
       2 bits    3 bits     3 bits
```

Two bits. Four values. `1 << scale_bits` gives 1, 2, 4, 8. The instruction set
did not choose those sizes to be convenient — it chose two bits, and the sizes
followed.

## `lea`: address arithmetic without memory

```asm
    mov rax, qword [rbx+rcx*4+16]   ; LOAD the value at that address
    lea rax, [rbx+rcx*4+16]         ; COMPUTE that address; touch nothing
```

`lea` — *load effective address* — runs the address computation and gives you
the result. It never dereferences. It cannot fault, even if the address is
garbage, unmapped, or null. Try it: `lea rax, [0]` is perfectly legal.

Compilers love it, and not for addresses. Look at what it really is:

> a three-operand, non-flag-setting, shift-and-add instruction

```asm
    lea rax, [rbx+rcx]      ; rax = rbx + rcx        -- add, without clobbering rbx
    lea rax, [rbx+rbx*4]    ; rax = rbx * 5          -- multiply by a small constant
    lea rax, [rcx*8+16]     ; rax = rcx * 8 + 16
```

Ordinary `add` is two-operand: it destroys its destination. `lea` reads two
registers and writes a third. Ordinary `add` sets flags; `lea` does not, so it
can sit between a `cmp` and its `jcc` without disturbing them. When you see
`lea` in optimised code with no memory access anywhere nearby, this is why.

Note the missing `qword` in the `lea` forms. There is no access, so there is no
access width, and the assembler will reject a size keyword there.

## Why `rsp` cannot be an index

Look again at the SIB byte's index field: three bits, so eight values, extended
to sixteen by the REX.X prefix bit.

The encoding needs a way to say *"there is no index register"*. It steals the
value `100` (decimal 4) for that. So:

```text
    index field = 100, REX.X = 0   ->   "no index"
    index field = 100, REX.X = 1   ->   r12
```

Register 4 is `rsp`. Its bit pattern has been spent on "no index", and REX.X
rescues only `r12`, not `rsp` itself. **There is no bit pattern left that means
`rsp` as an index.** Not a design decision about stack safety — just an encoding
that ran out of room.

`r12` *can* be an index, and `rsp` cannot, and now you know why they look so
similar in a hex dump. The decoder in this project enforces exactly this, in
`crates/asm-core/src/decode.rs`.

The mirror-image quirk: `rm = 100` in the *ModRM* byte means "a SIB byte
follows". So naming `rsp` as a **base** forces a SIB byte to exist, even when
there is no index at all:

```text
    mov rax, qword [rax]    ->  48 8b 00           three bytes
    mov rax, qword [rsp]    ->  48 8b 04 24        four bytes -- a SIB byte appears
```

And one more: `[rbp]` with no displacement would encode as `mod=00, rm=101`,
but that pattern was taken for RIP-relative addressing. So `[rbp]` must be
encoded as `[rbp+0]` with an explicit zero displacement byte:

```text
    mov rax, qword [rax]    ->  48 8b 00           three bytes
    mov rax, qword [rbp]    ->  48 8b 45 00        four bytes -- a wasted zero
```

Three registers, three different instruction lengths, for the same operation.
Assemble all three in the playground and read the bytes.

## RIP-relative addressing

New in 64-bit mode:

```asm
    lea rsi, [rip+message]      ; the address of `message`, wherever we were loaded
```

The base is the address of the *next* instruction. The linker computes the
displacement at link time; the loader does not have to patch anything, because
the distance between two things in the same image never changes no matter where
the image lands.

This is the machinery that makes position-independent executables cheap, and it
is why 64-bit code is full of `[rip+...]` where 32-bit code had absolute
addresses and a relocation table. In 64-bit mode the old `mod=00, rm=101`
absolute-address encoding was *repurposed* for this, so absolute addressing now
costs an extra SIB byte. Position-independent code got cheaper; position-
dependent code got more expensive. That trade was deliberate.

## Key points

- One formula: `[base + index*scale + disp]`. Everything else is a special case
  of it with parts left out.
- Scale is 1/2/4/8 because it is two bits wide, which happily matches primitive
  type sizes — so array indexing is free.
- `lea` computes an address and never touches memory: a three-operand add that
  does not set flags.
- `rsp` cannot be an index because its encoding was spent on "no index". `rsp`
  as a base costs a SIB byte; `[rbp]` costs a zero displacement byte.
- `[rip+disp]` addresses data relative to the next instruction, and is what
  makes PIE binaries practical.
