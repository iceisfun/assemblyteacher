import { test } from "node:test";
import assert from "node:assert/strict";
import { lookupReg, isKnownRegister, regByteRange } from "../src/core/reginfo.ts";

test("every width of a register resolves to the same family and role", () => {
  for (const n of ["rax", "eax", "ax", "al"]) {
    const info = lookupReg(n)!;
    assert.deepEqual(info.family, ["rax", "eax", "ax", "al"]);
    assert.match(info.role, /accumulator/);
    assert.equal(info.num, 0);
  }
  assert.equal(lookupReg("eax")!.width, 32);
  assert.equal(lookupReg("al")!.width, 8);
});

test("ah is a high-byte register covering bits 8–15", () => {
  const ah = lookupReg("ah")!;
  assert.equal(ah.high, true);
  assert.equal(ah.num, 0);
  assert.equal(regByteRange(ah), "bits 8–15");
});

test("callee- vs caller-saved is per the System V ABI", () => {
  assert.equal(lookupReg("rbx")!.saved, "callee-saved");
  assert.equal(lookupReg("r12")!.saved, "callee-saved");
  assert.equal(lookupReg("rbp")!.saved, "callee-saved");
  assert.equal(lookupReg("rax")!.saved, "caller-saved");
  assert.equal(lookupReg("r8")!.saved, "caller-saved");
});

test("rsp and the argument registers carry their roles", () => {
  assert.match(lookupReg("rsp")!.role, /stack pointer/);
  assert.match(lookupReg("rdi")!.role, /1st integer argument/);
  assert.match(lookupReg("rsi")!.role, /2nd integer argument/);
});

test("unknown words are not registers", () => {
  assert.ok(!isKnownRegister("printf"));
  assert.ok(!isKnownRegister("mov"));
  assert.ok(isKnownRegister("r15b"));
});

import {
  parentOf, ancestorsOf, largestOf, childrenOf, bitRangeOf, familyTree,
} from "../src/core/reginfo.ts";

test("bit ranges are exact for every width, including the high byte", () => {
  assert.deepEqual(bitRangeOf("al"), [0, 7]);
  assert.deepEqual(bitRangeOf("ah"), [8, 15]);
  assert.deepEqual(bitRangeOf("ax"), [0, 15]);
  assert.deepEqual(bitRangeOf("eax"), [0, 31]);
  assert.deepEqual(bitRangeOf("rax"), [0, 63]);
});

test("upward navigation reaches the largest register", () => {
  assert.equal(parentOf("al"), "ax");
  assert.equal(parentOf("ah"), "ax");
  assert.equal(parentOf("ax"), "eax");
  assert.equal(parentOf("eax"), "rax");
  assert.equal(parentOf("rax"), null);
  assert.deepEqual(ancestorsOf("al"), ["ax", "eax", "rax"]);
  assert.equal(largestOf("r8b"), "r8");
  assert.deepEqual(ancestorsOf("r8b"), ["r8w", "r8d", "r8"]);
});

test("downward children match the family shape", () => {
  assert.deepEqual(childrenOf("rax"), ["eax"]);
  assert.deepEqual(childrenOf("eax"), ["ax"]);
  assert.deepEqual(childrenOf("ax"), ["ah", "al"]); // A/B/C/D have a high byte
  assert.deepEqual(childrenOf("si"), ["sil"]);      // SI/DI/BP/SP do not
  assert.deepEqual(childrenOf("al"), []);
});

test("the family tree is complete and correctly nested", () => {
  const rax = familyTree("al")!; // any member yields the whole family
  assert.equal(rax.name, "rax");
  assert.equal(rax.children[0]!.name, "eax");
  const ax = rax.children[0]!.children[0]!;
  assert.equal(ax.name, "ax");
  assert.deepEqual(ax.children.map((c) => c.name), ["ah", "al"]);
  // A family without a high byte has a single 8-bit leaf.
  const r8 = familyTree("r8d")!;
  const r8w = r8.children[0]!.children[0]!;
  assert.deepEqual(r8w.children.map((c) => c.name), ["r8b"]);
});

test("every general-purpose register resolves consistently", () => {
  const names = [
    "rax","eax","ax","ah","al","rbx","ebx","bx","bh","bl",
    "rsi","esi","si","sil","rsp","esp","sp","spl",
    "r8","r8d","r8w","r8b","r15","r15b",
  ];
  for (const n of names) {
    assert.ok(familyTree(n), `${n} has a family tree`);
    assert.equal(ancestorsOf(largestOf(n)).length, 0, `${n}'s largest has no parent`);
  }
});
