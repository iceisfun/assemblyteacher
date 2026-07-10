import { test } from "node:test";
import assert from "node:assert/strict";
import {
  tokenizeCodeToHtml, forcedNumChip, forcedInsnChip, embedPlaceholder, tokenizeProse,
} from "../src/core/asm-tokens.ts";

test("numbers, mnemonics and registers inside code become interactive tokens", () => {
  const html = tokenizeCodeToHtml("mov al, 0x2a");
  assert.match(html, /class="tok tok-insn"[^>]*data-insn="mov"/);
  assert.match(html, /class="tok tok-num"[^>]*data-lit="0x2a"/);
  // The register is now interactive too, as a distinct kind.
  assert.match(html, /class="tok tok-reg"[^>]*data-reg="al"/);
  assert.ok(!/data-insn="al"/.test(html), "al is a register, not a mnemonic");
});

test("register names of every width are recognised", () => {
  for (const r of ["rax", "eax", "ax", "al", "ah", "r8", "r8d", "rsp", "spl"]) {
    assert.match(tokenizeCodeToHtml(r), new RegExp(`data-reg="${r}"`), r);
  }
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

test("a hex-dump line treats every bare byte as hex, not decimal or a word", () => {
  const html = tokenizeCodeToHtml("b8 2a 00 00 00");
  // b8 starts with a letter and would otherwise be a plain word; 2a would be
  // mis-split into decimal 2 + 'a'. Both are now hex bytes.
  assert.match(html, /data-lit="0xb8">b8</);
  assert.match(html, /data-lit="0x2a">2a</);
  assert.equal((html.match(/data-lit="0x00"/g) ?? []).length, 3);
  assert.equal((html.match(/tok-num/g) ?? []).length, 5);
});

test("a real instruction's bytes tokenize as hex", () => {
  const html = tokenizeCodeToHtml("48 8b 44 24 08");
  assert.match(html, /data-lit="0x48"/);
  assert.match(html, /data-lit="0x8b"/);
  assert.equal((html.match(/tok-num/g) ?? []).length, 5);
});

test("prose links registers and hex literals, but not mnemonic-like English words", () => {
  const html = tokenizeProse(
    "the call number goes in rax and arguments in rdi, rsi, r10; or set bit 0x80",
  );
  // Registers become interactive tokens.
  for (const r of ["rax", "rdi", "rsi", "r10"]) {
    assert.match(html, new RegExp(`data-reg="${r}"`), r);
  }
  // A hex literal becomes a number token.
  assert.match(html, /data-lit="0x80"/);
  // English words that are also mnemonics ("call", "or", "and", "set") stay plain.
  assert.ok(!/data-insn=/.test(html), "no mnemonic tokens in prose");
  // A bare decimal ("0" in "bit 0") is not linkified — that would be prose noise.
  assert.ok(!/data-lit="0"/.test(html), "bare decimals stay plain in prose");
});

test("an assembly line is NOT mistaken for a hex dump", () => {
  const html = tokenizeCodeToHtml("mov eax, 1");
  assert.match(html, /data-insn="mov"/);
  assert.match(html, /data-reg="eax"/);
  assert.match(html, /data-lit="1"/); // decimal 1, not 0x1
  assert.ok(!/data-lit="0x/.test(html), "no bare byte was hex-normalized");
});
