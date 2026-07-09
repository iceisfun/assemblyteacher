import { test } from "node:test";
import assert from "node:assert/strict";
import { parseWord, formatWord } from "../src/core/word.ts";

test("parseWord handles the full 64-bit range", () => {
  assert.equal(parseWord("0xffffffffffffffff"), 2n ** 64n - 1n);
  assert.equal(parseWord("0x0"), 0n);
  assert.equal(parseWord("0x401000"), 0x401000n);
});

test("parseWord is input-tolerant: hex, decimal, number, bigint", () => {
  assert.equal(parseWord("255"), 255n);
  assert.equal(parseWord(255), 255n);
  assert.equal(parseWord(0x401000), 0x401000n);
  assert.equal(parseWord(42n), 42n);
  assert.equal(parseWord("-1"), -1n);
  assert.equal(parseWord(""), 0n);
});

test("formatWord always emits canonical unsigned 0x…", () => {
  assert.equal(formatWord(0n), "0x0");
  assert.equal(formatWord(0x401000n), "0x401000");
  assert.equal(formatWord(2n ** 64n - 1n), "0xffffffffffffffff");
  // negative wraps to two's-complement u64
  assert.equal(formatWord(-1n), "0xffffffffffffffff");
});

test("parseWord ∘ formatWord round-trips the extremes", () => {
  for (const v of [0n, 1n, 0x401000n, 2n ** 63n, 2n ** 64n - 1n]) {
    assert.equal(parseWord(formatWord(v)), v);
  }
  // the specific case the wire format exists to preserve
  assert.equal(parseWord(formatWord(2n ** 64n - 1n)), 2n ** 64n - 1n);
});
