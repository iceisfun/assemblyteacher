import { test } from "node:test";
import assert from "node:assert/strict";
import { renderMarkdown } from "../src/core/markdown.ts";

test("numbers inside inline code light up in rendered prose", () => {
  const html = renderMarkdown("Load `0x2a` into `al`.");
  assert.match(html, /<code>.*tok-num[^>]*data-lit="0x2a"/);
});

test("inline :num[] and :insn[] directives render chips", () => {
  const html = renderMarkdown("The value :num[0x10] and the :insn[lea] instruction.");
  assert.match(html, /tok-num[^>]*data-lit="0x10"/);
  assert.match(html, /tok-insn[^>]*data-insn="lea"/);
});

test("the block :::number directive emits an inline embed", () => {
  const html = renderMarkdown(":::number 0x2a");
  assert.match(html, /help-embed[^>]*data-embed="number"[^>]*data-arg="0x2a"/);
});

test("a fenced code block is tokenized too", () => {
  const html = renderMarkdown("```\nmov eax, 1\nret\n```");
  assert.match(html, /<pre class="md-code">/);
  assert.match(html, /data-insn="mov"/);
  assert.match(html, /data-insn="ret"/);
});

test("the block :::register directive emits a register embed", () => {
  const html = renderMarkdown(":::register al");
  assert.match(html, /help-embed[^>]*data-embed="register"[^>]*data-arg="al"/);
});
