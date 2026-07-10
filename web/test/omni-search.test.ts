import { test } from "node:test";
import assert from "node:assert/strict";
import { searchRegs } from "../src/core/reginfo.ts";
import { searchEntities } from "../src/core/omni-search.ts";

test("searchRegs ranks an exact register name first", () => {
  assert.equal(searchRegs("eax")[0], "eax");
  assert.equal(searchRegs("r8")[0], "r8");
});

test("searchRegs matches a family by prefix", () => {
  const hits = searchRegs("r8");
  for (const r of ["r8", "r8d", "r8w", "r8b"]) assert.ok(hits.includes(r), r);
});

test("searchRegs bridges a role word to the canonical register", () => {
  assert.equal(searchRegs("counter")[0], "rcx");
  assert.equal(searchRegs("stack pointer")[0], "rsp");
});

test("searchRegs does not let a single letter match half the file", () => {
  // "a" is a prefix of ax/al/ah only; it must not substring-match rax/eax/etc.
  const hits = searchRegs("a");
  assert.ok(!hits.includes("rax"), "substring match suppressed for 1-char query");
});

test("searchEntities surfaces an entity token inside a multi-word query", () => {
  // "eax foo bar" matches no register as a phrase, but the "eax" token does.
  const r = searchEntities("eax foo bar");
  assert.ok(r.registers.some((h) => h.label === "EAX"), "eax token should surface EAX");
});

test("searchEntities returns register and instruction hits with links", () => {
  const eax = searchEntities("eax");
  assert.equal(eax.registers[0]?.kind, "register");
  assert.equal(eax.registers[0]?.label, "EAX");
  assert.equal(eax.registers[0]?.href, "#/registers/eax");

  const mov = searchEntities("mov");
  const top = mov.instructions[0];
  assert.equal(top?.kind, "instruction");
  assert.equal(top?.label, "MOV");
  assert.equal(top?.href, "#/instructions/mov");
  assert.ok(top && top.sub.length > 0, "instruction hit carries a summary");
});
