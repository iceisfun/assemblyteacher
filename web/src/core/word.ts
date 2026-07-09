// A "machine word": any 64-bit value (register contents, address, immediate)
// that crosses the wire as a `0x`-prefixed lowercase hex STRING, never a JSON
// number. JSON numbers are IEEE doubles, exact only to 2^53; a register holds
// any of 2^64 bit patterns, so a tool whose whole premise is exact bytes must
// not round-trip them through a double. Internally we work in `bigint`.
//
// Input is tolerant (the server accepts a number, a decimal string, or "0x…");
// output is always canonical "0x…" unsigned to 64 bits.

export type Word = string;

const TWO_64 = 1n << 64n;
const U64_MASK = TWO_64 - 1n;

/** Parse a wire word (or a lenient number/bigint) into a bigint. */
export function parseWord(w: Word | number | bigint): bigint {
  if (typeof w === "bigint") return w;
  if (typeof w === "number") return BigInt(Math.trunc(w));
  const s = w.trim();
  if (s.length === 0) return 0n;
  return BigInt(s); // handles "0x..", decimal, and "-decimal"
}

/** Format a bigint as the canonical unsigned 64-bit `0x…` wire word. */
export function formatWord(v: bigint): Word {
  const u = ((v % TWO_64) + TWO_64) & U64_MASK;
  return "0x" + u.toString(16);
}
