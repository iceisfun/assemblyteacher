+++
id = "endianness"
title = "Endianness"
order = 3
estimated_minutes = 20
objectives = [
  "Predict the byte order of a multi-byte value in a hex dump",
  "Explain why little-endian makes a narrowing cast free",
  "Recognise big-endian data inside a little-endian process, and know where it comes from",
]
prerequisites = ["binary-and-hexadecimal"]

[[exercises]]
id = "d-little-endian-imm"
kind = "disassemble"
prompt = "The bytes `b8 78 56 34 12` are one instruction. What is it? (Note the order of the immediate.)"
hex = "b878563412"
expect_text = "mov eax, 0x12345678"
hints = [
  "`b8+r` is `mov r32, imm32`, followed by four immediate bytes.",
  "Little-endian: the byte at the lowest address is the *least* significant.",
]

[[exercises]]
id = "q-which-byte-first"
kind = "quiz"
prompt = "You store the 32-bit value `0x12345678` at address `0x1000` on x86_64. What single byte is at `0x1000`?"
choices = ["0x12", "0x34", "0x56", "0x78"]
answer = 3
explanation = "x86_64 is little-endian: the least significant byte, 0x78, goes at the lowest address."

[[exercises]]
id = "e-bswap"
kind = "emulate"
prompt = "Put `0x12345678` into `eax`, then reverse its byte order in place. The program should halt with `rax = 0x78563412`. End with `hlt`."
starter = """
mov eax, 0x12345678
; reverse the bytes of eax
hlt
"""
solution = """
mov eax, 0x12345678
bswap eax
hlt
"""
expect_registers = { rax = 2018915346 }
hints = [
  "There is a single instruction whose entire job is reversing byte order.",
  "It is `bswap`. It exists because networks are big-endian and CPUs are not.",
]
+++

# Endianness

A byte has no order — it is eight bits, and we read them most-significant-first
by universal convention. But a 32-bit value is four bytes, and they have to go
into memory *somewhere*, in *some* order. There are two sensible choices, and
the industry made both.

## The two orders

Store `0x12345678` at address `0x1000`:

```text
  little-endian (x86, ARM as configured, RISC-V)
    0x1000: 78   ← least significant byte at the lowest address
    0x1001: 56
    0x1002: 34
    0x1003: 12

  big-endian (network byte order, SPARC, older PowerPC, 68k)
    0x1000: 12   ← most significant byte at the lowest address
    0x1001: 34
    0x1002: 56
    0x1003: 78
```

Big-endian matches how we write numbers on paper: most significant part first.
It reads naturally in a hex dump. Little-endian looks backwards.

x86_64 is little-endian. Every hex dump you take of a running x86 process will
show integers "backwards". You will get used to it faster than you expect,
because you have to.

## Why little-endian is not just perversity

The argument for little-endian is that **the address of a value is the address
of its least significant byte, regardless of how wide you decide it is**.

Say `0x1000` holds the 64-bit value `0x00000000000000ff`. Read one byte at
`0x1000` and you get `0xff`. Read two bytes and you get `0x00ff`. Read four,
eight — you keep getting 255. A narrowing cast is not an operation; it is a
decision to read fewer bytes from the same address.

```text
  little-endian, at 0x1000:  ff 00 00 00 00 00 00 00
    read 1 byte  -> 0xff          ✓ 255
    read 2 bytes -> 0x00ff        ✓ 255
    read 4 bytes -> 0x000000ff    ✓ 255
```

On a big-endian machine the same memory reads as `0xff00`, `0xff000000` — the
value changes with the width, and `(char)x` must read from `&x + 7`, not `&x`.

```text
  big-endian, 255 stored as 8 bytes at 0x1000:  00 00 00 00 00 00 00 ff
    read 1 byte at 0x1000 -> 0x00     ✗ not 255; you must read at 0x1007
```

This is why little-endian machines can be sloppy in a way that occasionally
works: casting an `int*` to a `char*` and dereferencing gives you the low byte,
which is usually what you wanted. On big-endian, the same code silently reads
the high byte. Endianness bugs found this way have a long and boring history.

Neither order is faster. The argument is entirely about which convenience you
prefer, and little-endian won the desktop.

## Big-endian is still here

Two places you will meet it constantly:

**Network protocols.** TCP/IP fixed its byte order in the 1980s, and it chose
big-endian. Every port number, every IP address, every length field on the wire
is big-endian, which is why it is called *network byte order* and why every
socket program is littered with `htons` and `ntohl`. On a little-endian machine
those are byte swaps; on a big-endian machine they compile to nothing.

**Human-facing constants.** Magic numbers are chosen to be readable in a hex
dump, which means they are written big-endian even in little-endian files. ELF
begins with `7f 45 4c 46` — `\x7f`, `E`, `L`, `F` — as *bytes*, in that order.
It is not an integer that happens to look like that; it is a byte string, and
byte strings have no endianness. That distinction matters when you parse
headers: `e_ident` is bytes, `e_entry` is a little-endian integer, and they sit
next to each other in the same struct.

The CPU offers `bswap` (and `movbe`) for exactly this. One instruction, reverse
the bytes.

## Where you will actually see this

Look at the immediate in `mov eax, 0x12345678`:

```text
  b8 78 56 34 12
  ▲  ╰──────────╯
  │       the immediate, little-endian
  └── opcode: mov eax, imm32
```

The opcode comes first because it is *earlier in the instruction stream* — that
is not endianness, it is just order. The four immediate bytes are stored
least-significant-first because they are an integer, and this is a little-endian
machine.

Getting these two straight — instruction stream order versus integer byte order
— is the thing that trips people up. The instruction stream is read low address
to high. Integers inside it are little-endian. Both are true simultaneously.

## Key points

- x86_64 stores integers least-significant-byte first.
- The payoff: the address of a value does not depend on its width, so narrowing
  is free and `(char*)&x` gives the low byte.
- Byte *strings* (magic numbers, ASCII, hashes) have no endianness. Only
  multi-byte *integers* do. Headers mix both.
- Network byte order is big-endian, so `bswap` earns its keep.
