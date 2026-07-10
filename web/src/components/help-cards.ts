// The card bodies for the two helpers. Each returns a detached element the
// popover (or an inline embed) drops into place. Kept separate from the popover
// wiring so the same card can appear as a hover/tap popover or as an always-open
// inline block.

import { assemble, explain } from "../api.ts";
import { lookupInsn } from "../core/insninfo.ts";
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
      btn.addEventListener("click", () => render(atWidth(info, w)));
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

// ---- instruction card ------------------------------------------------------

/**
 * A reference card for a mnemonic. `context` is the full instruction text (e.g.
 * "mov al, 0x2a") when known, enabling an on-demand byte-encoding lookup.
 */
export function buildInsnCard(mnemonic: string, context?: string): HTMLElement {
  const card = el("div", "help-card help-card-insn");
  const info = lookupInsn(mnemonic);
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

  const flags = el("div", "help-insn-flags");
  flags.appendChild(el("span", "help-num-label", "flags"));
  flags.appendChild(el("span", "help-flags-value", info.flags));
  card.appendChild(flags);

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
  card.appendChild(link);

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
