import { test } from "node:test";
import assert from "node:assert/strict";
import {
  byteHex,
  padHex,
  formatAddress,
  asciiChar,
  isPrintable,
  hexRow,
  asciiRow,
  parseHex,
  toHexString,
} from "../src/core/hex.ts";

test("byteHex zero-pads to two lowercase digits", () => {
  assert.equal(byteHex(0), "00");
  assert.equal(byteHex(0xf), "0f");
  assert.equal(byteHex(0xab), "ab");
  assert.equal(byteHex(0xff), "ff");
});

test("padHex and formatAddress", () => {
  assert.equal(padHex(0x1234, 8), "00001234");
  assert.equal(formatAddress(0x401000n), "0000000000401000");
  assert.equal(formatAddress(0x401000n, 8), "00401000");
});

test("printable classification and ascii char", () => {
  assert.equal(isPrintable(0x41), true);
  assert.equal(isPrintable(0x1f), false);
  assert.equal(isPrintable(0x7f), false);
  assert.equal(asciiChar(0x41), "A");
  assert.equal(asciiChar(0x00), ".");
});

test("hexRow lays out a full row with the mid gutter", () => {
  const bytes = new Uint8Array([
    0x48, 0x8b, 0x44, 0x24, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
  ]);
  const row = hexRow(bytes, 0, 16, 16);
  // 16 bytes, extra space after the 8th
  assert.equal(row.startsWith("48 8b 44 24 08 00 00 00  00"), true);
});

test("hexRow pads a short trailing row", () => {
  const bytes = new Uint8Array([0xaa, 0xbb]);
  const row = hexRow(bytes, 0, 2, 16);
  assert.equal(row.startsWith("aa bb   "), true);
  // total width is stable regardless of count
  assert.equal(row.length, hexRow(new Uint8Array(16), 0, 16, 16).length);
});

test("asciiRow renders non-printable as dot and pads the tail", () => {
  const bytes = new Uint8Array([0x41, 0x00, 0x7e]);
  assert.equal(asciiRow(bytes, 0, 3, 16), "A.~             ");
});

test("parseHex tolerates whitespace and 0x, rejects odd/garbage", () => {
  assert.deepEqual([...parseHex("48 8b")!], [0x48, 0x8b]);
  assert.deepEqual([...parseHex("0x480x8b")!], [0x48, 0x8b]);
  assert.equal(parseHex("48b"), null);
  assert.equal(parseHex("zz"), null);
  assert.equal(parseHex(""), null);
});

test("toHexString round-trips parseHex", () => {
  const s = "488b442408";
  assert.equal(toHexString(parseHex(s)!), s);
});
