# AOB Scanning: Finding Code by Its Bytes

Signature (array-of-bytes) scanning: how to locate a piece of code by its stable
instruction bytes when its address will not hold still across builds, recompiles,
and ASLR. The lesson teaches the one idea that makes signatures work — an
instruction splits into stable opcode bytes and volatile operand bytes, and
wildcards (`??`) go on exactly the volatile ones. It uses the relative branch
`E8 ?? ?? ?? ??` as the archetype (the rel32 displacement is a distance that
moves every build), covers short (rel8) vs near (rel32) forms and how they change
a pattern's length, and tackles the alignment hazard that follows from x86's
variable-length encoding — why a byte match must land on an instruction boundary
and how surrounding stable bytes anchor a signature.

It is a neutral RE-reference technique (the basis of YARA/AV signatures,
instrumentation, and cross-version function-finding), framed as "naming code by
what it is, not where it is."

Prerequisites: **Reading Compiled Code** (knowing opcode from operand) and
**Addressing Modes** (RIP-relative displacements).
