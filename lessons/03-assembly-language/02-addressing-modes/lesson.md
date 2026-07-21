+++
id = "addressing-modes"
title = "Addressing Modes"
order = 2
estimated_minutes = 45
objectives = [
  "Read any x86_64 memory operand as base + index*scale + displacement",
  "Name the fields of an x86_64 instruction in order, and say which are optional",
  "Split a ModRM byte into mod/reg/r/m and say what each field selects",
  "Predict from a ModRM byte whether a SIB byte or a displacement follows",
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
id = "q-modrm-fields"
kind = "quiz"
prompt = "The ModRM byte `44` is `01|000|100`. What does it say?"
choices = [
  "Register-to-register, rax to rsp",
  "Memory with an 8-bit displacement, destination rax, and a SIB byte follows",
  "Memory with a 32-bit displacement, destination rsp, no SIB byte",
  "RIP-relative addressing with a 32-bit displacement",
]
answer = 1
explanation = "`mod = 01` means a memory operand with a one-byte displacement. `reg = 000` is rax. `r/m = 100` is the escape that means a SIB byte follows — it does not mean rsp, though rsp is register 4 and that is not a coincidence. Read in that order, the byte tells you the shape of the whole instruction before you have seen the rest of it."

[[exercises]]
id = "q-modrm-digit"
kind = "quiz"
prompt = "`83 /0` is `add` and `83 /7` is `cmp`, on the same opcode byte. Where does the digit live?"
choices = [
  "In the low three bits of the opcode byte itself",
  "In the `reg` field of the ModRM byte, which here selects an operation instead of a register",
  "In a prefix byte before the opcode",
  "In the REX byte",
]
answer = 1
explanation = "Opcode `83` is shared by a family of arithmetic operations, so the `reg` field is repurposed as an opcode extension. That frees `mod` and `r/m` to describe the single operand. Any time the reference writes `/digit`, it is telling you the value of `reg`."

[[exercises]]
id = "d-load-rbp"
kind = "disassemble"
prompt = "`48 8b 45 00` — the displacement byte is zero. Why is it there at all, and what is the instruction?"
hex = "488b4500"
expect_text = "mov rax, qword [rbp]"
hints = [
  "ModRM `45` is `01|000|101`: mod=01 means a one-byte displacement follows.",
  "The shorter mod=00 form with r/m=101 was taken for RIP-relative addressing, so `[rbp]` has to pay for a zero.",
]

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

## Anatomy of an instruction

You have now met a byte *inside* an instruction, so it is worth saying plainly
what an instruction is made of. Every x86-64 instruction is a run of optional
fields in a fixed order:

```text
    [prefixes] [REX] opcode [ModRM] [SIB] [displacement] [immediate]
```

Only the opcode is mandatory. Everything else appears or does not, which is why
instructions are 1 to 15 bytes long and why you cannot find the start of one by
counting. The *order* never varies; the *presence* of each field is decided by
the opcode and by ModRM.

Two orderings live in that line and they are not the same. The fields are laid
out left to right in the order the decoder meets them. But a multi-byte
displacement or immediate is a little-endian integer *within* its own field —
`08 00 00 00` is the number 8. Instruction stream: forward. Numbers inside it:
backward.

## The ModRM byte

```text
     7   6   5   4   3   2   1   0
   +---+---+---+---+---+---+---+---+
   |  mod  |    reg    |    r/m    |
   +---+---+---+---+---+---+---+---+
       2 bits    3 bits     3 bits
```

The same 2/3/3 split as the SIB byte, for the same reason: three bits hold a
register number, and REX supplies a fourth bit when you need the upper eight.

`mod` decides how `r/m` is to be read:

| mod  | what `r/m` means                              |
|------|-----------------------------------------------|
| `00` | memory, no displacement bytes                 |
| `01` | memory, followed by a signed 8-bit  displacement  |
| `10` | memory, followed by a signed 32-bit displacement |
| `11` | not memory at all — `r/m` is a register        |

That is the whole register-versus-memory distinction, in two bits. `mov rax,
rbx` and `mov rax, qword [rbx]` differ by `mod`, and nothing else.

`reg` is normally the second operand: the register on the other side of the
`mov`. But some opcodes are shared by a whole family of operations, and there
`reg` carries no register — it selects *which* operation. Opcode `83` is
"arithmetic on r/m with a sign-extended imm8", and the choice of arithmetic
lives in `reg`:

```text
    83 /0   add        83 /4   and
    83 /1   or         83 /5   sub
    83 /7   cmp
```

That `/digit` notation, which the instruction reference uses throughout, *is*
the value of the `reg` field. When you see `ff /2` for an indirect call later
on, it means the same thing: opcode `ff`, `reg` = 2.

`r/m` is the remaining operand, and it has two escape hatches — the two you met
above, now with names:

- **`r/m` = `100`, in any memory form** (`mod` ≠ `11`): a SIB byte follows.
  This is why naming `rsp` as a base costs an extra byte.
- **`mod` = `00`, `r/m` = `101`**: not `[rbp]`, but RIP-relative — a disp32
  follows. This is why `[rbp]` must borrow the `mod=01` form with a zero.

REX, when present, widens all three fields at once: **REX.R** extends `reg`,
**REX.B** extends `r/m` (or the SIB base), and **REX.X** extends the SIB index.
One bit each, which is exactly how sixteen registers fit through three-bit
holes.

## Reading five bytes

Take the bytes from the last exercise in this lesson and do it by hand:

```text
    48 8b 44 24 08

    48              REX.W  -- 64-bit operand size
    8b              mov r64, r/m64  -- load into a register

    44 = 01|000|100
         │   │   └── r/m = 100 -> a SIB byte follows
         │   └────── reg = 000 -> rax, the destination
         └────────── mod = 01  -> memory, with a disp8

    24 = 00|100|100
         │   │   └── base  = 100 -> rsp
         │   └────── index = 100 -> none
         └────────── scale = 00  -> 1

    08              the disp8
```

Assemble that: base `rsp`, no index, displacement 8 — `mov rax, qword
[rsp+0x8]`. Five bytes, and every one of them accounted for.

Now the three loads from the same address family, which differ only in the
register named:

```text
    mov rax, qword [rax]   48 8b 00      00 = 00|000|000   plain memory, done
    mov rax, qword [rsp]   48 8b 04 24   04 = 00|000|100   r/m=100 -> SIB
    mov rax, qword [rbp]   48 8b 45 00   45 = 01|000|101   mod=01, a zero disp8
```

Three registers, three lengths, one operation. `rax` needs nothing extra.
`rsp` trips the SIB escape. `rbp` cannot use `mod=00` because RIP-relative
took it, so it pays for a displacement byte that holds zero.

Type any of these into the playground and open the **Explain** tab: it draws
the same bit ruler over the real ModRM and SIB bytes, field by field. Read a
few instructions there until the split stops needing arithmetic.

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

Note that this is the mirror image of the ModRM escape you already know. In
ModRM, `r/m = 100` was spent on "a SIB byte follows"; in SIB, `index = 100` was
spent on "no index". The same register number, register 4, sacrificed twice —
once in each byte — and `rsp` is register 4. Both of its costs trace back to a
single fact: three-bit fields have no spare encodings, so any new meaning has
to be carved out of a register that already exists.

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
- An instruction is `[prefixes] [REX] opcode [ModRM] [SIB] [disp] [imm]`. Only
  the opcode is required, which is why instructions vary from 1 to 15 bytes.
- ModRM splits `mod|reg|r/m` as 2|3|3. `mod` says register-or-memory and how
  wide the displacement is; `reg` is an operand *or* an opcode extension
  (`/digit`); `r/m` is the other operand.
- Two escapes carve special meanings out of register 4 and register 5:
  `r/m=100` means a SIB follows, and `mod=00, r/m=101` means RIP-relative.
- Scale is 1/2/4/8 because it is two bits wide, which happily matches primitive
  type sizes — so array indexing is free.
- `lea` computes an address and never touches memory: a three-operand add that
  does not set flags.
- `rsp` cannot be an index because its encoding was spent on "no index". `rsp`
  as a base costs a SIB byte; `[rbp]` costs a zero displacement byte.
- `[rip+disp]` addresses data relative to the next instruction, and is what
  makes PIE binaries practical.
