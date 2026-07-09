+++
id = "binary-and-hexadecimal"
title = "Binary and Hexadecimal"
order = 1
estimated_minutes = 25
objectives = [
  "Convert between binary, hexadecimal and decimal without a calculator",
  "Explain why hexadecimal, and not octal or decimal, is the notation of choice for machine code",
  "Read a byte as both a number and a bit pattern, and know when each reading is the useful one",
]
prerequisites = []

[[exercises]]
id = "q-binary-value"
kind = "quiz"
prompt = "What is `0b1011` in decimal?"
choices = ["9", "11", "13", "15"]
answer = 1
explanation = "8 + 0 + 2 + 1 = 11. The set bits are at positions 3, 1 and 0."

[[exercises]]
id = "q-hex-digit"
kind = "quiz"
prompt = "How many bits does a single hexadecimal digit represent?"
choices = ["2", "4", "8", "16"]
answer = 1
explanation = "A hex digit spans 0..15, which is exactly four bits. Two hex digits make one byte — which is why hex, not octal, won."

[[exercises]]
id = "a-load-binary"
kind = "assemble"
prompt = "Write one instruction that loads the value `0b1011` into `al`."
starter = "mov al, "
solution = "mov al, 0b1011"
expect_hex = "b00b"
hints = [
  "The assembler accepts `0b`, `0x` and plain decimal literals — all three mean the same number.",
  "`mov al, 11` and `mov al, 0x0b` assemble to exactly the same two bytes.",
]

[[exercises]]
id = "d-read-mov"
kind = "disassemble"
prompt = "These bytes are one instruction: `b8 2a 00 00 00`. What is it?"
hex = "b82a000000"
expect_text = "mov eax, 0x2a"
hints = [
  "`b8+r` is `mov r32, imm32`. The `+r` means the register number is baked into the opcode byte itself.",
  "The four immediate bytes are little-endian.",
]
+++

# Binary and Hexadecimal

A computer does not store numbers. It stores voltages, and we agree to read
them as numbers. Everything above that agreement — integers, text, pointers,
instructions — is interpretation.

This lesson is about the two notations we use to talk about those bits, and
about why one of them turned out to matter far more than the other.

## Counting in base 2

A positional number system assigns each digit a weight. In base 10 the weights
are powers of ten; in base 2 they are powers of two:

```text
  0b1011
    │││└─ 1 × 2^0 = 1
    ││└── 1 × 2^1 = 2
    │└─── 0 × 2^2 = 0
    └──── 1 × 2^3 = 8
                   ──
                   11
```

That is the whole idea. A bit is a digit that can only be 0 or 1, so the
"multiply by the digit" step degenerates into "include this weight, or don't".
Reading binary is adding up the powers of two where a bit is set.

Eight bits make a **byte**, and a byte can hold 2⁸ = 256 distinct patterns.
Whether those patterns mean 0..255, or −128..127, or a character, or part of a
pointer, is not something the byte knows.

## Why hexadecimal

Binary is unreadable at length. `01001000 10001001 11100101` is three bytes and
already hard to hold in your head. Decimal is worse: it hides the bit structure
entirely, because 10 is not a power of 2, so a decimal digit does not correspond
to any fixed number of bits.

Hexadecimal — base 16 — is the compromise, and it wins for one reason:

> **16 = 2⁴, so one hex digit is exactly four bits, always.**

Nothing is hidden and nothing is smeared across digit boundaries. Converting is
mechanical: chop the bits into groups of four and look each group up.

```text
  0100 1000   1000 1001   1110 0101
    4    8      8    9      e    5
   0x48        0x89        0xe5
```

Two hex digits per byte, no arithmetic, no carrying. This is why every hex dump,
every debugger, and every disassembler you will ever use speaks hex.

| binary | hex | decimal |   | binary | hex | decimal |
|--------|-----|---------|---|--------|-----|---------|
| `0000` | `0` | 0       |   | `1000` | `8` | 8       |
| `0001` | `1` | 1       |   | `1001` | `9` | 9       |
| `0010` | `2` | 2       |   | `1010` | `a` | 10      |
| `0011` | `3` | 3       |   | `1011` | `b` | 11      |
| `0100` | `4` | 4       |   | `1100` | `c` | 12      |
| `0101` | `5` | 5       |   | `1101` | `d` | 13      |
| `0110` | `6` | 6       |   | `1110` | `e` | 14      |
| `0111` | `7` | 7       |   | `1111` | `f` | 15      |

Octal (base 8) has the same property — one digit is three bits — but three does
not divide eight. An octal digit straddles byte boundaries, so a byte needs two
and two-thirds octal digits. Hex needs exactly two. That is the entire argument,
and it is why octal survives only in Unix file permissions, where the fields
really are three bits wide.

## Bits are not the number

Here is `0x48`, one byte, read three ways:

```text
  bits:      0100 1000
  as u8:     72
  as ASCII:  'H'
  as x86:    the REX.W prefix, announcing a 64-bit operand
```

All three readings are correct at the same time. The byte is `0x48`; what it
*means* depends on who is looking at it and why. When you disassemble a program
and the disassembler goes off the rails halfway through a function, this is
exactly what happened: it started reading data as if it were instructions, and
the bytes did not object.

Hold onto that. It is the single most important idea in this course, and the
rest of it is consequences.

## Reading machine code

Load these five bytes into the memory viewer and look at them:

```text
  b8 2a 00 00 00
```

`0xb8` is the opcode for "move a 32-bit immediate into `eax`", with the
register number encoded in the low three bits of the opcode byte itself. The
remaining four bytes are the immediate, `0x0000002a`, stored least-significant
byte first — which is a whole lesson of its own, coming up shortly.

`0x2a` is 42.

## Try it

Assemble `mov al, 0b1011` in the playground and look at the two bytes it
produces. Then change the literal to `11`, and to `0x0b`. The bytes do not
move. The assembler does not remember what base you typed a number in, because
by the time it emits a byte there is no base left — only bits.

## Key points

- A bit is a digit in base 2; a byte is eight of them, with 256 possible values.
- Hex is used everywhere because one hex digit is exactly four bits, so hex
  never hides or splits the bit structure. Two hex digits is always one byte.
- A byte's *meaning* is imposed from outside. The same eight bits are a number,
  a character, and an instruction prefix, all at once.
