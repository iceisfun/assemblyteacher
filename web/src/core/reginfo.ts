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
  return BY_NAME.has(word.toLowerCase());
}

/** The byte range a name covers within the 64-bit register, for display. */
export function regByteRange(info: RegInfo): string {
  if (info.high) return "bits 8–15";
  return `bits 0–${info.width - 1}`;
}
