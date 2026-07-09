import { test } from "node:test";
import assert from "node:assert/strict";
import {
  asciiNeedle,
  buildNeedle,
  findAll,
  findNext,
  findPrev,
} from "../src/core/search.ts";

const hay = new Uint8Array([
  0xde, 0xad, 0xbe, 0xef, 0x00, 0xde, 0xad, 0xbe, 0xef, 0x41, 0x42, 0x43,
]);

test("findAll returns every match start", () => {
  const needle = new Uint8Array([0xde, 0xad]);
  assert.deepEqual(findAll(hay, needle), [0, 5]);
});

test("findAll handles no match and oversized needle", () => {
  assert.deepEqual(findAll(hay, new Uint8Array([0x99])), []);
  assert.deepEqual(findAll(new Uint8Array([1]), new Uint8Array([1, 2])), []);
});

test("findNext wraps around", () => {
  const needle = new Uint8Array([0xde, 0xad]);
  assert.equal(findNext(hay, needle, 0), 0);
  assert.equal(findNext(hay, needle, 1), 5);
  assert.equal(findNext(hay, needle, 6), 0); // wrap
});

test("findPrev wraps around", () => {
  const needle = new Uint8Array([0xde, 0xad]);
  assert.equal(findPrev(hay, needle, 5), 0);
  assert.equal(findPrev(hay, needle, 12), 5);
  assert.equal(findPrev(hay, needle, 0), 5); // wrap
});

test("ascii needle matches text bytes", () => {
  assert.deepEqual([...asciiNeedle("ABC")], [0x41, 0x42, 0x43]);
  assert.deepEqual(findAll(hay, asciiNeedle("ABC")), [9]);
});

test("buildNeedle parses per mode", () => {
  assert.deepEqual([...buildNeedle("dead", "hex")!], [0xde, 0xad]);
  assert.deepEqual([...buildNeedle("AB", "ascii")!], [0x41, 0x42]);
  assert.equal(buildNeedle("", "hex"), null);
  assert.equal(buildNeedle("xyz", "hex"), null);
});
