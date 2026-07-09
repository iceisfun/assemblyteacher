// Endian-aware interpretation of a byte window. Pure; unit-tested.

export type Endianness = "little" | "big";

/** Read `size` (1,2,4,8) bytes as an unsigned bigint. */
export function readUnsigned(
  bytes: Uint8Array,
  offset: number,
  size: number,
  endianness: Endianness,
): bigint {
  let value = 0n;
  if (endianness === "little") {
    for (let i = size - 1; i >= 0; i--) {
      value = (value << 8n) | BigInt(bytes[offset + i] ?? 0);
    }
  } else {
    for (let i = 0; i < size; i++) {
      value = (value << 8n) | BigInt(bytes[offset + i] ?? 0);
    }
  }
  return value;
}

/** Sign-extend an unsigned bigint of `size` bytes to a signed bigint. */
export function toSigned(value: bigint, size: number): bigint {
  const bits = BigInt(size * 8);
  const signBit = 1n << (bits - 1n);
  return value & signBit ? value - (1n << bits) : value;
}

/** Read `size` bytes as a signed bigint. */
export function readSigned(
  bytes: Uint8Array,
  offset: number,
  size: number,
  endianness: Endianness,
): bigint {
  return toSigned(readUnsigned(bytes, offset, size, endianness), size);
}

/** Interpret an 8-byte window as an IEEE-754 double. */
export function readF64(
  bytes: Uint8Array,
  offset: number,
  endianness: Endianness,
): number {
  const buf = new DataView(new ArrayBuffer(8));
  for (let i = 0; i < 8; i++) buf.setUint8(i, bytes[offset + i] ?? 0);
  return buf.getFloat64(0, endianness === "little");
}

/** Interpret a 4-byte window as an IEEE-754 float. */
export function readF32(
  bytes: Uint8Array,
  offset: number,
  endianness: Endianness,
): number {
  const buf = new DataView(new ArrayBuffer(4));
  for (let i = 0; i < 4; i++) buf.setUint8(i, bytes[offset + i] ?? 0);
  return buf.getFloat32(0, endianness === "little");
}

export interface Interpretation {
  u8?: bigint;
  u16?: bigint;
  u32?: bigint;
  u64?: bigint;
  i8?: bigint;
  i16?: bigint;
  i32?: bigint;
  i64?: bigint;
  f32?: number;
  f64?: number;
}

/**
 * Interpret the bytes starting at `offset` as every integer/float width that
 * fits within `available` bytes. Used by the status bar and hover tooltip.
 */
export function interpret(
  bytes: Uint8Array,
  offset: number,
  endianness: Endianness,
): Interpretation {
  const avail = bytes.length - offset;
  const out: Interpretation = {};
  if (avail >= 1) {
    out.u8 = readUnsigned(bytes, offset, 1, endianness);
    out.i8 = readSigned(bytes, offset, 1, endianness);
  }
  if (avail >= 2) {
    out.u16 = readUnsigned(bytes, offset, 2, endianness);
    out.i16 = readSigned(bytes, offset, 2, endianness);
  }
  if (avail >= 4) {
    out.u32 = readUnsigned(bytes, offset, 4, endianness);
    out.i32 = readSigned(bytes, offset, 4, endianness);
    out.f32 = readF32(bytes, offset, endianness);
  }
  if (avail >= 8) {
    out.u64 = readUnsigned(bytes, offset, 8, endianness);
    out.i64 = readSigned(bytes, offset, 8, endianness);
    out.f64 = readF64(bytes, offset, endianness);
  }
  return out;
}
