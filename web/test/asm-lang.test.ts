import { test } from "node:test";
import assert from "node:assert/strict";
import {
  classifyWord,
  tokenizeLine,
  MNEMONICS,
  REGISTERS,
} from "../src/core/asm-lang.ts";

test("classifyWord recognises the token classes", () => {
  assert.equal(classifyWord("mov"), "mnemonic");
  assert.equal(classifyWord("MOV"), "mnemonic");
  assert.equal(classifyWord("jne"), "mnemonic");
  assert.equal(classifyWord("cmovg"), "mnemonic");
  assert.equal(classifyWord("rax"), "register");
  assert.equal(classifyWord("r15d"), "register");
  assert.equal(classifyWord("qword"), "size");
  assert.equal(classifyWord("global"), "directive");
  assert.equal(classifyWord("frobnicate"), "identifier");
});

test("condition-code mnemonics are generated", () => {
  for (const m of ["je", "jne", "jz", "setg", "cmovb", "jbe"]) {
    assert.ok(MNEMONICS.includes(m), `${m} missing`);
  }
});

test("register set covers all widths and legacy high bytes", () => {
  for (const r of ["rax", "eax", "ax", "al", "ah", "spl", "r8b", "rip"]) {
    assert.ok(REGISTERS.includes(r), `${r} missing`);
  }
});

test("tokenizeLine classifies a full instruction line", () => {
  const toks = tokenizeLine("  mov rax, qword [rsp+0x8]  ; load arg");
  const kinds = toks.map((t) => `${t.type}:${t.text}`);
  assert.deepEqual(kinds, [
    "mnemonic:mov",
    "register:rax",
    "operator:,",
    "size:qword",
    "operator:[",
    "register:rsp",
    "operator:+",
    "number:0x8",
    "operator:]",
    "comment:; load arg",
  ]);
});

test("tokenizeLine recognises a label definition and directive", () => {
  const toks = tokenizeLine("loop:  dec ecx");
  assert.equal(toks[0]!.type, "labelDef");
  assert.equal(toks[0]!.text, "loop:");
  assert.equal(toks[1]!.type, "mnemonic");
  assert.equal(toks[2]!.type, "register");
});

test("tokenizeLine handles binary and decimal numbers and strings", () => {
  const toks = tokenizeLine('db 0b1010, 42, "hi"');
  assert.equal(toks[0]!.type, "directive");
  assert.equal(toks[1]!.type, "number");
  assert.equal(toks[1]!.text, "0b1010");
  assert.equal(toks[5]!.type, "string");
});
