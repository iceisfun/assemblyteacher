// Instruction reference: an offline, teaching-oriented model of the x86-64
// mnemonics the platform supports. Each entry carries enough to answer "what
// does this do, when would I use it, what does it touch, how is it encoded, and
// what's related" without a round-trip. The byte-level encoding of a *specific*
// instruction still comes from /api/asm/explain on demand.
//
// `InsnInfo` / `lookupInsn` / `isKnownMnemonic` are the narrow projection the
// hover chips and tokenizer use; `InsnEntry` / `lookupInsnEntry` /
// `allInsnEntries` are the rich model the full reference page renders.
//
// Pure and dependency-free.

/** The architectural flags, in the conventional display order. */
export const FLAGS = ["CF", "OF", "SF", "ZF", "AF", "PF"] as const;
export type Flag = (typeof FLAGS)[number];

/** What an instruction does to a flag. `preserved` is the default (a flag not
 *  named), so it is never stored — only the interesting effects are. */
export type FlagEffect = "written" | "cleared" | "set" | "undefined" | "tested";

export interface InsnEntry {
  mnemonic: string;
  /** Family, used as a subtitle and for the category browse. */
  category: string;
  /** One-line plain-English summary — the hover-card headline. */
  summary: string;
  /** A short paragraph: what it does and why you'd reach for it. */
  description: string;
  /** Operand forms, most common first (e.g. "mov r/m64, r64"). */
  syntax: string[];
  /** Terse, human flag string — kept true to the integer core. Back-compat. */
  flags: string;
  /** Structured flag effects, for the colour-coded flag panel. Only the
   *  affected flags appear; anything absent is preserved. */
  flagEffects: Partial<Record<Flag, FlagEffect>>;
  /** Progressively richer worked examples, each openable in the Playground. */
  examples: string[];
  /** A note on how the instruction is encoded, when it's illuminating. */
  encoding?: string;
  /** Architectural notes, gotchas, common misconceptions. */
  notes: string[];
  /** Adjacent mnemonics worth exploring. */
  related: string[];
}

// Reusable flag-effect sets ---------------------------------------------------

// The full arithmetic status set: every flag reflects the result.
const ARITH: Partial<Record<Flag, FlagEffect>> = {
  CF: "written", OF: "written", SF: "written", ZF: "written", AF: "written", PF: "written",
};
// inc/dec: like arithmetic but CF is deliberately left alone.
const ARITH_NO_CF: Partial<Record<Flag, FlagEffect>> = {
  OF: "written", SF: "written", ZF: "written", AF: "written", PF: "written",
};
// Logic: result flags set, OF and CF forced to 0, AF undefined.
const LOGIC: Partial<Record<Flag, FlagEffect>> = {
  CF: "cleared", OF: "cleared", SF: "written", ZF: "written", AF: "undefined", PF: "written",
};
// Shifts: CF is the last bit out, OF defined for count 1, result flags set.
const SHIFT: Partial<Record<Flag, FlagEffect>> = {
  CF: "written", OF: "written", SF: "written", ZF: "written", AF: "undefined", PF: "written",
};
// Rotates: only CF (and OF for count 1) move; the data flags are untouched.
const ROTATE: Partial<Record<Flag, FlagEffect>> = { CF: "written", OF: "written" };
// Multiply: CF/OF say "the result didn't fit the low half"; the rest undefined.
const MULFLAGS: Partial<Record<Flag, FlagEffect>> = {
  CF: "written", OF: "written", SF: "undefined", ZF: "undefined", AF: "undefined", PF: "undefined",
};
const ALL_UNDEF: Partial<Record<Flag, FlagEffect>> = {
  CF: "undefined", OF: "undefined", SF: "undefined", ZF: "undefined", AF: "undefined", PF: "undefined",
};

// The catalog ----------------------------------------------------------------
// Keyed by the canonical lowercase mnemonic. Condition-carrying forms
// (jcc/setcc/cmovcc) are generated on demand below.

// In the literal table, `notes` may be omitted; it is normalised to [] on read.
type Entry = Omit<InsnEntry, "mnemonic" | "notes"> & { notes?: string[] };

const TABLE: Record<string, Entry> = {
  mov: {
    category: "data movement",
    summary: "Copy the source into the destination.",
    description:
      "The workhorse of the instruction set: copy a value between registers, memory, and immediates. It does not touch flags, so it can sit freely between a comparison and the branch that reads its result.",
    syntax: ["mov r/m64, r64", "mov r64, r/m64", "mov r64, imm64", "mov r/m8, imm8"],
    flags: "none",
    flagEffects: {},
    examples: ["mov eax, 42", "mov rax, rbx", "mov [rsp+8], eax", "mov rax, [rbx]"],
    encoding:
      "Register/memory forms use ModRM (opcodes 88–8B); the r64,imm64 form is the only true 64-bit-immediate move (opcode B8+rd, 10 bytes with REX.W).",
    notes: [
      "There is no memory-to-memory mov; one operand must be a register or immediate.",
      "Writing a 32-bit register (mov eax, …) zero-extends into the full 64-bit register.",
    ],
    related: ["movzx", "movsx", "lea", "xchg", "push", "pop"],
  },
  movzx: {
    category: "data movement",
    summary: "Copy a smaller value into a wider register, filling the top with zeroes.",
    description:
      "Zero-extend: load an 8- or 16-bit source into a 32- or 64-bit register, setting all the new high bits to zero. The way to widen an unsigned value.",
    syntax: ["movzx r32, r/m8", "movzx r64, r/m16"],
    flags: "none",
    flagEffects: {},
    examples: ["movzx eax, byte [rsi]", "movzx rax, ax"],
    notes: ["Use for unsigned widening; use movsx for signed."],
    related: ["movsx", "movsxd", "mov"],
  },
  movsx: {
    category: "data movement",
    summary: "Copy a smaller value into a wider register, replicating its sign bit.",
    description:
      "Sign-extend: load an 8- or 16-bit source into a wider register, copying the source's top bit across all the new high bits so the two's-complement value is preserved.",
    syntax: ["movsx r32, r/m8", "movsx r64, r/m16"],
    flags: "none",
    flagEffects: {},
    examples: ["movsx eax, byte [rsi]", "movsx rax, ax"],
    notes: ["Use for signed widening; use movzx for unsigned."],
    related: ["movzx", "movsxd", "mov"],
  },
  movsxd: {
    category: "data movement",
    summary: "Sign-extend a 32-bit value into a 64-bit register.",
    description:
      "The dedicated 32→64 sign extension. Common when a signed int index must be used in a 64-bit address computation.",
    syntax: ["movsxd r64, r/m32"],
    flags: "none",
    flagEffects: {},
    examples: ["movsxd rax, eax", "movsxd rdi, dword [rsi]"],
    related: ["movsx", "movzx", "cdqe"],
  },
  lea: {
    category: "address arithmetic",
    summary: "Compute an address and store it — no memory is touched, no flags change.",
    description:
      "Load Effective Address evaluates the [base + index*scale + disp] expression and stores the result, without dereferencing it. Because that expression is a small ALU, lea doubles as a flag-free multiply-add: `lea rax, [rdi + rdi*4]` computes rdi*5.",
    syntax: ["lea r64, [base + index*scale + disp]"],
    flags: "none",
    flagEffects: {},
    examples: ["lea rax, [rbx + 8]", "lea rax, [rdi + rdi*4]", "lea rsi, [rel msg]"],
    encoding: "Uses the ModRM/SIB addressing bytes of a memory operand, but computes rather than accesses.",
    notes: [
      "scale is limited to 1, 2, 4, or 8.",
      "A favourite for arithmetic precisely because it leaves the flags alone.",
    ],
    related: ["mov", "add", "imul"],
  },
  push: {
    category: "stack",
    summary: "Decrement rsp by the operand size, then store the operand at [rsp].",
    description:
      "Grow the stack downward and place a value at the new top. The building block of call frames and saving registers across calls.",
    syntax: ["push r/m64", "push imm32"],
    flags: "none",
    flagEffects: {},
    examples: ["push rbp", "push rax", "push 0"],
    encoding: "opcode 50+rd for a register; the operand is always 64-bit in long mode.",
    notes: ["rsp moves by 8 in 64-bit mode regardless of the operand's apparent width."],
    related: ["pop", "call", "leave", "sub"],
  },
  pop: {
    category: "stack",
    summary: "Load from [rsp], then increment rsp.",
    description:
      "Read the value at the top of the stack into the destination and shrink the stack. The inverse of push.",
    syntax: ["pop r/m64"],
    flags: "none",
    flagEffects: {},
    examples: ["pop rbp", "pop rax"],
    encoding: "opcode 58+rd for a register.",
    related: ["push", "ret", "leave"],
  },
  xchg: {
    category: "data movement",
    summary: "Swap the contents of the two operands.",
    description:
      "Exchange two operands in one instruction. With a memory operand it is implicitly atomic (a full lock), which is why it underpins simple spinlocks.",
    syntax: ["xchg r/m64, r64"],
    flags: "none",
    flagEffects: {},
    examples: ["xchg rax, rbx", "xchg [rdi], rax"],
    notes: ["A memory xchg carries an implied LOCK — useful for atomics, costly if unintended."],
    related: ["mov", "lock", "cmpxchg"],
  },
  add: {
    category: "arithmetic",
    summary: "Add the source into the destination.",
    description:
      "Integer addition, destination += source. Sets the full flag set: CF for unsigned overflow, OF for signed overflow, plus the sign/zero/parity of the result.",
    syntax: ["add r/m64, r64", "add r64, r/m64", "add r/m64, imm32"],
    flags: "OF SF ZF AF PF CF",
    flagEffects: ARITH,
    examples: ["add eax, 1", "add rax, rbx", "add qword [rsp], 8"],
    notes: ["CF is the unsigned carry; OF is the signed overflow. They answer different questions."],
    related: ["adc", "sub", "inc", "lea"],
  },
  sub: {
    category: "arithmetic",
    summary: "Subtract the source from the destination.",
    description:
      "Integer subtraction, destination -= source. Sets flags exactly like cmp — cmp is a sub that throws the result away.",
    syntax: ["sub r/m64, r64", "sub r64, r/m64", "sub r/m64, imm32"],
    flags: "OF SF ZF AF PF CF",
    flagEffects: ARITH,
    examples: ["sub eax, 1", "sub rsp, 16", "sub rax, rbx"],
    notes: ["CF is set when the subtraction borrows (unsigned a < b)."],
    related: ["sbb", "add", "dec", "cmp", "neg"],
  },
  adc: {
    category: "arithmetic",
    summary: "Add source plus the carry flag into the destination.",
    description:
      "Add-with-carry: destination += source + CF. Chained across words it performs wide addition — the low add sets CF, each higher adc folds it in.",
    syntax: ["adc r/m64, r64", "adc r/m64, imm32"],
    flags: "OF SF ZF AF PF CF",
    flagEffects: ARITH,
    examples: ["add rax, rcx", "adc rdx, rbx"],
    notes: ["Reads CF as an input, unlike add. Pair a leading add with trailing adcs for multi-word math."],
    related: ["add", "sbb", "inc"],
  },
  sbb: {
    category: "arithmetic",
    summary: "Subtract source and the carry flag from the destination.",
    description:
      "Subtract-with-borrow: destination -= source + CF. The subtraction counterpart of adc, used to chain wide subtractions.",
    syntax: ["sbb r/m64, r64", "sbb r/m64, imm32"],
    flags: "OF SF ZF AF PF CF",
    flagEffects: ARITH,
    examples: ["sub rax, rcx", "sbb rdx, rbx"],
    notes: ["`sbb r, r` yields 0 or -1 depending on CF — an old trick to materialise a carry mask."],
    related: ["sub", "adc", "dec"],
  },
  inc: {
    category: "arithmetic",
    summary: "Add one — deliberately without touching the carry flag.",
    description:
      "Increment by one. Identical to `add x, 1` except it leaves CF untouched, so it can advance a counter inside a loop that uses CF for something else.",
    syntax: ["inc r/m64"],
    flags: "OF SF ZF AF PF (not CF)",
    flagEffects: ARITH_NO_CF,
    examples: ["inc rax", "inc dword [counter]"],
    notes: ["Not touching CF is the whole point — it is what distinguishes inc from add …, 1."],
    related: ["dec", "add"],
  },
  dec: {
    category: "arithmetic",
    summary: "Subtract one — deliberately without touching the carry flag.",
    description:
      "Decrement by one, leaving CF alone. Frequently the loop counter update before a `jnz`.",
    syntax: ["dec r/m64"],
    flags: "OF SF ZF AF PF (not CF)",
    flagEffects: ARITH_NO_CF,
    examples: ["dec rcx", "dec dword [n]"],
    related: ["inc", "sub"],
  },
  neg: {
    category: "arithmetic",
    summary: "Replace the operand with its two's-complement negation.",
    description:
      "Compute 0 - operand. Sets CF unless the operand was zero (i.e. CF = operand != 0), which is how a negation doubles as a nonzero test.",
    syntax: ["neg r/m64"],
    flags: "OF SF ZF AF PF CF",
    flagEffects: ARITH,
    examples: ["neg rax"],
    notes: ["CF = (operand != 0). Negating the most-negative value overflows and sets OF."],
    related: ["sub", "not"],
  },
  mul: {
    category: "arithmetic",
    summary: "Unsigned multiply of the accumulator by the operand; result in rdx:rax.",
    description:
      "Unsigned multiply: rax (or a narrower accumulator) times the operand, producing a double-width product with the high half in rdx. CF and OF are set when the high half is nonzero — i.e. the product overflowed the low half.",
    syntax: ["mul r/m64"],
    flags: "OF CF (SF ZF AF PF undefined)",
    flagEffects: MULFLAGS,
    examples: ["mov rax, 6", "mul rbx"],
    notes: [
      "One explicit operand; the other factor is always rax, and rdx is clobbered with the high half.",
      "Use imul for signed multiply.",
    ],
    related: ["imul", "div", "add"],
  },
  imul: {
    category: "arithmetic",
    summary: "Signed multiply.",
    description:
      "Signed multiply. The one-operand form mirrors mul (rdx:rax); the two- and three-operand forms give a truncated same-width result and are the usual way to multiply, since they name their own destination.",
    syntax: ["imul r/m64", "imul r64, r/m64", "imul r64, r/m64, imm32"],
    flags: "OF CF (others undefined)",
    flagEffects: MULFLAGS,
    examples: ["imul rax, rbx", "imul rax, rbx, 10", "imul rcx"],
    notes: ["CF/OF flag when the true product doesn't fit the destination width."],
    related: ["mul", "idiv", "lea"],
  },
  div: {
    category: "arithmetic",
    summary: "Unsigned divide rdx:rax by the operand; quotient to rax, remainder to rdx.",
    description:
      "Unsigned division of the double-width value in rdx:rax by the operand. Quotient lands in rax, remainder in rdx. You must set up rdx first — zero it for a 64-bit dividend.",
    syntax: ["div r/m64"],
    flags: "all undefined",
    flagEffects: ALL_UNDEF,
    examples: ["xor edx, edx", "mov rax, 100", "div rbx"],
    notes: [
      "Divide-by-zero, or a quotient too large for the destination, raises #DE — a fault, not a flag.",
      "Always initialise rdx (xor edx,edx for unsigned) before dividing.",
    ],
    related: ["idiv", "mul", "cqo"],
  },
  idiv: {
    category: "arithmetic",
    summary: "Signed divide rdx:rax by the operand.",
    description:
      "Signed division of rdx:rax by the operand. Prepare the high half with a sign extension (cqo/cdq) rather than by zeroing, so the dividend's sign is correct.",
    syntax: ["idiv r/m64"],
    flags: "all undefined",
    flagEffects: ALL_UNDEF,
    examples: ["cqo", "idiv rbx"],
    notes: ["Sign-extend into rdx with cqo (64-bit) or cdq (32-bit) first, not xor."],
    related: ["div", "imul", "cqo", "cdq"],
  },
  and: {
    category: "logic",
    summary: "Bitwise AND the source into the destination.",
    description:
      "Bitwise AND — the mask-out operator: keep the bits set in both operands. `and x, 0xF` isolates the low nibble. Clears CF and OF, sets the data flags.",
    syntax: ["and r/m64, r64", "and r/m64, imm32"],
    flags: "SF ZF PF (clears OF, CF)",
    flagEffects: LOGIC,
    examples: ["and eax, 0xff", "and rax, rbx"],
    related: ["or", "xor", "not", "test"],
  },
  or: {
    category: "logic",
    summary: "Bitwise OR the source into the destination.",
    description:
      "Bitwise OR — the set-bits operator: keep any bit set in either operand. `or x, 0x80` forces a flag bit on.",
    syntax: ["or r/m64, r64", "or r/m64, imm32"],
    flags: "SF ZF PF (clears OF, CF)",
    flagEffects: LOGIC,
    examples: ["or eax, 0x80", "or rax, rbx"],
    related: ["and", "xor", "not"],
  },
  xor: {
    category: "logic",
    summary: "Bitwise exclusive-OR. `xor r, r` is the idiomatic zero.",
    description:
      "Bitwise XOR — flip the destination bits wherever the source is 1. `xor eax, eax` is the shortest, fastest way to zero a register (and the CPU recognises it as a zeroing idiom).",
    syntax: ["xor r/m64, r64", "xor r/m64, imm32"],
    flags: "SF ZF PF (clears OF, CF)",
    flagEffects: LOGIC,
    examples: ["xor eax, eax", "xor rax, rbx", "xor byte [rdi], 0x20"],
    notes: ["`xor eax, eax` zeroes all of rax (32-bit write zero-extends) in 2 bytes."],
    related: ["and", "or", "not", "test"],
  },
  not: {
    category: "logic",
    summary: "Flip every bit of the operand.",
    description:
      "One's-complement: invert every bit. Unlike the other logic ops it changes no flags. `not x` then `inc x` is `neg x`.",
    syntax: ["not r/m64"],
    flags: "none",
    flagEffects: {},
    examples: ["not rax"],
    related: ["neg", "and", "or", "xor"],
  },
  cmp: {
    category: "compare",
    summary: "Subtract, set the flags, and discard the result — a comparison.",
    description:
      "Compare by subtracting source from destination and setting flags exactly as sub would, but throwing the difference away. The setup for a conditional branch: `cmp a, b` then `jl`, `je`, `ja`, …",
    syntax: ["cmp r/m64, r64", "cmp r/m64, imm32"],
    flags: "OF SF ZF AF PF CF",
    flagEffects: ARITH,
    examples: ["cmp eax, 10", "cmp rax, rbx"],
    notes: [
      "For unsigned comparisons read CF/ZF (jb/ja/je); for signed read SF/OF/ZF (jl/jg/je).",
      "Order matters: `cmp a, b` computes a - b, so `jg` means a > b.",
    ],
    related: ["test", "sub", "je", "jl", "jg"],
  },
  test: {
    category: "compare",
    summary: "Bitwise AND, set the flags, and discard the result. `test r, r` tests for zero.",
    description:
      "AND the operands for their flags only, without storing the result. `test rax, rax` sets ZF iff rax is zero and SF to its sign — the standard nonzero/sign check. `test x, mask` checks whether any masked bit is set.",
    syntax: ["test r/m64, r64", "test r/m64, imm32"],
    flags: "SF ZF PF (clears OF, CF)",
    flagEffects: LOGIC,
    examples: ["test rax, rax", "test al, 1", "test eax, 0x8000"],
    notes: ["`test reg, reg` is the idiomatic zero/sign check — cheaper than `cmp reg, 0`."],
    related: ["cmp", "and", "jz", "jnz"],
  },
  shl: {
    category: "shift",
    summary: "Shift left, filling with zeroes. CF holds the last bit shifted out.",
    description:
      "Logical shift left by a count: each bit moves toward the high end, zeroes enter at the bottom. A shift left by n multiplies by 2ⁿ. The last bit to leave the top lands in CF.",
    syntax: ["shl r/m64, imm8", "shl r/m64, cl"],
    flags: "CF OF SF ZF PF",
    flagEffects: SHIFT,
    examples: ["shl rax, 1", "shl rax, cl"],
    notes: ["The count is taken modulo the operand width (& 63 for 64-bit)."],
    related: ["shr", "sar", "sal", "rol"],
  },
  sal: {
    category: "shift",
    summary: "Arithmetic shift left — identical to shl.",
    description:
      "Arithmetic shift left. There is no distinction from shl (shifting in zeroes at the bottom is the same operation whether signed or not), so they share an encoding.",
    syntax: ["sal r/m64, imm8", "sal r/m64, cl"],
    flags: "CF OF SF ZF PF",
    flagEffects: SHIFT,
    examples: ["sal rax, 3"],
    notes: ["A pure alias of shl — same opcode."],
    related: ["shl", "sar", "shr"],
  },
  shr: {
    category: "shift",
    summary: "Shift right, filling with zeroes (unsigned divide by a power of two).",
    description:
      "Logical shift right: bits move toward the low end, zeroes enter at the top. Equivalent to an unsigned divide by 2ⁿ. The last bit shifted out of the bottom lands in CF.",
    syntax: ["shr r/m64, imm8", "shr r/m64, cl"],
    flags: "CF OF SF ZF PF",
    flagEffects: SHIFT,
    examples: ["shr rax, 1", "shr eax, cl"],
    notes: ["Use shr for unsigned division; use sar to preserve a value's sign."],
    related: ["sar", "shl", "ror"],
  },
  sar: {
    category: "shift",
    summary: "Shift right, replicating the sign bit (signed divide by a power of two).",
    description:
      "Arithmetic shift right: bits move down, but the top is refilled with copies of the sign bit so a negative value stays negative. Divides a signed value by 2ⁿ, rounding toward negative infinity.",
    syntax: ["sar r/m64, imm8", "sar r/m64, cl"],
    flags: "CF OF SF ZF PF",
    flagEffects: SHIFT,
    examples: ["sar rax, 1", "sar eax, cl"],
    notes: ["sar rounds toward −∞, so `sar -1, 1` is still -1, not 0 — different from integer division truncation."],
    related: ["shr", "shl", "idiv"],
  },
  rol: {
    category: "shift",
    summary: "Rotate left.",
    description:
      "Rotate the bits left: bits that fall off the top re-enter at the bottom. No bit is lost. CF receives a copy of the bit that wrapped around.",
    syntax: ["rol r/m64, imm8", "rol r/m64, cl"],
    flags: "CF OF",
    flagEffects: ROTATE,
    examples: ["rol rax, 8", "rol al, 1"],
    related: ["ror", "rcl", "shl", "bswap"],
  },
  ror: {
    category: "shift",
    summary: "Rotate right.",
    description:
      "Rotate the bits right: bits shifted off the bottom re-enter at the top. CF receives the bit that wrapped.",
    syntax: ["ror r/m64, imm8", "ror r/m64, cl"],
    flags: "CF OF",
    flagEffects: ROTATE,
    examples: ["ror rax, 8", "ror al, 1"],
    related: ["rol", "rcr", "shr"],
  },
  rcl: {
    category: "shift",
    summary: "Rotate left through the carry flag.",
    description:
      "Rotate left treating CF as an extra high bit: the operand and CF form a chain one bit wider than the register, rotated together. Used to shift multi-word values a bit at a time.",
    syntax: ["rcl r/m64, imm8", "rcl r/m64, cl"],
    flags: "CF OF",
    flagEffects: ROTATE,
    examples: ["rcl rax, 1"],
    notes: ["CF participates as a 65th bit — the difference from rol."],
    related: ["rcr", "rol", "adc"],
  },
  rcr: {
    category: "shift",
    summary: "Rotate right through the carry flag.",
    description:
      "Rotate right with CF as an extra low bit. The mirror of rcl, for shifting wide values rightward one bit at a time.",
    syntax: ["rcr r/m64, imm8", "rcr r/m64, cl"],
    flags: "CF OF",
    flagEffects: ROTATE,
    examples: ["rcr rax, 1"],
    related: ["rcl", "ror", "sbb"],
  },
  jmp: {
    category: "control flow",
    summary: "Unconditional jump.",
    description:
      "Transfer control to the target unconditionally. A direct jmp uses a signed displacement from the next instruction; an indirect jmp (through a register or memory) is how switch tables and function pointers dispatch.",
    syntax: ["jmp rel32", "jmp r/m64"],
    flags: "none",
    flagEffects: {},
    examples: ["jmp .loop", "jmp rax"],
    encoding: "Direct: EB (rel8) or E9 (rel32). Indirect: FF /4. The assembler picks the shortest that reaches.",
    related: ["call", "je", "jne", "ret"],
  },
  call: {
    category: "control flow",
    summary: "Push the address of the next instruction, then jump.",
    description:
      "Invoke a subroutine: push the return address (the instruction after the call) onto the stack, then jump to the target. A matching ret pops that address to return.",
    syntax: ["call rel32", "call r/m64"],
    flags: "none",
    flagEffects: {},
    examples: ["call printf", "call rax"],
    encoding: "Direct E8 (rel32); indirect FF /2. The pushed return address is 8 bytes.",
    notes: ["Pairs with ret. The pushed return address means the callee sees rsp 8 lower than the caller did."],
    related: ["ret", "jmp", "push", "leave"],
  },
  ret: {
    category: "control flow",
    summary: "Pop a return address off the stack and jump to it.",
    description:
      "Return from a subroutine: pop the top of the stack into the instruction pointer and continue there. The stack must hold the address a matching call pushed.",
    syntax: ["ret", "ret imm16"],
    flags: "none",
    flagEffects: {},
    examples: ["ret"],
    notes: ["A stack imbalance before ret returns to the wrong address — a classic bug and exploit primitive."],
    related: ["call", "leave", "pop"],
  },
  leave: {
    category: "control flow",
    summary: "Restore rsp from rbp, then pop rbp — undo a standard prologue.",
    description:
      "Tear down a frame-pointer-based stack frame in one instruction: `mov rsp, rbp` then `pop rbp`. The usual epilogue partner of a `push rbp; mov rbp, rsp` prologue.",
    syntax: ["leave"],
    flags: "none",
    flagEffects: {},
    examples: ["leave", "ret"],
    related: ["ret", "pop", "mov"],
  },
  syscall: {
    category: "system",
    summary: "Trap into the kernel; the call number is in rax.",
    description:
      "Enter the kernel to request a service. On Linux x86-64 the call number goes in rax and arguments in rdi, rsi, rdx, r10, r8, r9; the result comes back in rax. This platform supports a small subset (write to fd 1/2, exit).",
    syntax: ["syscall"],
    flags: "kernel-defined",
    flagEffects: {},
    examples: ["mov rax, 60", "mov rdi, 0", "syscall"],
    notes: [
      "On any OS, syscall saves the return address into rcx and the flags into r11 before entering the kernel — so both are clobbered.",
      "Windows x64 uses the same instruction but a different convention: the service number is in eax, and the first argument is in r10, then rdx, r8, r9, and the stack. The r10 quirk exists because syscall overwrites rcx (the normal first-argument register) with the return address, so ntdll's stubs run `mov r10, rcx` right before the syscall.",
      "Windows syscall numbers are deliberately unstable — they change between OS builds and are undocumented — so user code is expected to call through ntdll (the Nt*/Zw* stubs) and the Win32 API, never to invoke syscall directly. Linux, by contrast, guarantees its call numbers as a stable ABI, which is why invoking syscall by hand is normal there.",
      "This emulator models the Linux convention only.",
    ],
    related: ["int", "int3", "ret"],
  },
  int3: {
    category: "system",
    summary: "One-byte breakpoint trap — how a debugger patches any instruction.",
    description:
      "Raise a breakpoint exception. Its single-byte encoding (0xCC) is why debuggers implement breakpoints by overwriting the first byte of an instruction with int3 and restoring it when hit.",
    syntax: ["int3"],
    flags: "none",
    flagEffects: {},
    examples: ["int3"],
    encoding: "The dedicated one-byte 0xCC — distinct from the two-byte `int 3`.",
    related: ["int", "nop", "ud2"],
  },
  int: {
    category: "system",
    summary: "Raise the software interrupt named by the operand.",
    description:
      "Trigger the software interrupt with the given vector. `int 0x80` was the legacy 32-bit Linux syscall gate; in 64-bit code syscall is used instead.",
    syntax: ["int imm8"],
    flags: "none",
    flagEffects: {},
    examples: ["int 0x80"],
    related: ["int3", "syscall"],
  },
  hlt: {
    category: "system",
    summary: "Halt the processor until an interrupt arrives.",
    description:
      "Stop executing until the next interrupt. A privileged instruction — user code cannot run it — used by idle loops in kernels.",
    syntax: ["hlt"],
    flags: "none",
    flagEffects: {},
    examples: ["hlt"],
    related: ["nop", "ud2"],
  },
  ud2: {
    category: "system",
    summary: "Raise an invalid-opcode fault; execution never continues past here.",
    description:
      "Deliberately raise #UD (invalid opcode). Compilers emit it to mark unreachable code, so a bug that falls through faults immediately rather than running into garbage.",
    syntax: ["ud2"],
    flags: "none",
    flagEffects: {},
    examples: ["ud2"],
    related: ["int3", "hlt", "nop"],
  },
  nop: {
    category: "system",
    summary: "Do nothing.",
    description:
      "No operation — advance past one (or, in its multi-byte forms, several) bytes without any effect. Used to pad code for alignment and to patch out instructions.",
    syntax: ["nop", "nop r/m"],
    flags: "none",
    flagEffects: {},
    examples: ["nop"],
    encoding: "0x90 is a one-byte nop (actually `xchg eax, eax`); wider multi-byte nops exist for alignment.",
    related: ["endbr64", "int3"],
  },
  endbr64: {
    category: "system",
    summary: "A valid indirect-branch landing pad, required by CET.",
    description:
      "Mark a legal target for an indirect call or jump under Control-flow Enforcement Technology. With CET on, an indirect branch to any byte that is not an endbr64 faults — a defence against jump-oriented attacks.",
    syntax: ["endbr64"],
    flags: "none",
    flagEffects: {},
    examples: ["endbr64"],
    notes: ["On CPUs without CET it decodes as a NOP, so it is safe to emit unconditionally."],
    related: ["nop", "call", "jmp"],
  },
  bswap: {
    category: "data movement",
    summary: "Reverse the byte order of the operand — convert endianness.",
    description:
      "Reverse the bytes of a 32- or 64-bit register: byte 0 swaps with the top byte, and so on. The standard way to convert between little- and big-endian (host/network byte order).",
    syntax: ["bswap r32", "bswap r64"],
    flags: "none",
    flagEffects: {},
    examples: ["bswap eax", "bswap rax"],
    notes: ["Defined only for 32- and 64-bit operands; for 16-bit use `xchg al, ah` or ror by 8."],
    related: ["rol", "ror", "mov", "movbe"],
  },
  movbe: {
    category: "data movement",
    summary: "Move to or from memory, reversing the byte order on the way.",
    description:
      "A load or store that swaps endianness as part of the access: read big-endian bytes from memory into a register in host order, or write a register out big-endian. Does the work of a mov plus a bswap in one instruction — the natural way to touch network-order data in memory.",
    syntax: ["movbe r16/32/64, m16/32/64", "movbe m16/32/64, r16/32/64"],
    flags: "none",
    flagEffects: {},
    examples: ["movbe eax, [rdi]", "movbe [rdi], eax"],
    encoding: "0F 38 F0 /r loads (reg ← mem); 0F 38 F1 /r stores (mem ← reg). One operand must be memory.",
    notes: [
      "One operand must be memory — there is no register-to-register form; use bswap for that.",
      "Requires the MOVBE CPU feature (Atom onwards, and Haswell onwards on the big cores).",
      "This platform's assembler and emulator do not implement movbe; the examples are for reading, not running.",
    ],
    related: ["bswap", "mov", "rol"],
  },
  cdq: {
    category: "arithmetic",
    summary: "Sign-extend eax into edx:eax, preparing for a signed divide.",
    description:
      "Fill edx with copies of eax's sign bit, producing the 64-bit signed dividend edx:eax that a 32-bit idiv consumes. The signed counterpart of zeroing edx.",
    syntax: ["cdq"],
    flags: "none",
    flagEffects: {},
    examples: ["cdq", "idiv ecx"],
    related: ["cqo", "cdqe", "idiv"],
  },
  cqo: {
    category: "arithmetic",
    summary: "Sign-extend rax into rdx:rax.",
    description:
      "Fill rdx with copies of rax's sign bit, forming the 128-bit signed dividend rdx:rax for a 64-bit idiv. Always precede a signed 64-bit divide with cqo, never a zeroing of rdx.",
    syntax: ["cqo"],
    flags: "none",
    flagEffects: {},
    examples: ["cqo", "idiv rcx"],
    related: ["cdq", "cdqe", "idiv"],
  },
  cdqe: {
    category: "arithmetic",
    summary: "Sign-extend eax into rax.",
    description:
      "Sign-extend the 32-bit eax to fill the full 64-bit rax. The register-internal counterpart of movsxd rax, eax.",
    syntax: ["cdqe"],
    flags: "none",
    flagEffects: {},
    examples: ["cdqe"],
    related: ["movsxd", "cdq", "cqo"],
  },
};

// Condition-carrying families -------------------------------------------------
// jcc / setcc / cmovcc share a set of condition suffixes. Rather than list all
// of them, we generate an entry on demand and enumerate a canonical set for the
// browse view.

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

// A canonical, alias-deduplicated condition list for enumeration.
const CANON_CONDS = [
  "o", "no", "b", "ae", "e", "ne", "be", "a",
  "s", "ns", "p", "np", "l", "ge", "le", "g",
] as const;

// Which flags a condition reads — for the flag panel on jcc/setcc/cmovcc.
const COND_FLAGS: Record<string, Flag[]> = {
  o: ["OF"], no: ["OF"], b: ["CF"], c: ["CF"], ae: ["CF"], nb: ["CF"],
  e: ["ZF"], z: ["ZF"], ne: ["ZF"], nz: ["ZF"], be: ["CF", "ZF"], a: ["CF", "ZF"],
  s: ["SF"], ns: ["SF"], p: ["PF"], np: ["PF"],
  l: ["SF", "OF"], ge: ["SF", "OF"], le: ["SF", "OF", "ZF"], g: ["SF", "OF", "ZF"],
};

type CondKind = "j" | "set" | "cmov";

function condTests(cc: string): Partial<Record<Flag, FlagEffect>> {
  const out: Partial<Record<Flag, FlagEffect>> = {};
  for (const f of COND_FLAGS[cc] ?? []) out[f] = "tested";
  return out;
}

function condEntry(kind: CondKind, cc: string): InsnEntry | null {
  const desc = CONDITIONS[cc];
  if (!desc) return null;
  const mnemonic = kind === "cmov" ? `cmov${cc}` : `${kind}${cc}`;
  const jcc = `j${cc}`;
  const shared = { flags: "none (reads flags)", flagEffects: condTests(cc), notes: [] as string[] };

  if (kind === "j") {
    return {
      mnemonic,
      category: "control flow",
      summary: `Jump if the condition holds: ${desc}.`,
      description:
        `Conditional branch: take the jump when the flags say ${desc}. Almost always follows a cmp or test that set those flags. Falls through to the next instruction when the condition is false.`,
      syntax: [`${mnemonic} rel32`],
      ...shared,
      examples: [`cmp rax, rbx`, `${mnemonic} .target`],
      encoding: "Short form 7x (rel8) or near form 0F 8x (rel32); the assembler chooses.",
      notes: [
        "Signed vs unsigned matters: use jl/jg after signed comparisons, jb/ja after unsigned ones.",
      ],
      related: ["cmp", "test", "jmp", `set${cc}`, `cmov${cc}`],
    };
  }
  if (kind === "set") {
    return {
      mnemonic,
      category: "conditional",
      summary: `Set the byte operand to 1 (else 0) if the condition holds: ${desc}.`,
      description:
        `Materialise a condition as a value: write 1 to the byte destination when the flags say ${desc}, otherwise 0. Turns a comparison result into a 0/1 integer without a branch.`,
      syntax: [`${mnemonic} r/m8`],
      ...shared,
      examples: [`cmp rax, rbx`, `${mnemonic} al`, `movzx eax, al`],
      notes: ["Writes only a byte; zero-extend (movzx) if you need a full-width 0/1."],
      related: ["cmp", "test", jcc, `cmov${cc}`],
    };
  }
  return {
    mnemonic,
    category: "conditional",
    summary: `Conditionally copy the source into the destination if the condition holds: ${desc}.`,
    description:
      `Branchless conditional move: copy the source into the destination when the flags say ${desc}, otherwise leave the destination unchanged. Replaces a short jump, avoiding a branch mispredict.`,
    syntax: [`${mnemonic} r64, r/m64`],
    ...shared,
    examples: [`cmp rax, rbx`, `${mnemonic} rcx, rdx`],
    notes: ["The memory source is always read, even when the move doesn't happen — mind faulting addresses."],
    related: ["cmp", "test", "mov", jcc],
  };
}

// Public API ------------------------------------------------------------------

/** The rich entry for a mnemonic, resolving jcc/setcc/cmovcc suffixes. */
export function lookupInsnEntry(mnemonic: string): InsnEntry | null {
  const m = mnemonic.toLowerCase();
  const base = TABLE[m];
  if (base) return { mnemonic: m, ...base, notes: base.notes ?? [] };

  const tryCond = (kind: CondKind, prefix: string): InsnEntry | null =>
    m.startsWith(prefix) ? condEntry(kind, m.slice(prefix.length)) : null;

  return (
    tryCond("cmov", "cmov") ??
    tryCond("set", "set") ??
    // `j` last, so `jmp` (in TABLE) is not shadowed.
    (m !== "jmp" ? tryCond("j", "j") : null)
  );
}

/** The narrow projection the hover chips and tokenizer consume. */
export interface InsnInfo {
  mnemonic: string;
  summary: string;
  flags: string;
  category: string;
}

/** Look up a mnemonic, resolving jcc/setcc/cmovcc condition suffixes. */
export function lookupInsn(mnemonic: string): InsnInfo | null {
  const e = lookupInsnEntry(mnemonic);
  if (!e) return null;
  return { mnemonic: e.mnemonic, summary: e.summary, flags: e.flags, category: e.category };
}

/** Is this word one of the mnemonics we can describe? */
export function isKnownMnemonic(word: string): boolean {
  return lookupInsnEntry(word) !== null;
}

/** Every documented entry: the base table plus the canonical condition
 *  families, for the browse-by-category reference view. */
export function allInsnEntries(): InsnEntry[] {
  const entries: InsnEntry[] = Object.keys(TABLE).map((m) => {
    const base = TABLE[m]!;
    return { mnemonic: m, ...base, notes: base.notes ?? [] };
  });
  for (const kind of ["j", "set", "cmov"] as CondKind[]) {
    for (const cc of CANON_CONDS) {
      const e = condEntry(kind, cc);
      if (e) entries.push(e);
    }
  }
  return entries;
}

/** Category display order for the reference page. */
export const INSN_CATEGORIES = [
  "data movement",
  "address arithmetic",
  "arithmetic",
  "logic",
  "shift",
  "compare",
  "control flow",
  "conditional",
  "stack",
  "system",
] as const;

// Concept/alias search hints: a typed word that is not itself a mnemonic but
// should surface a group of them.
const CONCEPTS: Record<string, string[]> = {
  jump: ["jmp", "je", "jne", "jg", "jl", "jge", "jle", "ja", "jb"],
  branch: ["jmp", "je", "jne", "call", "ret"],
  multiply: ["mul", "imul"],
  divide: ["div", "idiv"],
  division: ["div", "idiv"],
  stack: ["push", "pop", "call", "ret", "leave"],
  compare: ["cmp", "test"],
  shift: ["shl", "shr", "sar", "sal"],
  rotate: ["rol", "ror", "rcl", "rcr"],
  copy: ["mov", "movzx", "movsx", "xchg"],
  move: ["mov", "movzx", "movsx", "cmove"],
  negate: ["neg", "not"],
  zero: ["xor", "and"],
  endian: ["bswap", "movbe"],
  extend: ["movzx", "movsx", "movsxd", "cdqe", "cdq", "cqo"],
  syscall: ["syscall", "int"],
  return: ["ret", "leave"],
  call: ["call", "ret"],
  address: ["lea"],
  logical: ["and", "or", "xor", "not"],
  bitwise: ["and", "or", "xor", "not", "test"],
  add: ["add", "adc", "inc"],
  subtract: ["sub", "sbb", "dec"],
  carry: ["adc", "sbb", "rcl", "rcr"],
  conditional: ["cmove", "setne", "jne"],
};

/**
 * Full-text-ish search over the catalog. Matches mnemonics (prefix and
 * substring), concept/alias words, and the summary/description text, ranking
 * exact and prefix mnemonic hits first. Returns mnemonics.
 */
export function searchInsns(query: string, limit = 40): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const all = allInsnEntries();
  const scored: Array<{ m: string; score: number }> = [];
  const conceptHits = new Set(CONCEPTS[q] ?? []);

  for (const e of all) {
    const m = e.mnemonic;
    let score = 0;
    if (m === q) score = 100;
    else if (m.startsWith(q)) score = 80 - m.length;
    else if (m.includes(q)) score = 50;
    if (conceptHits.has(m)) score = Math.max(score, 70);
    if (score === 0) {
      const hay = `${e.summary} ${e.description} ${e.category} ${e.notes.join(" ")}`.toLowerCase();
      if (hay.includes(q)) score = 20;
    }
    if (score > 0) scored.push({ m, score });
  }
  scored.sort((a, b) => b.score - a.score || a.m.localeCompare(b.m));
  return scored.slice(0, limit).map((s) => s.m);
}
