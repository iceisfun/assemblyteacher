# Registers

The sixteen general purpose registers, their four widths, and the zero-extension
rule — presented as a consequence of partial-register dependency stalls rather
than as trivia. Covers the `ah`/`spl` encoding collision, why `xor eax, eax` is
the canonical zero, and the flags register with the CF-versus-OF distinction.

- `examples/widths.asm` — the zero-extension rule demonstrated three ways.
- `examples/zeroing.asm` — four ways to zero a register, and their byte counts.
- `examples/flags.asm` — one addition, two overflow readings.
