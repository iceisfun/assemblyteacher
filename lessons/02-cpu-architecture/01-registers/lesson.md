+++
id = "registers"
title = "Registers"
order = 1
estimated_minutes = 35
objectives = [
  "Name the sixteen general purpose registers and the four widths each is addressable at",
  "State the zero-extension rule and predict the contents of a register after a partial write",
  "Explain why `xor eax, eax` is the idiomatic way to zero a 64-bit register",
  "Read the flags register and say which instructions set which flags",
]
prerequisites = ["binary-and-hexadecimal", "signed-integers"]

[[exercises]]
id = "q-zero-extension"
kind = "quiz"
prompt = "`rax` holds `0xffffffffffffffff`. You execute `mov eax, 5`. What is in `rax` afterwards?"
choices = [
  "0xffffffff00000005",
  "0x0000000000000005",
  "0xffffffffffffff05",
  "0x00000005ffffffff",
]
answer = 1
explanation = "A 32-bit write zero-extends into the full 64-bit register. This is the only width that does. An 8- or 16-bit write would have left the upper bits alone."

[[exercises]]
id = "a-zero-eax"
kind = "assemble"
prompt = "Write the shortest instruction that sets `rax` to zero. (Two bytes is possible.)"
starter = ""
solution = "xor eax, eax"
expect_hex = "31c0"
hints = [
  "`mov rax, 0` is seven bytes: a REX prefix, an opcode, a ModRM byte, and a four-byte immediate.",
  "A register exclusive-ORed with itself is always zero — and no immediate is needed.",
  "Work on the 32-bit name. The write zero-extends, so all 64 bits are cleared anyway.",
]

[[exercises]]
id = "e-zero-extend"
kind = "emulate"
prompt = "Fill `rax` with ones, then write 5 into `eax`. Halt. If the zero-extension rule is what the lesson claims, `rax` will be exactly 5."
starter = """
mov rax, -1
; now write 5 into the 32-bit name
hlt
"""
solution = """
mov rax, -1
mov eax, 5
hlt
"""
expect_registers = { rax = 5 }
hints = ["Write to `eax`, not to `rax`.", "The upper 32 bits are cleared for you."]

[[exercises]]
id = "e-partial-write"
kind = "emulate"
prompt = "Now show the *other* half of the rule. Set `eax` to all ones, then write 0 into `al` only. The upper bits of the 32-bit value must survive: `rax` should end as `0xffffff00`."
starter = """
mov eax, -1
; now write 0 into the 8-bit name
hlt
"""
solution = """
mov eax, -1
mov al, 0
hlt
"""
expect_registers = { rax = 4294967040 }
hints = [
  "`0xffffff00` is 4294967040.",
  "An 8-bit write is a *merge* into the existing register, not a replacement.",
]
+++

# Registers

Memory is far away. Even an L1 cache hit costs several cycles, and a trip to
DRAM costs hundreds. Registers are the handful of storage slots that live inside
the execution core, and they are the only place the ALU can reach in zero time.
Almost every instruction you will read spends its operands out of registers and
puts its result back into one.

x86_64 gives you sixteen general purpose registers of 64 bits each. That is
128 bytes of the fastest storage in the machine, and essentially all of a
program's moment-to-moment work happens inside it.

## The sixteen

```text
  rax  rcx  rdx  rbx  rsp  rbp  rsi  rdi
  r8   r9   r10  r11  r12  r13  r14  r15
```

The first eight have names inherited from the 16-bit 8086: *accumulator*,
*counter*, *data*, *base*, *stack pointer*, *base pointer*, *source index*,
*destination index*. Those names described what the instruction set forced them
to do in 1978. Today they are nearly all general purpose, but the history has
not entirely washed out:

- **`rsp`** really is the stack pointer. `push`, `pop`, `call` and `ret` modify
  it implicitly. It is not general purpose.
- **`rcx`** is still the counter: it is the only register a variable shift count
  may live in (`shl rax, cl`).
- **`rdx:rax`** is still the implicit operand pair for `mul`, `div` and their
  signed twins.
- **`rbp`** is conventionally the frame pointer, but that is a convention, and
  optimising compilers reclaim it as a general register.

The odd ordering above (`rax rcx rdx rbx`, not `rax rbx rcx rdx`) is not a typo.
It is the *encoding* order: `rax` is register 0, `rcx` is 1, `rdx` is 2, `rbx`
is 3. When you read a ModRM byte by hand, that is the order you need.

## Four widths, one register

Each register can be named at four widths, and the name you use is part of the
instruction encoding:

```text
   63                             31              15      7      0
  +--------------------------------+---------------+-------+------+
  |                              rax                               |
  |                                |             eax               |
  |                                |               |      ax       |
  |                                |               |  ah   |  al   |
  +--------------------------------+---------------+-------+------+
```

`r8` through `r15` follow the same pattern with a suffix: `r8d`, `r8w`, `r8b`.

## The rule that catches everyone

> **Writing a 32-bit register zeroes the upper 32 bits. Writing an 8- or 16-bit
> register does not.**

```asm
    mov rax, -1         ; rax = 0xffffffffffffffff
    mov eax, 5          ; rax = 0x0000000000000005   <- upper half cleared!
```

```asm
    mov rax, -1         ; rax = 0xffffffffffffffff
    mov ax, 5           ; rax = 0xffffffffffff0005   <- upper bits survive
    mov al, 5           ; rax = 0xffffffffffff0005 -> ...ff05
```

Only the 32-bit width zero-extends. This looks arbitrary, and it is — it is a
decision AMD made when they extended the architecture, and they made it for a
good reason: **partial-register writes are expensive.** If `mov ax, 5` must
preserve the other 48 bits, the processor has to merge the new value with the
old one, which means the write *depends* on the previous value of the register,
which means it cannot be reordered or renamed freely. That dependency is a
pipeline stall waiting to happen.

By making 32-bit writes clobber the whole register, AMD gave the common case a
clean break in the dependency chain. And because the 32-bit form needs no REX
prefix, it is also one byte shorter.

This is why compiled code is full of 32-bit instructions operating on values you
know are 64-bit pointers or counters. `mov eax, edi` is not a bug. It is the
compiler telling you the value fits in 32 bits, and taking the free zero-extend
and the shorter encoding.

And it is why the idiomatic zero is:

```asm
    xor eax, eax        ; 31 c0        -- two bytes, clears all 64 bits
```

versus:

```asm
    mov rax, 0          ; 48 c7 c0 00 00 00 00   -- seven bytes
```

Two bytes instead of seven, no immediate to fetch, and every x86 since the
Pentium Pro special-cases `xor reg, reg` in the renamer: it does not even
execute, it just marks the register as zero. Assemble both in the playground and
compare.

## `ah` and its awkward cousins

`ah`, `ch`, `dh`, `bh` name bits 8..15 of the first four registers — a leftover
from when `ax` was the whole register and you wanted its two halves separately.

When AMD added `r8`..`r15` they needed a prefix byte (REX) to encode the extra
register bits. They also wanted byte access to `rsp`, `rbp`, `rsi`, `rdi`, which
the old encoding could not name. So they reused the encodings:

```text
  no REX prefix:   encodings 4,5,6,7  mean  ah, ch, dh, bh
  any REX prefix:  encodings 4,5,6,7  mean  spl, bpl, sil, dil
```

Consequences, both real:

- `mov al, ah` and `mov al, spl` have the **same ModRM byte**. They differ only
  by the presence of a REX prefix — even a REX byte with no bits set.
- **You cannot use `ah` and `r8b` in the same instruction.** One requires REX,
  the other forbids it. The assembler will refuse.

Try `mov ah, r8b` in the playground. The error you get is the encoding telling
you something true about the machine.

## The flags

`rflags` is not a general register. It is a bag of bits that arithmetic sets as
a side effect, and that conditional branches read.

| flag | name      | set when                                                    |
|------|-----------|-------------------------------------------------------------|
| `CF` | carry     | an unsigned operation carried out of, or borrowed into, the top bit |
| `ZF` | zero      | the result was zero                                          |
| `SF` | sign      | the result's top bit is set (it is negative, read as signed) |
| `OF` | overflow  | a *signed* operation produced a result of the wrong sign     |
| `PF` | parity    | the low **eight bits** of the result have an even number of ones |
| `AF` | adjust    | carry out of bit 3; used only by BCD arithmetic              |
| `DF` | direction | string operations run downwards                              |

CF and OF are the pair worth understanding. Add `0x7fffffffffffffff` and 1:

```text
    signed:    the largest positive + 1  ->  negative.  OF = 1, CF = 0
    unsigned:  a big number + 1          ->  a bigger number, no carry out.
```

Add `0xffffffffffffffff` and 1:

```text
    unsigned:  wraps to 0, carry out of the top bit.    CF = 1
    signed:    -1 + 1 = 0, entirely correct.            OF = 0
```

The hardware sets *both* flags from *one* addition, and lets the program decide
which one it cares about. Same adder, same result bits — two readings, exactly
as in the previous lesson.

Note that `inc` and `dec` deliberately do **not** touch CF, so you can carry a
flag across a loop that also counts. That single exception has caused a
surprising amount of confusion.

## Key points

- Sixteen 64-bit registers; four widths each; `rsp` and (by convention) `rbp`
  are special, and `rcx`/`rdx:rax` retain implicit roles.
- A 32-bit write zero-extends to 64 bits. An 8- or 16-bit write merges. This is
  a deliberate trade of orthogonality for a broken dependency chain.
- `xor eax, eax` is two bytes and free; `mov rax, 0` is seven and is not.
- `ah`/`ch`/`dh`/`bh` and `spl`/`bpl`/`sil`/`dil` share encodings, separated only
  by the presence of a REX prefix, so they cannot appear together.
- Flags are set by arithmetic and read by branches. CF is the unsigned story,
  OF the signed one, from the same result.
