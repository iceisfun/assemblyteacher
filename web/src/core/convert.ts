// Byte / value converter logic. The recurring pain it solves: a hex dump shows
// `aa bb cc dd`, and to use that value you must mentally reverse it to
// `0xddccbbaa` (little-endian). This turns the bytes you see into the value you
// need, in either byte order, and back.
//
// Pure and dependency-free.

export type Endian = "le" | "be";

/**
 * Parse a run of hex bytes: `aa bb cc dd`, `aabbccdd`, `0xaa 0xbb`, comma- or
 * underscore-separated — anything that reduces to an even number of hex digits.
 * Returns the bytes in the order written (left to right), or null.
 */
export function parseHexBytes(text: string): number[] | null {
  const cleaned = text.replace(/0x/gi, "").replace(/[\s,_]/g, "");
  if (cleaned.length === 0 || cleaned.length % 2 !== 0) return null;
  if (!/^[0-9a-fA-F]+$/.test(cleaned)) return null;
  const bytes: number[] = [];
  for (let i = 0; i < cleaned.length; i += 2) {
    bytes.push(parseInt(cleaned.slice(i, i + 2), 16));
  }
  return bytes;
}

/** Assemble a value from bytes under the given byte order. In little-endian the
 *  first byte is least significant (memory / dump order); in big-endian it is
 *  most significant (reading / network order). */
export function valueOfBytes(bytes: number[], endian: Endian): bigint {
  let v = 0n;
  for (let i = 0; i < bytes.length; i++) {
    const pos = endian === "le" ? i : bytes.length - 1 - i;
    v |= BigInt(bytes[i]!) << BigInt(8 * pos);
  }
  return v;
}

/** Lay a value out as `byteLen` bytes in the given order. */
export function bytesOfValue(value: bigint, byteLen: number, endian: Endian): number[] {
  const le: number[] = [];
  for (let i = 0; i < byteLen; i++) le.push(Number((value >> BigInt(8 * i)) & 0xffn));
  return endian === "le" ? le : le.reverse();
}

/** The two's-complement signed reading of a value at a given bit width. */
export function signedOf(value: bigint, bits: number): bigint {
  if (bits <= 0) return value;
  const sign = 1n << BigInt(bits - 1);
  return value & sign ? value - (1n << BigInt(bits)) : value;
}

/** `aa bb cc dd` */
export function formatBytes(bytes: number[]): string {
  return bytes.map((b) => b.toString(16).padStart(2, "0")).join(" ");
}
