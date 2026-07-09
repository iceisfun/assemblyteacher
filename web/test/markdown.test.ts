import { test } from "node:test";
import assert from "node:assert/strict";
import { renderMarkdown } from "../src/core/markdown.ts";

test("a GFM table becomes a real <table>, not a paragraph of pipes", () => {
  const src = [
    "| binary | hex | decimal |",
    "|--------|-----|---------|",
    "| `0000` | `0` | 0       |",
    "| `1111` | `f` | 15      |",
  ].join("\n");
  const html = renderMarkdown(src);
  assert.match(html, /<table class="md-table">/);
  assert.match(html, /<th>binary<\/th>/);
  // Inline code inside a cell is still rendered.
  assert.match(html, /<td><code>0000<\/code><\/td>/);
  assert.match(html, /<td>15<\/td>/);
  // The pipes must not survive as literal text in a paragraph.
  assert.doesNotMatch(html, /<p>[^<]*\|/);
});

test("column alignment from the separator row is honoured", () => {
  const src = ["| a | b | c |", "|:--|:-:|--:|", "| 1 | 2 | 3 |"].join("\n");
  const html = renderMarkdown(src);
  assert.match(html, /text-align:left/);
  assert.match(html, /text-align:center/);
  assert.match(html, /text-align:right/);
});

test("the wide binary/hex table with a spacer column renders as one table", () => {
  // This is the two-up layout from the Binary and Hexadecimal lesson: seven
  // columns with an empty header in the middle acting as a gap.
  const src = [
    "| binary | hex | decimal |   | binary | hex | decimal |",
    "|--------|-----|---------|---|--------|-----|---------|",
    "| `0000` | `0` | 0       |   | `1000` | `8` | 8       |",
  ].join("\n");
  const html = renderMarkdown(src);
  assert.equal((html.match(/<table/g) ?? []).length, 1);
  // `<th[ >]` so `<thead>` is not miscounted as a header cell.
  assert.equal((html.match(/<th[ >]/g) ?? []).length, 7);
});

test("a lone pipe in a paragraph is not mistaken for a table", () => {
  const html = renderMarkdown("The `a | b` idiom sets bits in a.");
  assert.doesNotMatch(html, /<table/);
  assert.match(html, /<p>/);
});

test("a table stops at a blank line and the following prose is separate", () => {
  const src = [
    "| x | y |",
    "|---|---|",
    "| 1 | 2 |",
    "",
    "Prose after the table.",
  ].join("\n");
  const html = renderMarkdown(src);
  assert.match(html, /<\/table>/);
  assert.match(html, /<p>Prose after the table\.<\/p>/);
});
