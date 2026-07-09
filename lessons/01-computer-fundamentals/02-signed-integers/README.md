# Signed Integers and Two's Complement

Derives two's complement from the requirement that one adder serve both signed
and unsigned arithmetic, rather than presenting it as a rule to memorise. Covers
negation, the `INT_MIN` asymmetry and the real bug it causes, and finishes by
locating exactly where signedness survives in the instruction set — which is the
foothold for recovering types from a disassembly.

- `examples/negate.asm` — negation by hand (`not` + `inc`) next to `neg`.
- `examples/signed_vs_unsigned.asm` — one `cmp`, two different branches taken.
