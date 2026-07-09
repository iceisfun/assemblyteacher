# Addressing Modes

The single formula `[base + index*scale + disp]` that every x86_64 memory
operand fits inside, derived from the SIB byte's bit layout rather than
presented as a table to memorise. Explains `lea` as a three-operand non-flag-
setting adder, and derives the `rsp`-cannot-be-an-index rule directly from the
encoding.

- `examples/shapes.asm` — every addressing shape, one per line.
- `examples/lea_arithmetic.asm` — `lea` used purely as arithmetic.
- `examples/encoding_quirks.asm` — `[rax]`, `[rsp]` and `[rbp]`, and their three
  different lengths.
