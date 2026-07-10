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
import { isKnownRegister } from "./reginfo.ts";

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

function regChip(word: string): string {
  return `<span class="tok tok-reg" role="button" tabindex="0" data-help="reg" data-reg="${word.toLowerCase()}">${word}</span>`;
}

/** A hex-dump byte: displays the bare pair (e.g. "b8") but the card reads it as
 *  a hex value (0xb8). Bare bytes have no `0x`, so they need this normalisation
 *  the plain number tokenizer can't do without knowing it is looking at a dump. */
function hexByteChip(byte: string): string {
  return `<span class="tok tok-num" role="button" tabindex="0" data-help="num" data-lit="0x${byte}">${byte}</span>`;
}

/**
 * Is this line a hex dump — whitespace-separated two-hex-digit bytes, at least
 * two of them? Then every group is a byte, and bare pairs like `b8` or `2a` are
 * hex, not decimal or a stray word. Detecting the whole line avoids guessing at
 * an ambiguous single token (`22` alone is decimal; in `48 22 00` it is hex).
 */
function isHexDumpLine(line: string): boolean {
  const groups = line.trim().split(/\s+/);
  if (groups.length < 2) return false;
  return groups.every((g) => /^[0-9a-fA-F]{2}$/.test(g));
}

/**
 * Wrap the numbers, mnemonics and registers inside already-escaped `code` text.
 * `context` — the full instruction text a mnemonic belongs to, if known — is
 * attached so the instruction card can offer that instruction's encoding.
 */
export function tokenizeCodeToHtml(escaped: string, context?: string): string {
  // Process line by line so a hex-dump line can be recognised as a whole.
  return escaped
    .split("\n")
    .map((line) => (isHexDumpLine(line) ? tokenizeHexDump(line) : tokenizeAsm(line, context)))
    .join("\n");
}

function tokenizeHexDump(line: string): string {
  // The line is only 2-hex-digit groups and whitespace, so replacing each pair
  // is safe and leaves the spacing intact.
  return line.replace(/[0-9a-fA-F]{2}/g, (b) => hexByteChip(b));
}

function tokenizeAsm(line: string, context?: string): string {
  let out = "";
  let last = 0;
  for (const m of line.matchAll(TOKEN_RE)) {
    const [full, charLit, numLit, word] = m;
    const at = m.index;
    out += line.slice(last, at);
    last = at + full.length;

    if (charLit && parseNumberLiteral(charLit)) {
      out += numChip(charLit);
    } else if (numLit && parseNumberLiteral(numLit)) {
      out += numChip(numLit);
    } else if (word && isKnownMnemonic(word)) {
      out += context
        ? insnChip(word).replace("data-insn=", `data-context="${escapeAttr(context)}" data-insn=`)
        : insnChip(word);
    } else if (word && isKnownRegister(word)) {
      out += regChip(word);
    } else {
      out += full;
    }
  }
  out += line.slice(last);
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

/** A forced register chip for the `:reg[..]` directive. */
export function forcedRegChip(word: string): string {
  const cleaned = word.trim();
  return isKnownRegister(cleaned) ? regChip(cleaned) : escapeAttr(cleaned);
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

/** An always-open inline embed placeholder for the `:::number` /
 * `:::instruction` / `:::register` block directives; hydrated by info-popover
 * after render. */
export function embedPlaceholder(kind: "number" | "instruction" | "register", arg: string): string {
  return `<div class="help-embed" data-embed="${kind}" data-arg="${escapeAttr(arg.trim())}"></div>`;
}
