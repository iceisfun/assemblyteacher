// x86-64 assembly lexical data and a pure tokenizer. This module has no browser
// or Monaco dependency so it can be unit-tested and reused to build the Monarch
// language definition (see src/asm/monarch.ts).

// Condition-code suffixes, including the common aliases, mirroring
// asm-core's Cond::parse. Drives jcc / setcc / cmovcc generation.
const COND_SUFFIXES = [
  "o", "no", "b", "c", "nae", "ae", "nb", "nc", "e", "z", "ne", "nz",
  "be", "na", "a", "nbe", "s", "ns", "p", "pe", "np", "po", "l", "nge",
  "ge", "nl", "le", "ng", "g", "nle",
];

// Base mnemonics from asm-core::insn::Mnemonic (the non-condition forms).
const BASE_MNEMONICS = [
  "add", "or", "adc", "sbb", "and", "sub", "xor", "cmp", "test", "not",
  "neg", "inc", "dec", "mul", "imul", "div", "idiv", "mov", "movzx",
  "movsx", "movsxd", "lea", "push", "pop", "xchg", "shl", "shr", "sar",
  "rol", "ror", "rcl", "rcr", "jmp", "call", "ret", "leave", "nop", "hlt",
  "int3", "int", "syscall", "cdq", "cqo", "cwd", "cdqe", "cbw", "cwde",
  "bswap", "endbr64", "ud2",
];

function expandConditionals(): string[] {
  const out: string[] = [];
  for (const c of COND_SUFFIXES) {
    out.push(`j${c}`, `set${c}`, `cmov${c}`);
  }
  return out;
}

/** Every mnemonic the decoder understands, condition forms expanded. */
export const MNEMONICS: string[] = [
  ...BASE_MNEMONICS,
  ...expandConditionals(),
].sort();

// Registers from asm-core::reg (all four widths + legacy high bytes) plus rip
// and the segment registers.
export const REGISTERS: string[] = [
  "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
  "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
  "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi",
  "r8d", "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d",
  "ax", "cx", "dx", "bx", "sp", "bp", "si", "di",
  "r8w", "r9w", "r10w", "r11w", "r12w", "r13w", "r14w", "r15w",
  "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil",
  "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b", "r15b",
  "ah", "ch", "dh", "bh",
  "rip", "es", "cs", "ss", "ds", "fs", "gs",
];

/** Size keywords used to disambiguate memory operand widths. */
export const SIZE_KEYWORDS = ["byte", "word", "dword", "qword", "ptr"];

/** Assembler directives the source view recognises. */
export const DIRECTIVES = [
  "bits", "section", "global", "extern", "db", "dw", "dd", "dq",
  "equ", "org", "align", "default", "times", "rel",
];

const MNEMONIC_SET = new Set(MNEMONICS);
const REGISTER_SET = new Set(REGISTERS);
const SIZE_SET = new Set(SIZE_KEYWORDS);
const DIRECTIVE_SET = new Set(DIRECTIVES);

export type TokenType =
  | "mnemonic"
  | "register"
  | "size"
  | "directive"
  | "number"
  | "label"
  | "labelDef"
  | "string"
  | "comment"
  | "operator"
  | "identifier"
  | "whitespace";

export interface Token {
  text: string;
  type: TokenType;
  start: number;
}

/** Classify a bare word (already lowercased for keyword lookup). */
export function classifyWord(word: string): TokenType {
  const lower = word.toLowerCase();
  if (MNEMONIC_SET.has(lower)) return "mnemonic";
  if (REGISTER_SET.has(lower)) return "register";
  if (SIZE_SET.has(lower)) return "size";
  if (DIRECTIVE_SET.has(lower)) return "directive";
  return "identifier";
}

const NUMBER_RE =
  /^(?:0x[0-9a-fA-F]+|0b[01]+|[0-9][0-9a-fA-F]*h|[0-9]+)\b/;

/**
 * Tokenize a single line of assembly. Deliberately small and dependency-free;
 * it mirrors the Monarch rules closely enough to test the classification of a
 * sample line, and powers offline syntax hints.
 */
export function tokenizeLine(line: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < line.length) {
    const ch = line[i]!;
    // whitespace
    if (/\s/.test(ch)) {
      i++;
      continue;
    }
    // comment to end of line
    if (ch === ";" || ch === "#") {
      tokens.push({ text: line.slice(i), type: "comment", start: i });
      break;
    }
    // string literal
    if (ch === '"' || ch === "'") {
      const quote = ch;
      let j = i + 1;
      while (j < line.length && line[j] !== quote) j++;
      const end = Math.min(j + 1, line.length);
      tokens.push({ text: line.slice(i, end), type: "string", start: i });
      i = end;
      continue;
    }
    // label definition: identifier immediately followed by ':'
    const labelMatch = /^[A-Za-z_.$][\w.$]*:/.exec(line.slice(i));
    if (labelMatch) {
      tokens.push({ text: labelMatch[0], type: "labelDef", start: i });
      i += labelMatch[0].length;
      continue;
    }
    // number
    const numMatch = NUMBER_RE.exec(line.slice(i));
    if (numMatch && /[0-9]/.test(ch)) {
      tokens.push({ text: numMatch[0], type: "number", start: i });
      i += numMatch[0].length;
      continue;
    }
    // word (identifier / keyword)
    const wordMatch = /^[A-Za-z_.$][\w.$]*/.exec(line.slice(i));
    if (wordMatch) {
      const w = wordMatch[0];
      tokens.push({ text: w, type: classifyWord(w), start: i });
      i += w.length;
      continue;
    }
    // operator / punctuation
    if ("+-*,:[]()".includes(ch)) {
      tokens.push({ text: ch, type: "operator", start: i });
      i++;
      continue;
    }
    // anything else: skip one char as identifier fragment
    tokens.push({ text: ch, type: "identifier", start: i });
    i++;
  }
  return tokens;
}
