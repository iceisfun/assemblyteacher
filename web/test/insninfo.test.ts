import { test } from "node:test";
import assert from "node:assert/strict";
import {
  lookupInsn,
  isKnownMnemonic,
  lookupInsnEntry,
  allInsnEntries,
  searchInsns,
  INSN_CATEGORIES,
} from "../src/core/insninfo.ts";

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

test("rich entries carry syntax, examples, related, and structured flags", () => {
  const add = lookupInsnEntry("add")!;
  assert.ok(add.syntax.length > 0, "has operand forms");
  assert.ok(add.examples.length > 0, "has examples");
  assert.ok(add.related.includes("sub"), "add relates to sub");
  assert.equal(add.flagEffects.CF, "written");
  assert.equal(add.flagEffects.OF, "written");

  // inc keeps CF: the whole point of the instruction.
  const inc = lookupInsnEntry("inc")!;
  assert.equal(inc.flagEffects.CF, undefined, "inc must not touch CF");
  assert.equal(inc.flagEffects.ZF, "written");

  // logic ops force CF/OF to zero.
  assert.equal(lookupInsnEntry("and")!.flagEffects.CF, "cleared");
  // mov touches nothing.
  assert.deepEqual(lookupInsnEntry("mov")!.flagEffects, {});
});

test("condition families produce rich entries that read the right flags", () => {
  const jne = lookupInsnEntry("jne")!;
  assert.equal(jne.category, "control flow");
  assert.equal(jne.flagEffects.ZF, "tested");
  const setg = lookupInsnEntry("setg")!;
  assert.equal(setg.category, "conditional");
  // signed greater reads SF, OF and ZF.
  assert.equal(setg.flagEffects.SF, "tested");
  assert.equal(setg.flagEffects.ZF, "tested");
});

test("the catalog enumerates every category and has no duplicate mnemonics", () => {
  const all = allInsnEntries();
  const seen = new Set<string>();
  for (const e of all) {
    assert.ok(!seen.has(e.mnemonic), `duplicate ${e.mnemonic}`);
    seen.add(e.mnemonic);
    assert.ok(INSN_CATEGORIES.includes(e.category as (typeof INSN_CATEGORIES)[number]), e.category);
  }
  // Every category has at least one instruction.
  for (const cat of INSN_CATEGORIES) {
    assert.ok(all.some((e) => e.category === cat), `category ${cat} is populated`);
  }
});

test("search matches mnemonics, concepts, and keywords", () => {
  // exact / prefix mnemonic ranks first.
  assert.equal(searchInsns("mov")[0], "mov");
  assert.ok(searchInsns("cm").includes("cmp"), "partial mnemonic");
  // concept words fan out to a family.
  const mul = searchInsns("multiply");
  assert.ok(mul.includes("mul") && mul.includes("imul"), "multiply → mul/imul");
  const stack = searchInsns("stack");
  for (const m of ["push", "pop", "call", "ret"]) assert.ok(stack.includes(m), `stack → ${m}`);
  // free-text over descriptions.
  assert.ok(searchInsns("endianness").includes("bswap"), "keyword in description");
  assert.deepEqual(searchInsns(""), []);
});
