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
