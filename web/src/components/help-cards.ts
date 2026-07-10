// The card bodies for the two helpers. Each returns a detached element the
// popover (or an inline embed) drops into place. Kept separate from the popover
// wiring so the same card can appear as a hover/tap popover or as an always-open
// inline block.

import { assemble, explain } from "../api.ts";
import { lookupInsnEntry } from "../core/insninfo.ts";
import {
  ancestorsOf,
  bitRangeOf,
  childrenOf,
  familyTree,
  largestOf,
  lookupReg,
  parentOf,
  type RegNode,
} from "../core/reginfo.ts";
import {
  atWidth,
  nibbles,
  parseNumberLiteral,
  readings,
  type NumberInfo,
  type Width,
} from "../core/numinfo.ts";

function el(tag: string, className?: string, text?: string): HTMLElement {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

// ---- number card -----------------------------------------------------------

/** A base/binary decomposition card for a numeric literal. */
export function buildNumberCard(lit: string): HTMLElement {
  const card = el("div", "help-card help-card-num");
  const parsed = parseNumberLiteral(lit);
  if (!parsed) {
    card.appendChild(el("div", "help-title", `${lit} is not a number`));
    return card;
  }

  card.appendChild(el("div", "help-title", `${lit}`));

  const body = el("div", "help-num-body");
  card.appendChild(body);

  const detail = el("div", "help-num-detail");

  const render = (info: NumberInfo) => {
    body.innerHTML = "";
    const r = readings(info);

    const rows = el("div", "help-num-rows");
    const addRow = (label: string, value: string) => {
      const row = el("div", "help-num-row");
      row.appendChild(el("span", "help-num-label", label));
      const v = el("span", "help-num-value", value);
      v.tabIndex = 0;
      v.title = "click to copy";
      v.addEventListener("click", () => {
        void navigator.clipboard?.writeText(value.replace(/\s/g, ""));
        v.classList.add("copied");
        setTimeout(() => v.classList.remove("copied"), 600);
      });
      row.appendChild(v);
      rows.appendChild(row);
    };
    addRow("hex", r.hex);
    addRow("decimal", r.decimalUnsigned);
    addRow(`signed (${info.width}-bit)`, r.decimalSigned);
    if (r.ascii) addRow("ascii", `'${r.ascii}'`);
    body.appendChild(rows);

    // The nibble grid: four bits per group, the hex digit under each group.
    const grid = el("div", "help-nibbles");
    grid.setAttribute("aria-label", "binary decomposition, grouped by nibble");
    for (const nib of nibbles(info)) {
      const group = el("div", "help-nibble");
      const bitsRow = el("div", "help-nibble-bits");
      for (const bit of nib.bits) {
        const cell = el("span", `help-bit${bit.set ? " on" : ""}`, bit.set ? "1" : "0");
        cell.tabIndex = 0;
        const describe = () => {
          detail.textContent = `bit ${bit.index} = 2^${bit.index} = ${bit.place}${
            bit.set ? "  (set — contributes to the value)" : "  (clear)"
          }`;
        };
        cell.addEventListener("mouseenter", describe);
        cell.addEventListener("focus", describe);
        cell.addEventListener("mouseleave", () => (detail.textContent = defaultDetail(info)));
        bitsRow.appendChild(cell);
      }
      group.appendChild(bitsRow);
      group.appendChild(el("div", "help-nibble-hex", nib.hex));
      grid.appendChild(group);
    }
    body.appendChild(grid);

    detail.textContent = defaultDetail(info);
    body.appendChild(detail);

    // Width toggle: re-read the same bit pattern at 8/16/32/64.
    const widths: Width[] = [8, 16, 32, 64];
    const toggle = el("div", "help-width-toggle");
    toggle.appendChild(el("span", "help-num-label", "width"));
    for (const w of widths) {
      const btn = el("button", `help-width${w === info.width ? " active" : ""}`, String(w));
      btn.addEventListener("click", () => {
        render(atWidth(info, w));
        // The card height changes with the bit count; ask the popover to
        // re-place itself so it stays inside the viewport.
        card.dispatchEvent(new CustomEvent("help-resize", { bubbles: true }));
      });
      toggle.appendChild(btn);
    }
    body.appendChild(toggle);
  };

  render(parsed);
  return card;
}

function defaultDetail(info: NumberInfo): string {
  return `${info.width} bits · one hex digit per group of four · hover a bit for its place value`;
}

// ---- register card ---------------------------------------------------------
//
// An interactive explorer of the register file, not a static reference: the
// whole family tree is shown with the selected register highlighted; hovering a
// relative previews its bit ownership without navigating; clicking navigates the
// card to it. Every general-purpose family behaves identically.

/** Build a 64-cell bit strip. `tint(i)` classes each bit 63..0. */
function bitStrip(tint: (i: number) => string): HTMLElement {
  const strip = el("div", "reg-bits");
  strip.setAttribute("aria-hidden", "true");
  // Eight byte-groups, high byte (bits 63–56) leftmost; bit 7 leftmost in each.
  for (let b = 7; b >= 0; b--) {
    const group = el("span", "reg-bits-byte");
    for (let k = 7; k >= 0; k--) {
      const i = b * 8 + k;
      group.appendChild(el("span", `reg-bit ${tint(i)}`));
    }
    strip.appendChild(group);
  }
  return strip;
}

function ownershipTint(lo: number, hi: number): (i: number) => string {
  return (i) => (i >= lo && i <= hi ? "own" : "off");
}

/** The write-effect strip: which bits a write to this register changes, and
 *  what happens to the rest (zeroed for a 32-bit write, preserved otherwise). */
function writeTint(name: string): (i: number) => string {
  const info = lookupReg(name)!;
  const [lo, hi] = bitRangeOf(name);
  return (i) => {
    if (i >= lo && i <= hi) return "written";
    if (info.width === 32 && i >= 32) return "zeroed";
    return "preserved";
  };
}

const WIDTH_LABEL: Record<number, string> = { 8: "8-bit", 16: "16-bit", 32: "32-bit", 64: "64-bit" };

function writeExplanation(name: string): string {
  const info = lookupReg(name)!;
  const r64 = largestOf(name);
  const [lo, hi] = bitRangeOf(name);
  switch (info.width) {
    case 64:
      return `Writing ${name} replaces the entire register.`;
    case 32:
      return `Writing ${name} zero-extends into ${r64}: the upper 32 bits become zero. This is the only width that clears the rest of the register.`;
    default:
      return `Writing ${name} changes only bits ${lo}–${hi}; the other bits of ${r64} keep their old value.`;
  }
}

function exampleInstructions(name: string): string[] {
  const info = lookupReg(name)!;
  const imm = info.width === 8 ? "11" : "1";
  return [`mov ${name}, ${imm}`, `add ${name}, 1`, `cmp ${name}, 0`];
}

export interface RegCardOptions {
  /** When provided, clicking a register calls this instead of navigating the
   *  card in place — used by the full-page reference to drive the URL. */
  onNavigate?: (name: string) => void;
}

export function buildRegCard(name: string, opts: RegCardOptions = {}): HTMLElement {
  const card = el("div", "help-card help-card-reg");
  if (!lookupReg(name)) {
    card.appendChild(el("div", "help-title", name));
    return card;
  }

  // `selected` is the navigated register; `active` is what the bit strip and
  // readout currently show (selected, or a hovered relative).
  const render = (selected: string): void => {
    card.innerHTML = "";
    const info = lookupReg(selected)!;

    // Active readout — updates on hover-preview, reverts on leave.
    const readout = el("div", "reg-readout");
    const bitsWrap = el("div", "reg-bits-wrap");
    const setActive = (activeName: string) => {
      const [lo, hi] = bitRangeOf(activeName);
      const ai = lookupReg(activeName)!;
      readout.innerHTML = "";
      readout.appendChild(el("span", "help-mnemonic reg-active-name", activeName));
      readout.appendChild(el("span", "reg-active-meta", `${WIDTH_LABEL[ai.width]} · bits ${lo}–${hi}`));
      bitsWrap.innerHTML = "";
      const strip = bitStrip(ownershipTint(lo, hi));
      const scale = el("div", "reg-bits-scale");
      scale.appendChild(el("span", undefined, "63"));
      scale.appendChild(el("span", undefined, "0"));
      bitsWrap.append(strip, scale);
    };
    card.append(readout, bitsWrap);
    setActive(selected);

    // The family tree, selected node highlighted, every node interactive.
    const tree = el("div", "reg-tree");
    const root = familyTree(selected)!;
    const renderNode = (node: RegNode, depth: number): HTMLElement => {
      const row = el("div", "reg-node-row");
      row.style.paddingLeft = `${depth * 16}px`;
      if (depth > 0) row.appendChild(el("span", "reg-node-branch", "└─"));
      const chip = el("span", `reg-node${node.name === selected ? " selected" : ""}`, node.name);
      chip.tabIndex = 0;
      chip.setAttribute("role", "button");
      chip.title = `${WIDTH_LABEL[node.width]} · bits ${node.bitLo}–${node.bitHi}`;
      const preview = () => setActive(node.name);
      const revert = () => setActive(selected);
      chip.addEventListener("mouseenter", preview);
      chip.addEventListener("focus", preview);
      chip.addEventListener("mouseleave", revert);
      chip.addEventListener("blur", revert);
      chip.addEventListener("click", () => navigate(node.name));
      chip.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          navigate(node.name);
        }
      });
      row.appendChild(chip);
      const wrap = el("div", "reg-node-wrap");
      wrap.appendChild(row);
      for (const child of node.children) wrap.appendChild(renderNode(child, depth + 1));
      return wrap;
    };
    tree.appendChild(renderNode(root, 0));
    card.appendChild(tree);

    // Metadata.
    const meta = el("div", "reg-meta");
    const metaRow = (label: string, value: string) => {
      const row = el("div", "reg-meta-row");
      row.appendChild(el("span", "help-num-label", label));
      row.appendChild(el("span", "reg-meta-value", value));
      meta.appendChild(row);
    };
    metaRow("parent", parentOf(selected) ?? "— (largest)");
    const kids = childrenOf(selected);
    metaRow("children", kids.length ? kids.join(", ") : "— (smallest)");
    const aliases = ancestorsOf(selected);
    metaRow("aliases", aliases.length ? aliases.join(", ") : "—");
    metaRow("role", info.role);
    const saved = el("span", `help-reg-saved help-${info.saved.startsWith("callee") ? "callee" : "caller"}`, info.saved);
    const savedRow = el("div", "reg-meta-row");
    savedRow.appendChild(el("span", "help-num-label", "ABI"));
    savedRow.appendChild(saved);
    meta.appendChild(savedRow);
    card.appendChild(meta);

    // Write behaviour, with a before/after-style effect strip.
    const write = el("div", "reg-write");
    write.appendChild(el("div", "reg-section-title", "when you write it"));
    write.appendChild(el("div", "help-reg-note", writeExplanation(selected)));
    const wstrip = el("div", "reg-bits-wrap");
    wstrip.appendChild(bitStrip(writeTint(selected)));
    write.appendChild(wstrip);
    const legend = el("div", "reg-write-legend");
    legend.appendChild(el("span", "reg-leg reg-leg-written", "written"));
    if (info.width === 32) legend.appendChild(el("span", "reg-leg reg-leg-zeroed", "zeroed"));
    if (info.width < 64) legend.appendChild(el("span", "reg-leg reg-leg-preserved", "preserved"));
    write.appendChild(legend);
    card.appendChild(write);

    // Instruction examples, with machine code filled in lazily.
    const ex = el("div", "reg-examples");
    ex.appendChild(el("div", "reg-section-title", "examples"));
    const examples = exampleInstructions(selected);
    const rows = examples.map((text) => {
      const row = el("div", "reg-example-row");
      row.appendChild(el("span", "reg-example-text", text));
      const bytes = el("span", "reg-example-bytes", "…");
      row.appendChild(bytes);
      ex.appendChild(row);
      return bytes;
    });
    card.appendChild(ex);
    fillExampleBytes(examples, rows);

    const links = el("div", "help-links");
    linkToPlayground(links, `mov ${selected}, 1`);
    // In the popover/embed context, offer a jump to the full-page reference.
    // On the reference page itself (onNavigate set) that would be circular.
    if (!opts.onNavigate) {
      const ref = document.createElement("a");
      ref.className = "help-pg-link";
      ref.textContent = "full register reference →";
      ref.href = `#/registers/${selected}`;
      links.appendChild(ref);
    }
    card.appendChild(links);
  };

  let currentSelected = name.toLowerCase();
  const navigate = (to: string): void => {
    if (opts.onNavigate) {
      opts.onNavigate(to);
      return;
    }
    currentSelected = to;
    render(to);
    card.dispatchEvent(new CustomEvent("help-resize", { bubbles: true }));
  };
  render(currentSelected);
  return card;
}

/** Assemble the example instructions in one request and fill each row's bytes.
 *  Degrades silently to no bytes when the API is unreachable. */
function fillExampleBytes(examples: string[], rows: HTMLElement[]): void {
  assemble({ source: examples.join("\n") })
    .then((res) => {
      res.lines.forEach((line, i) => {
        const cell = rows[i];
        if (cell) cell.textContent = line.hex.replace(/(..)(?=.)/g, "$1 ");
      });
    })
    .catch(() => {
      for (const cell of rows) cell.textContent = "";
    });
}

function linkToPlayground(card: HTMLElement, prefill: string): void {
  const link = document.createElement("a");
  link.className = "help-pg-link";
  link.textContent = "try in the Playground →";
  link.href = "#/playground";
  link.addEventListener("click", () => {
    try {
      sessionStorage.setItem("playground-prefill", prefill);
    } catch {
      /* private mode */
    }
  });
  card.appendChild(link);
}

// ---- instruction card ------------------------------------------------------

/**
 * A reference card for a mnemonic. `context` is the full instruction text (e.g.
 * "mov al, 0x2a") when known, enabling an on-demand byte-encoding lookup.
 */
export function buildInsnCard(mnemonic: string, context?: string): HTMLElement {
  const card = el("div", "help-card help-card-insn");
  const info = lookupInsnEntry(mnemonic);
  if (!info) {
    card.appendChild(el("div", "help-title", mnemonic));
    card.appendChild(el("div", "help-insn-summary", "No reference available."));
    return card;
  }

  const title = el("div", "help-title");
  title.appendChild(el("span", "help-mnemonic", info.mnemonic));
  title.appendChild(el("span", "help-insn-cat", info.category));
  card.appendChild(title);

  card.appendChild(el("div", "help-insn-summary", info.summary));

  // The canonical operand form, so the reader sees the shape at a glance.
  if (info.syntax[0]) {
    const syn = el("div", "help-insn-syntax");
    syn.appendChild(el("code", undefined, info.syntax[0]));
    card.appendChild(syn);
  }

  const flags = el("div", "help-insn-flags");
  flags.appendChild(el("span", "help-num-label", "flags"));
  flags.appendChild(el("span", "help-flags-value", info.flags));
  card.appendChild(flags);

  // A few related mnemonics, each a link into the full reference.
  if (info.related.length) {
    const rel = el("div", "help-insn-related");
    rel.appendChild(el("span", "help-num-label", "related"));
    for (const r of info.related.slice(0, 5)) {
      const a = document.createElement("a");
      a.className = "help-insn-related-chip";
      a.textContent = r;
      a.href = `#/instructions/${r}`;
      rel.appendChild(a);
    }
    card.appendChild(rel);
  }

  // If we know the exact instruction, offer its encoding on demand.
  if (context && /\s/.test(context.trim())) {
    const enc = el("div", "help-insn-enc");
    const btn = el("button", "help-enc-btn", `show encoding of  ${context}`) as HTMLButtonElement;
    btn.addEventListener(
      "click",
      () => {
        btn.disabled = true;
        btn.textContent = "encoding…";
        assembleAndExplain(context)
          .then((line) => {
            enc.innerHTML = "";
            enc.appendChild(line);
          })
          .catch(() => {
            enc.innerHTML = "";
            enc.appendChild(el("div", "help-insn-summary", "could not encode this instruction"));
          });
      },
      { once: true },
    );
    enc.appendChild(btn);
    card.appendChild(enc);
  }

  const links = el("div", "help-links");
  const link = document.createElement("a");
  link.className = "help-pg-link";
  link.textContent = "try in the Playground →";
  link.href = "#/playground";
  link.addEventListener("click", () => {
    try {
      sessionStorage.setItem("playground-prefill", context ?? info.mnemonic);
    } catch {
      /* private mode: fall through, just navigate */
    }
  });
  links.appendChild(link);

  const ref = document.createElement("a");
  ref.className = "help-pg-link";
  ref.textContent = "full instruction reference →";
  ref.href = `#/instructions/${info.mnemonic}`;
  links.appendChild(ref);
  card.appendChild(links);

  return card;
}

async function assembleAndExplain(instruction: string): Promise<HTMLElement> {
  // We only have the text; assemble it to bytes, then explain those bytes.
  const asm = await assemble({ source: instruction });
  const res = await explain({ hex: asm.hex });
  const wrap = el("div", "help-enc-result");
  const bytes = el("div", "help-enc-bytes");
  for (const f of res.fields) {
    const chip = el("span", "help-enc-field", f.hex);
    chip.title = `${f.name}: ${f.explanation}`;
    bytes.appendChild(chip);
  }
  wrap.appendChild(bytes);
  wrap.appendChild(el("div", "help-enc-text", `${res.length} bytes · ${res.text}`));
  return wrap;
}
