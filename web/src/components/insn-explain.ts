// <insn-explain> — renders POST /api/asm/explain. The instruction's bytes are
// coloured chips, one hue per field (prefix / REX / opcode / ModRM / SIB /
// displacement / immediate). Each field card carries the server's explanation,
// and ModRM/SIB additionally render a labelled bit ruler splitting the byte into
// its mod/reg/rm (or scale/index/base) fields. This is the project's single most
// important teaching visual, so it is built to be scrubbed over slowly.

import type { ExplainField, ExplainResponse } from "../api.ts";
import { parseHex } from "../core/hex.ts";
import { tokenizeCodeToHtml } from "../core/asm-tokens.ts";

type FieldKind =
  | "prefix"
  | "rex"
  | "opcode"
  | "modrm"
  | "sib"
  | "disp"
  | "imm"
  | "other";

function fieldKind(name: string): FieldKind {
  const n = name.toLowerCase();
  if (n.includes("prefix")) return "prefix";
  if (n === "rex") return "rex";
  if (n === "opcode") return "opcode";
  if (n === "modrm") return "modrm";
  if (n === "sib") return "sib";
  if (n.startsWith("disp")) return "disp";
  if (n.startsWith("imm")) return "imm";
  return "other";
}

interface BitGroup {
  label: string;
  bits: number; // count
  value: number;
}

// Split a ModRM byte into (mod, reg, rm) and a SIB byte into
// (scale, index, base). The high bits come first.
function modrmGroups(byte: number): BitGroup[] {
  return [
    { label: "mod", bits: 2, value: byte >> 6 },
    { label: "reg", bits: 3, value: (byte >> 3) & 7 },
    { label: "rm", bits: 3, value: byte & 7 },
  ];
}
function sibGroups(byte: number): BitGroup[] {
  return [
    { label: "scale", bits: 2, value: byte >> 6 },
    { label: "index", bits: 3, value: (byte >> 3) & 7 },
    { label: "base", bits: 3, value: byte & 7 },
  ];
}

export class InsnExplain extends HTMLElement {
  private data: ExplainResponse | null = null;
  private error: string | null = null;
  private built = false;

  connectedCallback(): void {
    if (!this.built) {
      this.built = true;
      this.classList.add("ie");
    }
    this.render();
  }

  /** Provide an explanation response (fetched by the caller). */
  setData(data: ExplainResponse): void {
    this.data = data;
    this.error = null;
    this.render();
  }

  setError(message: string): void {
    this.error = message;
    this.data = null;
    this.render();
  }

  clear(): void {
    this.data = null;
    this.error = null;
    this.render();
  }

  private allBytes(): number[] {
    if (!this.data) return [];
    const out: number[] = [];
    for (const f of this.data.fields) {
      const b = parseHex(f.hex);
      if (b) out.push(...b);
    }
    return out;
  }

  private render(): void {
    if (!this.built) return;
    this.innerHTML = "";
    if (this.error) {
      const e = document.createElement("div");
      e.className = "ie-error";
      e.textContent = this.error;
      this.appendChild(e);
      return;
    }
    if (!this.data) {
      const e = document.createElement("div");
      e.className = "ie-empty";
      e.textContent = "Select an instruction to see its byte-by-byte breakdown.";
      this.appendChild(e);
      return;
    }

    // ---- header: the disassembly text and total length ----
    const header = document.createElement("div");
    header.className = "ie-header";
    // The disassembly text gets interactive mnemonic/register/number tokens;
    // the full instruction is passed as context so a mnemonic card can offer
    // this exact instruction's encoding.
    header.innerHTML =
      `<span class="ie-text">${tokenizeCodeToHtml(escapeHtml(this.data.text), this.data.text)}</span>` +
      `<span class="ie-len">${this.data.length} bytes</span>`;
    this.appendChild(header);

    // ---- the full byte strip, coloured by field ----
    const strip = document.createElement("div");
    strip.className = "ie-strip";
    let byteIndex = 0;
    this.data.fields.forEach((field, fi) => {
      const bytes = parseHex(field.hex) ?? new Uint8Array(0);
      const kind = fieldKind(field.name);
      for (const b of bytes) {
        const chip = document.createElement("span");
        chip.className = `ie-byte ie-k-${kind}`;
        chip.dataset.field = String(fi);
        chip.dataset.pos = String(byteIndex);
        chip.textContent = b.toString(16).padStart(2, "0");
        strip.appendChild(chip);
        byteIndex++;
      }
    });
    this.appendChild(strip);

    // ---- one card per field ----
    const cards = document.createElement("div");
    cards.className = "ie-cards";
    this.data.fields.forEach((field, fi) => {
      cards.appendChild(this.fieldCard(field, fi));
    });
    this.appendChild(cards);

    // hover wiring: hovering a byte chip or a card highlights both.
    this.wireHighlight();
  }

  private fieldCard(field: ExplainField, fi: number): HTMLElement {
    const kind = fieldKind(field.name);
    const card = document.createElement("div");
    card.className = `ie-card ie-k-${kind}`;
    card.dataset.field = String(fi);

    const title = document.createElement("div");
    title.className = "ie-card-title";
    title.innerHTML =
      `<span class="ie-swatch ie-k-${kind}"></span>` +
      `<span class="ie-fname">${escapeHtml(field.name)}</span>` +
      `<span class="ie-fhex">${escapeHtml(field.hex)}</span>` +
      `<span class="ie-foff">@${field.offset}</span>`;
    card.appendChild(title);

    const expl = document.createElement("div");
    expl.className = "ie-expl";
    expl.textContent = field.explanation;
    card.appendChild(expl);

    // ModRM / SIB bit ruler
    const bytes = parseHex(field.hex);
    if (bytes && bytes.length === 1 && (kind === "modrm" || kind === "sib")) {
      const groups = kind === "modrm" ? modrmGroups(bytes[0]!) : sibGroups(bytes[0]!);
      card.appendChild(this.bitRuler(bytes[0]!, groups));
    }

    return card;
  }

  private bitRuler(byte: number, groups: BitGroup[]): HTMLElement {
    const wrap = document.createElement("div");
    wrap.className = "ie-ruler";

    const bitsRow = document.createElement("div");
    bitsRow.className = "ie-bits";
    const labelsRow = document.createElement("div");
    labelsRow.className = "ie-bitlabels";

    let bitPos = 7;
    groups.forEach((g, gi) => {
      const groupEl = document.createElement("span");
      groupEl.className = `ie-bitgroup ie-bg-${gi}`;
      for (let k = 0; k < g.bits; k++) {
        const bit = (byte >> bitPos) & 1;
        bitPos--;
        const cell = document.createElement("span");
        cell.className = "ie-bit";
        cell.textContent = String(bit);
        groupEl.appendChild(cell);
      }
      bitsRow.appendChild(groupEl);

      const lbl = document.createElement("span");
      lbl.className = `ie-bitlabel ie-bg-${gi}`;
      lbl.style.setProperty("--w", String(g.bits));
      const dec = g.value;
      const bin = g.value.toString(2).padStart(g.bits, "0");
      lbl.innerHTML = `<b>${g.label}</b> ${bin}₂ = ${dec}`;
      labelsRow.appendChild(lbl);
    });

    wrap.append(bitsRow, labelsRow);
    return wrap;
  }

  private wireHighlight(): void {
    const chips = Array.from(this.querySelectorAll<HTMLElement>(".ie-byte"));
    const cards = Array.from(this.querySelectorAll<HTMLElement>(".ie-card"));
    const apply = (fi: string | null) => {
      for (const c of chips) c.classList.toggle("ie-hot", c.dataset.field === fi);
      for (const c of cards) c.classList.toggle("ie-hot", c.dataset.field === fi);
    };
    // Hover cross-highlights the chip/card pair (progressive enhancement); tap
    // PINS it so the correspondence is reachable without a pointer. Hover falls
    // back to the pinned field when the pointer leaves.
    let pinned: string | null = null;
    const setActive = (fi: string | null) => apply(fi ?? pinned);
    for (const el of [...chips, ...cards]) {
      el.addEventListener("mouseenter", () => setActive(el.dataset.field ?? null));
      el.addEventListener("mouseleave", () => setActive(null));
      el.addEventListener("click", () => {
        const fi = el.dataset.field ?? null;
        pinned = pinned === fi ? null : fi;
        setActive(null);
      });
    }
  }
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

customElements.define("insn-explain", InsnExplain);
