import { test } from "node:test";
import assert from "node:assert/strict";
import {
  readUnsigned,
  readSigned,
  toSigned,
  readF64,
  interpret,
} from "../src/core/endian.ts";

const bytes = new Uint8Array([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);

test("little-endian unsigned reads", () => {
  assert.equal(readUnsigned(bytes, 0, 1, "little"), 0x01n);
  assert.equal(readUnsigned(bytes, 0, 2, "little"), 0x0201n);
  assert.equal(readUnsigned(bytes, 0, 4, "little"), 0x04030201n);
  assert.equal(readUnsigned(bytes, 0, 8, "little"), 0x0807060504030201n);
});

test("big-endian unsigned reads", () => {
  assert.equal(readUnsigned(bytes, 0, 2, "big"), 0x0102n);
  assert.equal(readUnsigned(bytes, 0, 4, "big"), 0x01020304n);
  assert.equal(readUnsigned(bytes, 0, 8, "big"), 0x0102030405060708n);
});

test("sign extension", () => {
  assert.equal(toSigned(0xffn, 1), -1n);
  assert.equal(toSigned(0x80n, 1), -128n);
  assert.equal(toSigned(0x7fn, 1), 127n);
  const neg = new Uint8Array([0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
  assert.equal(readSigned(neg, 0, 8, "little"), -1n);
  assert.equal(readUnsigned(neg, 0, 8, "little"), 0xffffffffffffffffn);
});

test("f64 interpretation", () => {
  // 1.0 as little-endian IEEE-754 double
  const one = new Uint8Array([0, 0, 0, 0, 0, 0, 0xf0, 0x3f]);
  assert.equal(readF64(one, 0, "little"), 1.0);
});

test("interpret only reports widths that fit", () => {
  const short = new Uint8Array([0xaa, 0xbb, 0xcc]);
  const out = interpret(short, 0, "little");
  assert.equal(out.u8, 0xaan);
  assert.equal(out.u16, 0xbbaan);
  assert.equal(out.u32, undefined);
  assert.equal(out.u64, undefined);
});
