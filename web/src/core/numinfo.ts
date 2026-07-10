// Number helper: parse a numeric literal as it appears in a lesson or a
// disassembly, and decompose it into the readings the first lessons teach —
// binary, decimal, hex, the signed (two's-complement) value, and the bit-by-bit
// place values grouped into nibbles so each hex digit lines up under its four
// bits.
//
// Pure and dependency-free so it can be unit-tested without a browser.

export type Radix = 2 | 8 | 10 | 16 | "char";
export type Width = 8 | 16 | 32 | 64;

export interface NumberInfo {
  /** The literal as written, e.g. "0x2a", "0b1011", "-1", "'A'". */
  raw: string;
  /** How it was written. */
  radix: Radix;
  /** True if the literal carried a leading minus. */
  negative: boolean;
  /** The value as an unsigned bit pattern, masked to `width`. */
  value: bigint;
  /** The natural width chosen to hold it (8/16/32/64). */
  width: Width;
}

const MASK: Record<Width, bigint> = {
  8: 0xffn,
  16: 0xffffn,
  32: 0xffff_ffffn,
  64: 0xffff_ffff_ffff_ffffn,
};

/** Smallest natural width whose *unsigned* range holds `v` (v >= 0). */
function fitWidth(v: bigint): Width {
  if (v <= 0xffn) return 8;
  if (v <= 0xffffn) return 16;
  if (v <= 0xffff_ffffn) return 32;
  return 64;
}

/** Smallest natural width whose *signed* range holds `v` (v may be negative). */
function fitSignedWidth(v: bigint): Width {
  const widths: Width[] = [8, 16, 32, 64];
  for (const w of widths) {
    const hi = (1n << BigInt(w - 1)) - 1n;
    const lo = -(1n << BigInt(w - 1));
    if (v >= lo && v <= hi) return w;
  }
  return 64;
}

/**
 * Parse a numeric literal. Accepts hex (`0x2a`), binary (`0b1011`), octal
 * (`0o52`), NASM trailing-h hex (`2ah`), decimal, and a single-character
 * literal (`'A'`), each optionally with a leading `-`. Returns `null` for
 * anything that is not a recognisable number.
 */
export function parseNumberLiteral(raw: string): NumberInfo | null {
  const s = raw.trim();
  if (s === "") return null;

  // Character literal: 'A' -> its byte value.
  const charMatch = /^'(\\?.)'$/.exec(s);
  if (charMatch) {
    const inner = charMatch[1]!;
    const map: Record<string, number> = {
      "\\n": 10,
      "\\t": 9,
      "\\r": 13,
      "\\0": 0,
      "\\\\": 92,
      "\\'": 39,
    };
    const code = inner.length === 2 ? map[inner] : inner.codePointAt(0);
    if (code === undefined || code > 0xff) return null;
    return { raw: s, radix: "char", negative: false, value: BigInt(code), width: 8 };
  }

  const negative = s.startsWith("-");
  const body = negative ? s.slice(1).trim() : s;

  let magnitude: bigint | null = null;
  let radix: Radix = 10;

  if (/^0x[0-9a-fA-F]+$/.test(body)) {
    magnitude = BigInt(body);
    radix = 16;
  } else if (/^0b[01]+$/.test(body)) {
    magnitude = BigInt(body);
    radix = 2;
  } else if (/^0o[0-7]+$/.test(body)) {
    magnitude = BigInt(body);
    radix = 8;
  } else if (/^[0-9a-fA-F]+[hH]$/.test(body)) {
    magnitude = BigInt("0x" + body.slice(0, -1));
    radix = 16;
  } else if (/^[0-9]+$/.test(body)) {
    magnitude = BigInt(body);
    radix = 10;
  } else {
    return null;
  }

  const signed = negative ? -magnitude : magnitude;
  const width = negative ? fitSignedWidth(signed) : fitWidth(magnitude);
  // Store the two's-complement bit pattern for negatives.
  const value = signed & MASK[width];
  return { raw: s, radix, negative, value, width };
}

/** Re-interpret the stored bit pattern at a chosen width (for a width toggle). */
export function atWidth(info: NumberInfo, width: Width): NumberInfo {
  return { ...info, width, value: info.value & MASK[width] };
}

/** The signed (two's-complement) value of the bit pattern at its width. */
export function signedValue(info: NumberInfo): bigint {
  const signBit = 1n << BigInt(info.width - 1);
  return info.value & signBit ? info.value - (1n << BigInt(info.width)) : info.value;
}

export interface Bit {
  /** Bit position, 0 = least significant. */
  index: number;
  set: boolean;
  /** The place value this bit contributes when set: 2**index. */
  place: bigint;
}

export interface Nibble {
  /** The hex digit this group of four bits spells. */
  hex: string;
  /** The four bits, most-significant first. */
  bits: Bit[];
}

/**
 * Decompose the value into nibbles, most-significant first. Each nibble is four
 * bits and one hex digit — the "one hex digit is exactly four bits" identity,
 * made structural.
 */
export function nibbles(info: NumberInfo): Nibble[] {
  const out: Nibble[] = [];
  const count = info.width / 4;
  for (let n = count - 1; n >= 0; n--) {
    const bits: Bit[] = [];
    for (let b = 3; b >= 0; b--) {
      const index = n * 4 + b;
      const set = (info.value >> BigInt(index)) & 1n ? true : false;
      bits.push({ index, set, place: 1n << BigInt(index) });
    }
    const digit = Number((info.value >> BigInt(n * 4)) & 0xfn);
    out.push({ hex: digit.toString(16), bits });
  }
  return out;
}

/** The three canonical readings plus the signed value and any ASCII glyph. */
export function readings(info: NumberInfo): {
  binary: string;
  decimalUnsigned: string;
  decimalSigned: string;
  hex: string;
  ascii: string | null;
} {
  const bin = info.value.toString(2).padStart(info.width, "0");
  const grouped = bin.replace(/(.{4})(?=.)/g, "$1 ");
  const signed = signedValue(info);
  const printable = info.value >= 0x20n && info.value <= 0x7en;
  return {
    binary: grouped,
    decimalUnsigned: info.value.toString(10),
    decimalSigned: signed.toString(10),
    hex: "0x" + info.value.toString(16),
    ascii: info.width === 8 && printable ? String.fromCharCode(Number(info.value)) : null,
  };
}
