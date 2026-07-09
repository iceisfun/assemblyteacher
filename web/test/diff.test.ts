import { test } from "node:test";
import assert from "node:assert/strict";
import { diffBytes, anyChanged } from "../src/core/diff.ts";

test("diffBytes reports changed indices", () => {
  const a = new Uint8Array([1, 2, 3, 4]);
  const b = new Uint8Array([1, 9, 3, 8]);
  assert.deepEqual([...diffBytes(a, b)].sort((x, y) => x - y), [1, 3]);
});

test("diffBytes treats length growth as changed", () => {
  const a = new Uint8Array([1, 2]);
  const b = new Uint8Array([1, 2, 3]);
  assert.deepEqual([...diffBytes(a, b)], [2]);
});

test("diffBytes treats length shrink as changed", () => {
  const a = new Uint8Array([1, 2, 3]);
  const b = new Uint8Array([1, 2]);
  assert.deepEqual([...diffBytes(a, b)], [2]);
});

test("anyChanged short-circuits", () => {
  assert.equal(anyChanged(new Uint8Array([1, 2]), new Uint8Array([1, 2])), false);
  assert.equal(anyChanged(new Uint8Array([1, 2]), new Uint8Array([1, 3])), true);
  assert.equal(anyChanged(new Uint8Array([1]), new Uint8Array([1, 2])), true);
});
