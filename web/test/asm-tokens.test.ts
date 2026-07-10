import { test } from "node:test";
import assert from "node:assert/strict";
import {
  tokenizeCodeToHtml, forcedNumChip, forcedInsnChip, embedPlaceholder,
} from "../src/core/asm-tokens.ts";

test("numbers and mnemonics inside code become interactive tokens", () => {
  const html = tokenizeCodeToHtml("mov al, 0x2a");
  assert.match(html, /class="tok tok-insn"[^>]*data-insn="mov"/);
  assert.match(html, /class="tok tok-num"[^>]*data-lit="0x2a"/);
  // A register is left plain.
  assert.ok(html.includes("al,"));
  assert.ok(!/data-insn="al"/.test(html));
});

test("a non-mnemonic word is not tokenized", () => {
  const html = tokenizeCodeToHtml("call printf");
  assert.match(html, /data-insn="call"/);
  assert.ok(!/data-insn="printf"/.test(html), "printf is not a mnemonic");
});

test("a mnemonic that is also an English word is still tokenized in code", () => {
  assert.match(tokenizeCodeToHtml("and eax, 1"), /data-insn="and"/);
});

test("forced chips and embed placeholders render", () => {
  assert.match(forcedNumChip("0x10"), /tok-num[^>]*data-lit="0x10"/);
  assert.match(forcedInsnChip("lea rax, [rbx]"), /tok-insn[^>]*data-insn="lea"/);
  assert.match(forcedInsnChip("lea rax, [rbx]"), /data-context="lea rax, \[rbx\]"/);
  assert.match(embedPlaceholder("number", "0x2a"), /help-embed[^>]*data-embed="number"[^>]*data-arg="0x2a"/);
});
