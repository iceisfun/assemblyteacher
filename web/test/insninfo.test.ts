import { test } from "node:test";
import assert from "node:assert/strict";
import { lookupInsn, isKnownMnemonic } from "../src/core/insninfo.ts";

test("known mnemonics resolve, unknown words do not", () => {
  assert.ok(isKnownMnemonic("mov"));
  assert.ok(isKnownMnemonic("lea"));
  assert.ok(isKnownMnemonic("and"));
  assert.ok(!isKnownMnemonic("printf"));
  assert.ok(!isKnownMnemonic("rax"));
});

test("condition suffixes resolve for jcc / setcc / cmovcc", () => {
  assert.match(lookupInsn("jne")!.summary, /Jump.*not equal/i);
  assert.match(lookupInsn("setg")!.summary, /Set.*greater.*signed/i);
  assert.match(lookupInsn("cmovb")!.summary, /copy.*below.*unsigned/i);
  assert.equal(lookupInsn("jzz"), null);
});

test("jmp is the unconditional jump, not a jcc for condition 'mp'", () => {
  assert.match(lookupInsn("jmp")!.summary, /Unconditional/);
});

test("flag notes reflect the integer core's real behaviour", () => {
  assert.match(lookupInsn("inc")!.flags, /not CF/);
  assert.equal(lookupInsn("mov")!.flags, "none");
  assert.equal(lookupInsn("lea")!.flags, "none");
});
