+++
id = "arrays-strings-structs"
title = "Arrays, Strings and Structs"
order = 1
estimated_minutes = 40
objectives = [
  "Compute array element addresses from base, index and element size",
  "Compare NUL-terminated strings with length-carrying string values",
  "Read struct field accesses as fixed offsets from a base pointer",
]
prerequisites = ["addressing-modes", "reading-compiled-code"]

[[exercises]]
id = "q-array-address"
kind = "quiz"
prompt = "An array of 8-byte elements starts at 0x1000. Where is element index 3?"
choices = ["0x1003", "0x1008", "0x1018", "0x1030"]
answer = 2
explanation = "Array indexing is base + index * element_size. 0x1000 + 3 * 8 = 0x1018."

[[exercises]]
id = "q-c-string"
kind = "quiz"
prompt = "What tells `strlen` where a C string ends?"
choices = ["A hidden length field before every pointer", "A zero byte in the byte stream", "The page boundary", "The stack frame size"]
answer = 1
explanation = "A C string is just bytes ending at the first NUL byte. The pointer alone carries no length."

[[exercises]]
id = "e-field-address"
kind = "emulate"
prompt = "A struct starts at 0x2000 and field `count` is at offset 16. Compute the field address in rax and halt with rax = 0x2010."
starter = """
    mov rax, 0x2000
    ; compute field address
    hlt
"""
solution = """
    mov rax, 0x2000
    add rax, 16
    hlt
"""
expect_registers = { rax = 8208 }
hints = ["A field access is base pointer plus a compile-time offset."]

[[exercises]]
id = "d-array-load"
kind = "disassemble"
prompt = "These bytes encode a scaled-index load: `48 8b 04 f7`. What instruction are they?"
hex = "488b04f7"
expect_text = "mov rax, qword [rdi+rsi*8]"
hints = ["The scale of 8 is exactly what you expect for an array of qwords."]
+++

# Arrays, Strings and Structs

Source languages give names to data. Machine code gets addresses, sizes, and
offsets. Reverse engineering data structures is the art of recovering those
names from repeated address arithmetic.

## Arrays

An array is contiguous elements of one size. Element `i` is at:

```text
base + i * element_size
```

That is why x86 addressing modes have a scale field of 1, 2, 4, or 8. They match
common element sizes directly.

## Strings

A C string is a pointer to bytes ending at zero. The pointer does not know the
length, so every operation that needs the length must scan or be told a separate
limit.

Many modern string types carry pointer, length, and sometimes capacity. That
costs more metadata, but it makes embedded zero bytes and O(1) length possible.

## Structs

A struct is fields at fixed offsets. `mov eax, [rdi+16]` often means "load the
32-bit field at offset 16 of the object pointed to by rdi." Names are gone; the
offset survives.

Padding exists because many fields are faster, simpler, or required to be placed
at aligned addresses. The layout is not just the sum of field sizes.

## Key points

- Arrays are base plus scaled index.
- C strings end at a zero byte; richer strings carry length explicitly.
- Struct fields compile to fixed offsets from a base pointer.
- Padding and alignment are part of the real layout.
