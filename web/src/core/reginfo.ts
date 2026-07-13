// Register helper: given any register name (rax, eax, ax, al, ah, r8d, spl …),
// recover its family, its width, the bytes it covers, and its role. Powers the
// hover card, and reinforces the Registers lesson — the four-width ladder and
// the zero-extension rule made touchable.
//
// Pure and dependency-free.

export type RegWidth = 8 | 16 | 32 | 64;

export interface RegInfo {
  /** The name as written. */
  name: string;
  /** Architectural register number, 0..15. */
  num: number;
  width: RegWidth;
  /** True for the legacy high-byte names ah/ch/dh/bh. */
  high: boolean;
  /** The four-width family, e.g. ["rax","eax","ax","al"]. */
  family: [string, string, string, string];
  /** ah/ch/dh/bh, when this family has one. */
  highByte: string | null;
  /** What the register conventionally holds / how it is used. */
  role: string;
  /** Calling-convention preservation note (System V AMD64). */
  saved: "caller-saved" | "callee-saved";
}

// num -> [r64, r32, r16, r8low], and the legacy high-byte name for 0..3.
const FAMILIES: [string, string, string, string][] = [
  ["rax", "eax", "ax", "al"],
  ["rcx", "ecx", "cx", "cl"],
  ["rdx", "edx", "dx", "dl"],
  ["rbx", "ebx", "bx", "bl"],
  ["rsp", "esp", "sp", "spl"],
  ["rbp", "ebp", "bp", "bpl"],
  ["rsi", "esi", "si", "sil"],
  ["rdi", "edi", "di", "dil"],
  ["r8", "r8d", "r8w", "r8b"],
  ["r9", "r9d", "r9w", "r9b"],
  ["r10", "r10d", "r10w", "r10b"],
  ["r11", "r11d", "r11w", "r11b"],
  ["r12", "r12d", "r12w", "r12b"],
  ["r13", "r13d", "r13w", "r13b"],
  ["r14", "r14d", "r14w", "r14b"],
  ["r15", "r15d", "r15w", "r15b"],
];

const HIGH_BYTES = ["ah", "ch", "dh", "bh"];

const ROLES: string[] = [
  "accumulator; the function return value; implicit operand for mul/div",
  "counter; the only register a variable shift count (as cl) may use; 4th integer argument",
  "data; the high half of a mul/div result (rdx:rax); 3rd integer argument",
  "base; callee-saved, so a function must preserve it across calls",
  "the stack pointer — modified by push/pop/call/ret; not a general-purpose register",
  "the frame pointer by convention; callee-saved",
  "source index; the 2nd integer argument",
  "destination index; the 1st integer argument",
  "general purpose; the 5th integer argument; caller-saved",
  "general purpose; the 6th integer argument; caller-saved",
  "general purpose; caller-saved",
  "general purpose; caller-saved",
  "general purpose; callee-saved",
  "general purpose; callee-saved",
  "general purpose; callee-saved",
  "general purpose; callee-saved",
];

// callee-saved set: rbx, rsp, rbp, r12–r15.
const CALLEE_SAVED = new Set([3, 4, 5, 12, 13, 14, 15]);

const BY_NAME = new Map<string, { num: number; width: RegWidth; high: boolean }>();
for (let num = 0; num < 16; num++) {
  const [q, d, w, b] = FAMILIES[num]!;
  BY_NAME.set(q, { num, width: 64, high: false });
  BY_NAME.set(d, { num, width: 32, high: false });
  BY_NAME.set(w, { num, width: 16, high: false });
  BY_NAME.set(b, { num, width: 8, high: false });
}
HIGH_BYTES.forEach((name, i) => BY_NAME.set(name, { num: i, width: 8, high: true }));

// Registers outside the 16-strong general-purpose file: no four-width family, no
// ABI save class, not usable as an ordinary operand. They get a simpler card,
// but they should still light up and be searchable. `rip` is the one this course
// leans on constantly (RIP-relative addressing, call/ret).
export interface SpecialReg {
  name: string;
  width: RegWidth;
  role: string;
}
const SPECIAL_REGS: Record<string, SpecialReg> = {
  rip: {
    name: "rip",
    width: 64,
    role: "the instruction pointer — the address of the next instruction to execute. You cannot name it as an ordinary operand; call/ret/jmp change it, and RIP-relative addressing (`[rip+disp]`) reads it.",
  },
};

/** A non-general-purpose register (e.g. rip), or null. */
export function specialReg(name: string): SpecialReg | null {
  return SPECIAL_REGS[name.toLowerCase()] ?? null;
}

/** Look up a general-purpose register by any of its names. */
export function lookupReg(name: string): RegInfo | null {
  const entry = BY_NAME.get(name.toLowerCase());
  if (!entry) return null;
  return {
    name: name.toLowerCase(),
    num: entry.num,
    width: entry.width,
    high: entry.high,
    family: FAMILIES[entry.num]!,
    highByte: entry.num < 4 ? HIGH_BYTES[entry.num]! : null,
    role: ROLES[entry.num]!,
    saved: CALLEE_SAVED.has(entry.num) ? "callee-saved" : "caller-saved",
  };
}

export function isKnownRegister(word: string): boolean {
  const w = word.toLowerCase();
  return BY_NAME.has(w) || w in SPECIAL_REGS;
}

// Every register name, for scanning. Order does not matter — search ranks.
const ALL_REG_NAMES: string[] = [...FAMILIES.flat(), ...HIGH_BYTES, ...Object.keys(SPECIAL_REGS)];

// Role words a reader might type that should surface a register. Value is the
// canonical 64-bit name; the family's other widths still match by their names.
const REG_CONCEPTS: Record<string, string> = {
  accumulator: "rax",
  counter: "rcx",
  "stack pointer": "rsp",
  "frame pointer": "rbp",
  "base pointer": "rbp",
  "source index": "rsi",
  "destination index": "rdi",
  "return value": "rax",
};

/**
 * Rank register names against a query. Mirrors `searchInsns`: exact name first,
 * then prefix, then substring (for queries of two or more characters, so a lone
 * "a" does not match half the file), then a role-word concept map and the role
 * text itself. Returns register names, best first.
 */
export function searchRegs(query: string, limit = 8): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const concept = REG_CONCEPTS[q];
  const scored: Array<{ name: string; score: number }> = [];
  for (const name of ALL_REG_NAMES) {
    let score = 0;
    if (name === q) score = 100;
    else if (name.startsWith(q)) score = 80 - name.length;
    else if (q.length >= 2 && name.includes(q)) score = 45;
    if (concept === name) score = Math.max(score, 70);
    if (score === 0) {
      const info = lookupReg(name);
      if (info && info.role.toLowerCase().includes(q)) score = 20;
    }
    if (score > 0) scored.push({ name, score });
  }
  scored.sort((a, b) => b.score - a.score || a.name.localeCompare(b.name));
  return scored.slice(0, limit).map((s) => s.name);
}

/** The byte range a name covers within the 64-bit register, for display. */
export function regByteRange(info: RegInfo): string {
  const [lo, hi] = bitRangeOf(info.name);
  return `bits ${lo}–${hi}`;
}

// ---- register hierarchy ----------------------------------------------------
//
// Every family is a tree rooted at the 64-bit register:
//
//   R?X ─ E?X ─ ?X ─┬─ ?H   (only for A/B/C/D)
//                   └─ ?L
//
// Families without a high byte (SI/DI/BP/SP and R8–R15) drop the ?H branch.

/** The [low, high] bit range a register name owns within its 64-bit register. */
export function bitRangeOf(name: string): [number, number] {
  const info = BY_NAME.get(name.toLowerCase());
  if (!info) return [0, 63];
  if (info.high) return [8, 15];
  return [0, info.width - 1];
}

/** The immediate parent (next-wider alias), or null for the 64-bit register. */
export function parentOf(name: string): string | null {
  const info = BY_NAME.get(name.toLowerCase());
  if (!info) return null;
  const fam = FAMILIES[info.num]!;
  if (info.high) return fam[2]; // ah -> ax
  switch (info.width) {
    case 64:
      return null;
    case 32:
      return fam[0]; // eax -> rax
    case 16:
      return fam[1]; // ax -> eax
    default:
      return fam[2]; // al -> ax
  }
}

/** Every wider alias, from the immediate parent up to the 64-bit register. */
export function ancestorsOf(name: string): string[] {
  const out: string[] = [];
  let cur = parentOf(name);
  while (cur) {
    out.push(cur);
    cur = parentOf(cur);
  }
  return out;
}

/** The 64-bit register at the root of this name's family. */
export function largestOf(name: string): string {
  const info = BY_NAME.get(name.toLowerCase());
  return info ? FAMILIES[info.num]![0] : name;
}

export interface RegNode {
  name: string;
  width: RegWidth;
  bitLo: number;
  bitHi: number;
  children: RegNode[];
}

/** The full family tree for whichever family `name` belongs to (rooted at r64). */
export function familyTree(name: string): RegNode | null {
  const info = BY_NAME.get(name.toLowerCase());
  if (!info) return null;
  const [q, d, w, b] = FAMILIES[info.num]!;
  const node = (n: string, width: RegWidth, lo: number, hi: number, children: RegNode[]): RegNode => ({
    name: n,
    width,
    bitLo: lo,
    bitHi: hi,
    children,
  });

  const byte8: RegNode[] =
    info.num < 4
      ? // A/B/C/D: high byte then low byte, matching the SOW's downward order.
        [node(HIGH_BYTES[info.num]!, 8, 8, 15, []), node(b, 8, 0, 7, [])]
      : [node(b, 8, 0, 7, [])];

  return node(q, 64, 0, 63, [node(d, 32, 0, 31, [node(w, 16, 0, 15, byte8)])]);
}

/** Immediate child registers (one level down), for the metadata panel. */
export function childrenOf(name: string): string[] {
  const info = BY_NAME.get(name.toLowerCase());
  if (!info || info.high) return [];
  const [q, d, w, b] = FAMILIES[info.num]!;
  switch (info.width) {
    case 64:
      return [d];
    case 32:
      return [w];
    case 16:
      return info.num < 4 ? [HIGH_BYTES[info.num]!, b] : [b];
    default:
      return [];
  }
}
