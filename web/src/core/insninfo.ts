// Instruction helper: a compact, offline reference for the mnemonics the
// platform supports. Enough for a quick "what does this do and which flags does
// it touch" card without a round-trip; the byte-level encoding of a *specific*
// instruction still comes from /api/asm/explain on demand.
//
// Pure and dependency-free.

export interface InsnInfo {
  mnemonic: string;
  /** One-line plain-English summary. */
  summary: string;
  /** Flags written, or "none". Kept terse and true to the integer core. */
  flags: string;
  /** Family, for the card's subtitle. */
  category: string;
}

// Keyed by the canonical lowercase mnemonic. Condition-carrying forms
// (jcc/setcc/cmovcc) are resolved by prefix below.
const TABLE: Record<string, Omit<InsnInfo, "mnemonic">> = {
  mov: { summary: "Copy the source into the destination.", flags: "none", category: "data movement" },
  movzx: { summary: "Copy a smaller value into a wider register, filling the top with zeroes.", flags: "none", category: "data movement" },
  movsx: { summary: "Copy a smaller value into a wider register, replicating its sign bit.", flags: "none", category: "data movement" },
  movsxd: { summary: "Sign-extend a 32-bit value into a 64-bit register.", flags: "none", category: "data movement" },
  lea: { summary: "Compute an address and store it — no memory is touched, no flags change.", flags: "none", category: "address arithmetic" },
  push: { summary: "Decrement rsp by the operand size, then store the operand at [rsp].", flags: "none", category: "stack" },
  pop: { summary: "Load from [rsp], then increment rsp.", flags: "none", category: "stack" },
  xchg: { summary: "Swap the contents of the two operands.", flags: "none", category: "data movement" },
  add: { summary: "Add the source into the destination.", flags: "OF SF ZF AF PF CF", category: "arithmetic" },
  sub: { summary: "Subtract the source from the destination.", flags: "OF SF ZF AF PF CF", category: "arithmetic" },
  adc: { summary: "Add source plus the carry flag into the destination.", flags: "OF SF ZF AF PF CF", category: "arithmetic" },
  sbb: { summary: "Subtract source and the carry flag from the destination.", flags: "OF SF ZF AF PF CF", category: "arithmetic" },
  inc: { summary: "Add one — deliberately without touching the carry flag.", flags: "OF SF ZF AF PF (not CF)", category: "arithmetic" },
  dec: { summary: "Subtract one — deliberately without touching the carry flag.", flags: "OF SF ZF AF PF (not CF)", category: "arithmetic" },
  neg: { summary: "Replace the operand with its two's-complement negation.", flags: "OF SF ZF AF PF CF", category: "arithmetic" },
  mul: { summary: "Unsigned multiply of the accumulator by the operand; result in rdx:rax.", flags: "OF CF (SF ZF AF PF undefined)", category: "arithmetic" },
  imul: { summary: "Signed multiply.", flags: "OF CF (others undefined)", category: "arithmetic" },
  div: { summary: "Unsigned divide rdx:rax by the operand; quotient to rax, remainder to rdx.", flags: "all undefined", category: "arithmetic" },
  idiv: { summary: "Signed divide rdx:rax by the operand.", flags: "all undefined", category: "arithmetic" },
  and: { summary: "Bitwise AND the source into the destination.", flags: "SF ZF PF (clears OF, CF)", category: "logic" },
  or: { summary: "Bitwise OR the source into the destination.", flags: "SF ZF PF (clears OF, CF)", category: "logic" },
  xor: { summary: "Bitwise exclusive-OR. `xor r, r` is the idiomatic zero.", flags: "SF ZF PF (clears OF, CF)", category: "logic" },
  not: { summary: "Flip every bit of the operand.", flags: "none", category: "logic" },
  cmp: { summary: "Subtract, set the flags, and discard the result — a comparison.", flags: "OF SF ZF AF PF CF", category: "compare" },
  test: { summary: "Bitwise AND, set the flags, and discard the result. `test r, r` tests for zero.", flags: "SF ZF PF (clears OF, CF)", category: "compare" },
  shl: { summary: "Shift left, filling with zeroes. CF holds the last bit shifted out.", flags: "CF OF SF ZF PF", category: "shift" },
  sal: { summary: "Arithmetic shift left — identical to shl.", flags: "CF OF SF ZF PF", category: "shift" },
  shr: { summary: "Shift right, filling with zeroes (unsigned divide by a power of two).", flags: "CF OF SF ZF PF", category: "shift" },
  sar: { summary: "Shift right, replicating the sign bit (signed divide by a power of two).", flags: "CF OF SF ZF PF", category: "shift" },
  rol: { summary: "Rotate left.", flags: "CF OF", category: "shift" },
  ror: { summary: "Rotate right.", flags: "CF OF", category: "shift" },
  rcl: { summary: "Rotate left through the carry flag.", flags: "CF OF", category: "shift" },
  rcr: { summary: "Rotate right through the carry flag.", flags: "CF OF", category: "shift" },
  jmp: { summary: "Unconditional jump.", flags: "none", category: "control flow" },
  call: { summary: "Push the address of the next instruction, then jump.", flags: "none", category: "control flow" },
  ret: { summary: "Pop a return address off the stack and jump to it.", flags: "none", category: "control flow" },
  leave: { summary: "Restore rsp from rbp, then pop rbp — undo a standard prologue.", flags: "none", category: "control flow" },
  syscall: { summary: "Trap into the kernel; the call number is in rax.", flags: "kernel-defined", category: "system" },
  int3: { summary: "One-byte breakpoint trap — how a debugger patches any instruction.", flags: "none", category: "system" },
  int: { summary: "Raise the software interrupt named by the operand.", flags: "none", category: "system" },
  hlt: { summary: "Halt the processor until an interrupt arrives.", flags: "none", category: "system" },
  ud2: { summary: "Raise an invalid-opcode fault; execution never continues past here.", flags: "none", category: "system" },
  nop: { summary: "Do nothing.", flags: "none", category: "system" },
  endbr64: { summary: "A valid indirect-branch landing pad, required by CET.", flags: "none", category: "system" },
  bswap: { summary: "Reverse the byte order of the operand — convert endianness.", flags: "none", category: "data movement" },
  cdq: { summary: "Sign-extend eax into edx:eax, preparing for a signed divide.", flags: "none", category: "arithmetic" },
  cqo: { summary: "Sign-extend rax into rdx:rax.", flags: "none", category: "arithmetic" },
  cdqe: { summary: "Sign-extend eax into rax.", flags: "none", category: "arithmetic" },
};

const CONDITIONS: Record<string, string> = {
  o: "overflow (OF=1)",
  no: "no overflow (OF=0)",
  b: "below / carry — unsigned < (CF=1)",
  c: "carry (CF=1)",
  ae: "above-or-equal — unsigned >= (CF=0)",
  nb: "not below — unsigned >= (CF=0)",
  e: "equal / zero (ZF=1)",
  z: "zero (ZF=1)",
  ne: "not equal / not zero (ZF=0)",
  nz: "not zero (ZF=0)",
  be: "below-or-equal — unsigned <=",
  a: "above — unsigned >",
  s: "sign — negative (SF=1)",
  ns: "not sign (SF=0)",
  p: "parity even (PF=1)",
  np: "parity odd (PF=0)",
  l: "less — signed < (SF≠OF)",
  ge: "greater-or-equal — signed >=",
  le: "less-or-equal — signed <=",
  g: "greater — signed >",
};

/** Look up a mnemonic, resolving jcc/setcc/cmovcc condition suffixes. */
export function lookupInsn(mnemonic: string): InsnInfo | null {
  const m = mnemonic.toLowerCase();
  const base = TABLE[m];
  if (base) return { mnemonic: m, ...base };

  const cond = (prefix: string, verb: string): InsnInfo | null => {
    if (!m.startsWith(prefix)) return null;
    const c = CONDITIONS[m.slice(prefix.length)];
    if (!c) return null;
    return {
      mnemonic: m,
      summary: `${verb} if the condition holds: ${c}.`,
      flags: "none (reads flags)",
      category: "conditional",
    };
  };

  return (
    cond("cmov", "Conditionally copy the source into the destination") ??
    cond("set", "Set the byte operand to 1 (else 0)") ??
    // `j` last, so it does not shadow `jmp` (handled in TABLE).
    (m !== "jmp" ? cond("j", "Jump") : null)
  );
}

/** Is this word one of the mnemonics we can describe? */
export function isKnownMnemonic(word: string): boolean {
  return lookupInsn(word) !== null;
}
