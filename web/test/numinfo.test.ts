import { test } from "node:test";
import assert from "node:assert/strict";
import {
  parseNumberLiteral, signedValue, nibbles, readings, atWidth,
} from "../src/core/numinfo.ts";

test("parses hex, binary, octal, decimal, trailing-h and char literals", () => {
  assert.equal(parseNumberLiteral("0x2a")!.value, 42n);
  assert.equal(parseNumberLiteral("0b1011")!.value, 11n);
  assert.equal(parseNumberLiteral("0o52")!.value, 42n);
  assert.equal(parseNumberLiteral("42")!.value, 42n);
  assert.equal(parseNumberLiteral("2ah")!.value, 42n);
  assert.equal(parseNumberLiteral("'A'")!.value, 65n);
  assert.equal(parseNumberLiteral("'\\n'")!.value, 10n);
  assert.equal(parseNumberLiteral("printf"), null);
  assert.equal(parseNumberLiteral("rax"), null);
});

test("0xff is 255 unsigned and -1 signed at 8 bits", () => {
  const info = parseNumberLiteral("0xff")!;
  assert.equal(info.width, 8);
  assert.equal(info.value, 255n);
  assert.equal(signedValue(info), -1n);
  const r = readings(info);
  assert.equal(r.decimalUnsigned, "255");
  assert.equal(r.decimalSigned, "-1");
  assert.equal(r.hex, "0xff");
  assert.equal(r.binary, "1111 1111");
});

test("a negative literal is stored as its two's-complement pattern", () => {
  const info = parseNumberLiteral("-1")!;
  assert.equal(info.value, 0xffn);
  assert.equal(signedValue(info), -1n);
});

test("the same bit pattern re-reads at a wider width", () => {
  const info = atWidth(parseNumberLiteral("0xff")!, 16);
  assert.equal(info.value, 0x00ffn);
  assert.equal(signedValue(info), 255n, "0x00ff is +255 as a signed 16-bit");
});

test("nibbles group four bits under one hex digit", () => {
  const nibs = nibbles(parseNumberLiteral("0x2a")!); // 8-bit -> two nibbles
  assert.equal(nibs.length, 2);
  assert.deepEqual(nibs.map((n) => n.hex), ["2", "a"]);
  // 0xa = 1010: bits (msb-first) set, clear, set, clear.
  assert.deepEqual(nibs[1]!.bits.map((b) => b.set), [true, false, true, false]);
  // place value of the low set bit in the 'a' nibble is 2^1 = 2.
  const lowSet = nibs[1]!.bits.find((b) => b.index === 1)!;
  assert.equal(lowSet.place, 2n);
});
