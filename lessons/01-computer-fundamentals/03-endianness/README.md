# Endianness

Why integers appear "backwards" in every x86 hex dump, why that ordering is
actually the convenient one, and why magic numbers like `\x7fELF` are not
affected by it. Ends on the distinction that causes most of the confusion:
instruction-stream order versus integer byte order.

- `examples/byte_order.asm` — store a qword, read back its lowest byte.
- `examples/bswap.asm` — network byte order, in one instruction.
