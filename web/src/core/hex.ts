// Pure hex / ASCII formatting helpers. No DOM, no dependencies — unit-tested
// directly under node:test.

const HEX = "0123456789abcdef";

/** Format a single byte as two lowercase hex digits. */
export function byteHex(b: number): string {
  return HEX[(b >> 4) & 0xf]! + HEX[b & 0xf]!;
}

/** Format an unsigned number/bigint as zero-padded hex of `digits` width. */
export function padHex(value: number | bigint, digits: number): string {
  return value.toString(16).padStart(digits, "0");
}

/** Format an address as `0x`-less, zero-padded hex (default 16 nibbles = u64). */
export function formatAddress(addr: bigint, digits = 16): string {
  return padHex(addr, digits);
}

/** True when a byte is a printable ASCII glyph (0x20..0x7e). */
export function isPrintable(b: number): boolean {
  return b >= 0x20 && b <= 0x7e;
}

/** The ASCII rendering of a byte, using `.` for non-printable values. */
export function asciiChar(b: number): string {
  return isPrintable(b) ? String.fromCharCode(b) : ".";
}

/**
 * Render one row of hex, space-separated, with a wider gap after the halfway
 * point (the classic `hexdump` gutter). Missing trailing bytes render as spaces
 * so short final rows still line up.
 */
export function hexRow(
  bytes: Uint8Array,
  start: number,
  count: number,
  perRow: number,
): string {
  const half = perRow >> 1;
  let out = "";
  for (let i = 0; i < perRow; i++) {
    if (i === half && half > 0) out += " ";
    if (i < count) out += byteHex(bytes[start + i]!);
    else out += "  ";
    if (i !== perRow - 1) out += " ";
  }
  return out;
}

/** Render one row of ASCII, `.` for non-printable, spaces past the end. */
export function asciiRow(
  bytes: Uint8Array,
  start: number,
  count: number,
  perRow: number,
): string {
  let out = "";
  for (let i = 0; i < perRow; i++) {
    out += i < count ? asciiChar(bytes[start + i]!) : " ";
  }
  return out;
}

/** Parse a hex string (whitespace and a leading 0x tolerated) into bytes. */
export function parseHex(input: string): Uint8Array | null {
  const cleaned = input.replace(/0x/gi, "").replace(/[\s,]/g, "");
  if (cleaned.length === 0 || cleaned.length % 2 !== 0) return null;
  if (!/^[0-9a-fA-F]+$/.test(cleaned)) return null;
  const out = new Uint8Array(cleaned.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** Encode bytes as a contiguous lowercase hex string (the wire format). */
export function toHexString(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i++) out += byteHex(bytes[i]!);
  return out;
}
