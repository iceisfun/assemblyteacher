// The full instruction reference (#/instructions, #/instructions/<mnemonic>):
// the supported x86-64 instruction set on one linkable page. The left panel is
// a detailed card for the selected instruction; the right is a search box over
// a browse-by-category index, with the selected mnemonic highlighted.
//
// Selection lives in the URL, so any instruction is directly linkable and the
// hover card can jump here with "full instruction reference →". Register names
// and numeric literals inside examples become the same interactive tokens used
// throughout the app, so the reference cross-links into the Register Reference
// and the number/byte helpers without extra wiring.

import { installTokenHelpers } from "../components/info-popover.ts";
import { tokenizeCodeToHtml } from "../core/asm-tokens.ts";
import {
  allInsnEntries,
  FLAGS,
  INSN_CATEGORIES,
  lookupInsnEntry,
  searchInsns,
  type Flag,
  type FlagEffect,
  type InsnEntry,
} from "../core/insninfo.ts";

function el(tag: string, className?: string, text?: string): HTMLElement {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function goTo(mnemonic: string): void {
  location.hash = `#/instructions/${mnemonic}`;
}

// ---- flag panel -------------------------------------------------------------

const FLAG_NAME: Record<Flag, string> = {
  CF: "carry", OF: "overflow", SF: "sign", ZF: "zero", AF: "adjust", PF: "parity",
};
const EFFECT_LABEL: Record<FlagEffect | "preserved", string> = {
  written: "set from result",
  cleared: "forced 0",
  set: "forced 1",
  undefined: "undefined",
  tested: "read",
  preserved: "unchanged",
};

function flagPanel(entry: InsnEntry): HTMLElement {
  const wrap = el("div", "insn-flags-grid");
  for (const f of FLAGS) {
    const effect: FlagEffect | "preserved" = entry.flagEffects[f] ?? "preserved";
    const chip = el("span", `insn-flag insn-flag-${effect}`);
    chip.appendChild(el("span", "insn-flag-name", f));
    chip.appendChild(el("span", "insn-flag-effect", EFFECT_LABEL[effect]));
    chip.title = `${FLAG_NAME[f]} flag — ${EFFECT_LABEL[effect]}`;
    wrap.appendChild(chip);
  }
  return wrap;
}

// ---- visual explanations ----------------------------------------------------
// Small, honest diagrams for the categories where seeing the bits or the stack
// move teaches more than prose. Returns null when a diagram wouldn't help.

function visualFor(mnemonic: string): HTMLElement | null {
  const pre = (title: string, body: string): HTMLElement => {
    const box = el("div", "insn-visual");
    box.appendChild(el("div", "insn-visual-title", title));
    box.appendChild(el("pre", "insn-visual-art", body));
    return box;
  };
  switch (mnemonic) {
    case "shl": case "sal":
      return pre("shift left by 1 (×2)",
        "  1 0 1 1 0 1 1 0\n" +
        "   ↖ ↖ ↖ ↖ ↖ ↖ ↖ ↖ 0\n" +
        "CF←1  0 1 1 0 1 1 0 0");
    case "shr":
      return pre("shift right by 1 (unsigned ÷2)",
        "  1 0 1 1 0 1 1 0\n" +
        "0 ↘ ↘ ↘ ↘ ↘ ↘ ↘ ↘  →CF\n" +
        "  0 1 0 1 1 0 1 1  0");
    case "sar":
      return pre("shift right by 1 (signed ÷2, sign copied)",
        "  1 0 1 1 0 1 1 0   (top bit = sign)\n" +
        "  ↘ ↘ ↘ ↘ ↘ ↘ ↘ ↘  →CF\n" +
        "  1 1 0 1 1 0 1 1  0   (sign refilled)");
    case "rol":
      return pre("rotate left by 1 (top bit wraps to bottom)",
        "  1 0 1 1 0 1 1 0\n" +
        "  └──────────────┐\n" +
        "  0 1 1 0 1 1 0 1   CF←1");
    case "ror":
      return pre("rotate right by 1 (bottom bit wraps to top)",
        "  1 0 1 1 0 1 1 0\n" +
        "  ┌──────────────┘\n" +
        "  0 1 0 1 1 0 1 1   CF←0");
    case "push":
      return pre("push value (stack grows down)",
        "before        after\n" +
        "  ....          value  ← rsp\n" +
        "  old  ← rsp    old\n" +
        "  ....          ....\n" +
        "(rsp -= 8, then store)");
    case "pop":
      return pre("pop value (stack shrinks up)",
        "before          after\n" +
        "  value ← rsp     value\n" +
        "  next            next  ← rsp\n" +
        "(load, then rsp += 8)");
    case "add":
      return pre("add (binary, carries propagate)",
        "   0 0 0 0 0 1 0 1   (5)\n" +
        " + 0 0 0 0 0 0 1 1   (3)\n" +
        " ---------------\n" +
        "   0 0 0 0 1 0 0 0   (8)");
    case "sub": case "cmp":
      return pre("subtract (cmp discards the result, keeps the flags)",
        "   0 0 0 0 1 0 0 0   (8)\n" +
        " - 0 0 0 0 0 0 1 1   (3)\n" +
        " ---------------\n" +
        "   0 0 0 0 0 1 0 1   (5)   CF=borrow");
    default:
      return null;
  }
}

// ---- detail card ------------------------------------------------------------

function section(title: string): HTMLElement {
  const s = el("section", "insn-section");
  s.appendChild(el("h2", "insn-section-title", title));
  return s;
}

/** A block of interactive code lines (registers/numbers become hover tokens). */
function codeLines(lines: string[], withPlayground: boolean): HTMLElement {
  const wrap = el("div", "insn-code-list");
  for (const line of lines) {
    const row = el("div", "insn-code-row");
    const code = el("code", "insn-code");
    code.innerHTML = tokenizeCodeToHtml(escapeHtml(line));
    row.appendChild(code);
    if (withPlayground) {
      const btn = el("button", "insn-pg-btn", "▶ Playground");
      btn.title = "open this in the Playground";
      btn.addEventListener("click", () => openInPlayground(line));
      row.appendChild(btn);
    }
    wrap.appendChild(row);
  }
  return wrap;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function openInPlayground(code: string): void {
  try {
    sessionStorage.setItem("playground-prefill", code);
  } catch {
    /* private mode */
  }
  location.hash = "#/playground";
}

function renderDetail(entry: InsnEntry): HTMLElement {
  const card = el("div", "insn-detail");

  const head = el("div", "insn-detail-head");
  head.appendChild(el("span", "insn-mnemonic", entry.mnemonic));
  head.appendChild(el("span", "insn-cat", entry.category));
  card.appendChild(head);

  card.appendChild(el("p", "insn-summary", entry.summary));
  card.appendChild(el("p", "insn-desc", entry.description));

  // Syntax / operand forms.
  const syn = section("Syntax");
  syn.appendChild(codeLines(entry.syntax, false));
  card.appendChild(syn);

  // Flags — colour-coded panel plus the terse string.
  const flags = section("Flags");
  flags.appendChild(flagPanel(entry));
  flags.appendChild(el("div", "insn-flags-note", entry.flags));
  card.appendChild(flags);

  // Visual explanation, where one helps.
  const visual = visualFor(entry.mnemonic);
  if (visual) {
    const vs = section("How it moves the bits");
    vs.appendChild(visual);
    card.appendChild(vs);
  }

  // Encoding.
  if (entry.encoding) {
    const enc = section("Encoding");
    enc.appendChild(el("p", "insn-note-text", entry.encoding));
    card.appendChild(enc);
  }

  // Examples, each openable in the Playground.
  const ex = section("Examples");
  ex.appendChild(codeLines(entry.examples, true));
  card.appendChild(ex);

  // Architectural notes.
  if (entry.notes.length) {
    const notes = section("Notes");
    const ul = el("ul", "insn-notes");
    for (const n of entry.notes) ul.appendChild(el("li", undefined, n));
    notes.appendChild(ul);
    card.appendChild(notes);
  }

  // Related instructions — clickable, navigating the reference.
  if (entry.related.length) {
    const rel = section("Related");
    const chips = el("div", "insn-related");
    for (const r of entry.related) {
      if (!lookupInsnEntry(r)) continue;
      const chip = el("span", "insn-related-chip", r);
      chip.tabIndex = 0;
      chip.setAttribute("role", "button");
      chip.addEventListener("click", () => goTo(r));
      chip.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          goTo(r);
        }
      });
      chips.appendChild(chip);
    }
    rel.appendChild(chips);
    card.appendChild(rel);
  }

  return card;
}

// ---- browse / search index --------------------------------------------------

function indexChip(mnemonic: string, selected: string): HTMLElement {
  const chip = el("span", `insn-index-chip${mnemonic === selected ? " selected" : ""}`, mnemonic);
  chip.tabIndex = 0;
  chip.setAttribute("role", "button");
  chip.addEventListener("click", () => goTo(mnemonic));
  chip.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      goTo(mnemonic);
    }
  });
  return chip;
}

function renderIndex(container: HTMLElement, selected: string, query: string): void {
  container.innerHTML = "";

  if (query.trim()) {
    const hits = searchInsns(query);
    const head = el("div", "insn-index-cat");
    head.appendChild(el("span", "insn-index-cat-name", `results (${hits.length})`));
    container.appendChild(head);
    if (!hits.length) {
      container.appendChild(el("div", "insn-index-empty", `No instruction matches “${query}”.`));
      return;
    }
    const list = el("div", "insn-index-chips");
    for (const m of hits) list.appendChild(indexChip(m, selected));
    container.appendChild(list);
    return;
  }

  // Browse by category.
  const byCat = new Map<string, InsnEntry[]>();
  for (const e of allInsnEntries()) {
    const arr = byCat.get(e.category) ?? [];
    arr.push(e);
    byCat.set(e.category, arr);
  }
  for (const cat of INSN_CATEGORIES) {
    const list = byCat.get(cat);
    if (!list || !list.length) continue;
    const block = el("div", "insn-index-cat");
    block.appendChild(el("span", "insn-index-cat-name", cat));
    const chips = el("div", "insn-index-chips");
    for (const e of list) chips.appendChild(indexChip(e.mnemonic, selected));
    block.appendChild(chips);
    container.appendChild(block);
  }
}

// ---- page -------------------------------------------------------------------

export function renderInstructions(view: HTMLElement, selectedRaw?: string): void {
  const selected = selectedRaw && lookupInsnEntry(selectedRaw) ? selectedRaw.toLowerCase() : "mov";
  const entry = lookupInsnEntry(selected)!;
  view.innerHTML = "";

  const page = el("div", "insn-page");
  const header = el("div", "insn-page-header");
  header.appendChild(el("h1", undefined, "The x86-64 Instruction Reference"));
  header.appendChild(
    el(
      "p",
      "insn-page-intro",
      "What each instruction does, when to reach for it, which flags it touches, how it's " +
        "encoded, and what's related. Search by mnemonic, a keyword like “multiply”, or a " +
        "concept like “stack”. Registers and numbers in the examples are interactive, and " +
        "every example opens in the Playground.",
    ),
  );
  page.appendChild(header);

  const layout = el("div", "insn-page-layout");

  const detail = el("div", "insn-page-detail");
  detail.appendChild(renderDetail(entry));
  layout.appendChild(detail);

  const side = el("div", "insn-page-side");
  const search = document.createElement("input");
  search.className = "insn-search";
  search.type = "search";
  search.placeholder = "search: mov · multiply · stack · jump…";
  search.setAttribute("aria-label", "search instructions");
  search.spellcheck = false;
  side.appendChild(search);

  const index = el("div", "insn-index");
  side.appendChild(index);
  layout.appendChild(side);

  page.appendChild(layout);
  view.appendChild(page);

  renderIndex(index, selected, "");
  search.addEventListener("input", () => renderIndex(index, selected, search.value));

  // Make the register/number tokens in the examples live.
  installTokenHelpers(detail);

  // Bring the selected chip into view on a deep link.
  detail.scrollIntoView?.({ block: "nearest", behavior: "auto" });
}
