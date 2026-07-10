import { test } from "node:test";
import assert from "node:assert/strict";
import {
  parseHexBytes, valueOfBytes, bytesOfValue, signedOf, formatBytes,
} from "../src/core/convert.ts";

test("the dump case: aa bb cc dd little-endian is 0xddccbbaa", () => {
  const bytes = parseHexBytes("aa bb cc dd")!;
  assert.deepEqual(bytes, [0xaa, 0xbb, 0xcc, 0xdd]);
  assert.equal(valueOfBytes(bytes, "le"), 0xddccbbaan);
  assert.equal(valueOfBytes(bytes, "be"), 0xaabbccddn);
});

test("00 00 00 10 is 0x10000000 little-endian, 16 big-endian", () => {
  const bytes = parseHexBytes("00 00 00 10")!;
  assert.equal(valueOfBytes(bytes, "le"), 0x10000000n);
  assert.equal(valueOfBytes(bytes, "be"), 0x10n);
});

test("parseHexBytes tolerates spacing, 0x prefixes, commas and no separators", () => {
  assert.deepEqual(parseHexBytes("aabbccdd"), [0xaa, 0xbb, 0xcc, 0xdd]);
  assert.deepEqual(parseHexBytes("0xaa 0xbb"), [0xaa, 0xbb]);
  assert.deepEqual(parseHexBytes("de,ad,be,ef"), [0xde, 0xad, 0xbe, 0xef]);
  assert.equal(parseHexBytes("abc"), null, "odd digit count is rejected");
  assert.equal(parseHexBytes("zz"), null);
});

test("value to bytes round-trips through both orders", () => {
  const v = 0xddccbbaan;
  assert.equal(formatBytes(bytesOfValue(v, 4, "le")), "aa bb cc dd");
  assert.equal(formatBytes(bytesOfValue(v, 4, "be")), "dd cc bb aa");
  assert.equal(valueOfBytes(bytesOfValue(v, 4, "le"), "le"), v);
});

test("signed reading is two's complement at the byte width", () => {
  assert.equal(signedOf(0xffn, 8), -1n);
  assert.equal(signedOf(0xffffff00n, 32), -256n);
  assert.equal(signedOf(0x7fn, 8), 127n);
});

test("a full 64-bit value survives", () => {
  const bytes = parseHexBytes("ff ff ff ff ff ff ff ff")!;
  assert.equal(valueOfBytes(bytes, "le"), 0xffffffffffffffffn);
  assert.equal(signedOf(valueOfBytes(bytes, "le"), 64), -1n);
});
