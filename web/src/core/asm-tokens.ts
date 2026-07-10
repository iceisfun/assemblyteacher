// Turn a run of code text into HTML in which the numbers and mnemonics are
// wrapped as interactive tokens. Used in two places:
//   - the markdown renderer, over the contents of `<code>` spans and blocks
//   - the disassembly listing in the Playground
//
// The input is assumed to be ALREADY HTML-escaped (the markdown renderer
// escapes the whole source before this runs). Numbers and mnemonics contain
// none of the escaped characters, so matching on the escaped string is safe and
// the non-token text between matches is passed through verbatim.
//
// Interactive tokens carry the data the popover needs and nothing else; the
// popover wiring lives in components/info-popover.ts.

import { isKnownMnemonic } from "./insninfo.ts";
import { parseNumberLiteral } from "./numinfo.ts";

// A number literal, or a bare word (which may be a mnemonic). We tokenise words
// too so we can test each against the mnemonic set. Order matters: numbers
// first, since `0b1011` starts with a digit.
const TOKEN_RE =
  /('(?:\\.|[^'])')|(0x[0-9a-fA-F]+|0b[01]+|0o[0-7]+|[0-9][0-9a-fA-F]*[hH]|[0-9]+)|([A-Za-z_][A-Za-z0-9_]*)/g;

function numChip(lit: string): string {
  return `<span class="tok tok-num" role="button" tabindex="0" data-help="num" data-lit="${lit}">${lit}</span>`;
}

function insnChip(word: string): string {
  return `<span class="tok tok-insn" role="button" tabindex="0" data-help="insn" data-insn="${word.toLowerCase()}">${word}</span>`;
}

/**
 * Wrap the numbers and known mnemonics inside already-escaped `code` text.
 * `context` — the full instruction text a mnemonic belongs to, if known — is
 * attached so the instruction card can offer that instruction's encoding.
 */
export function tokenizeCodeToHtml(escaped: string, context?: string): string {
  let out = "";
  let last = 0;
  for (const m of escaped.matchAll(TOKEN_RE)) {
    const [full, charLit, numLit, word] = m;
    const at = m.index;
    out += escaped.slice(last, at);
    last = at + full.length;

    if (charLit && parseNumberLiteral(charLit)) {
      out += numChip(charLit);
    } else if (numLit && parseNumberLiteral(numLit)) {
      out += numChip(numLit);
    } else if (word && isKnownMnemonic(word)) {
      out += context
        ? insnChip(word).replace("data-insn=", `data-context="${escapeAttr(context)}" data-insn=`)
        : insnChip(word);
    } else {
      out += full;
    }
  }
  out += escaped.slice(last);
  return out;
}

function escapeAttr(s: string): string {
  return s.replace(/"/g, "&quot;").replace(/\n/g, " ");
}

/** A forced number chip for the `:num[..]` directive (input NOT pre-escaped). */
export function forcedNumChip(lit: string): string {
  const cleaned = escapeAttr(lit.trim());
  return parseNumberLiteral(cleaned) ? numChip(cleaned) : cleaned;
}

/** A forced instruction chip for the `:insn[..]` directive. */
export function forcedInsnChip(word: string): string {
  const cleaned = word.trim();
  return isKnownMnemonic(cleaned.split(/\s+/)[0] ?? "")
    ? insnChip(cleaned.split(/\s+/)[0]!).replace(
        "data-insn=",
        `data-context="${escapeAttr(cleaned)}" data-insn=`,
      )
    : escapeAttr(cleaned);
}

/** An always-open inline embed placeholder for the `:::number`/`:::instruction`
 * block directives; hydrated by info-popover after render. */
export function embedPlaceholder(kind: "number" | "instruction", arg: string): string {
  return `<div class="help-embed" data-embed="${kind}" data-arg="${escapeAttr(arg.trim())}"></div>`;
}
