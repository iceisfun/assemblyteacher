+++
id = "bitwise-shifts"
title = "Bitwise Operations and Shifts"
order = 4
estimated_minutes = 35
objectives = [
  "Use and, or, xor and test to mask, set, toggle and inspect bits",
  "Explain the difference between logical and arithmetic right shifts",
  "Recognise sign extension and zero extension as different interpretations of the same low bits",
]
prerequisites = ["signed-integers", "endianness"]

[[exercises]]
id = "q-mask-byte"
kind = "quiz"
prompt = "You want the low byte of rax and do not care about the other bits. Which operation expresses that?"
choices = ["or rax, 0xff", "and rax, 0xff", "xor rax, 0xff", "test rax, 0xff"]
answer = 1
explanation = "A mask keeps the 1 bits and clears the 0 bits. `and rax, 0xff` preserves only the low eight bits."

[[exercises]]
id = "q-test-vs-and"
kind = "quiz"
prompt = "Why do compilers often emit `test eax, eax` before a zero/non-zero branch?"
choices = ["It clears eax", "It computes eax AND eax, discards the result, and keeps the flags", "It is a faster spelling of cmp eax, 1", "It sign-extends eax"]
answer = 1
explanation = "`test` is to `and` what `cmp` is to `sub`: compute flags, suppress the destination write."

[[exercises]]
id = "a-toggle-low-bit"
kind = "assemble"
prompt = "Write the instruction that toggles bit 0 of eax."
starter = ""
solution = "xor eax, 1"
expect_hex = "83f001"
hints = ["XOR with 1 flips a bit; XOR with 0 leaves a bit alone."]

[[exercises]]
id = "e-pack-nibbles"
kind = "emulate"
prompt = "Put 0xa in al, shift it into the high nibble, OR in 0x5, and halt with rax = 0xa5."
starter = """
    mov eax, 0xa
    ; shift left by 4 and add the low nibble with or
    hlt
"""
solution = """
    mov eax, 0xa
    shl eax, 4
    or eax, 0x5
    hlt
"""
expect_registers = { rax = 165 }
hints = ["A left shift by 4 moves a nibble into the next hex digit.", "Use `or eax, 0x5` to set the low nibble without disturbing the high nibble."]
+++

# Bitwise Operations and Shifts

A byte is eight yes/no answers. Arithmetic treats those answers as a number;
bitwise operations let you work with the answers directly.

## Masks

`and` keeps bits. `or` sets bits. `xor` toggles bits. The immediate value is a
mask: every 1 bit says "touch this position", and every 0 bit says "leave or
clear it", depending on the operation.

```asm
    and eax, 0xff      ; keep only the low byte
    or  eax, 0x20      ; set bit 5
    xor eax, 1         ; toggle bit 0
```

This is why flags, permissions, and packed fields show up as powers of two. A
single integer can carry many independent facts.

## Testing without changing

`test` is `and` with the result discarded. It is how code asks "are these bits
set?" without destroying the value being inspected.

```asm
    test eax, eax      ; sets ZF if eax is zero
    jz is_zero
```

That is not magic syntax for zero comparison. `eax & eax` is zero exactly when
`eax` is zero.

## Shifts

Left shift multiplies an unsigned integer by powers of two until bits fall off
the top. Logical right shift fills the top with zeroes. Arithmetic right shift
copies the sign bit, which preserves the sign of a two's-complement value.

```asm
    shl eax, 4         ; move a nibble left
    shr eax, 1         ; unsigned divide by two
    sar eax, 1         ; signed divide by two-ish, keeping the sign
```

The "ish" matters: signed division rounds toward zero, while arithmetic shift
rounds toward negative infinity for negative odd numbers.

## Extension

Zero extension and sign extension both copy a smaller value into a larger place.
The difference is what fills the new high bits. Unsigned values fill with zero.
Signed values fill with the old sign bit.

This is not a property stored in memory. It is a choice made by the instruction
that widens the value.

## Key points

- Masks are values chosen so each bit position has a separate meaning.
- `test` asks about bits without changing the operand.
- `shr` is an unsigned right shift; `sar` is a signed right shift.
- Sign extension and zero extension are operations, not metadata attached to bytes.
