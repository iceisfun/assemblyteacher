// <register-view> — the 16 GPRs + rip + flags. Each register expands to reveal
// its 32/16/8-bit sub-registers so the zero-extension rule is legible. Registers
// written on the last step glow. Flags render as individual bits with tooltips.

import { padHex } from "../core/hex.ts";
import type { Flags, Registers } from "../api.ts";

const GPR_ORDER = [
  "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp",
  "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
];

// name of the 32/16/8-bit views for each 64-bit register
const SUBREGS: Record<string, [string, string, string, string]> = {
  rax: ["rax", "eax", "ax", "al"],
  rbx: ["rbx", "ebx", "bx", "bl"],
  rcx: ["rcx", "ecx", "cx", "cl"],
  rdx: ["rdx", "edx", "dx", "dl"],
  rsi: ["rsi", "esi", "si", "sil"],
  rdi: ["rdi", "edi", "di", "dil"],
  rbp: ["rbp", "ebp", "bp", "bpl"],
  rsp: ["rsp", "esp", "sp", "spl"],
  r8: ["r8", "r8d", "r8w", "r8b"],
  r9: ["r9", "r9d", "r9w", "r9b"],
  r10: ["r10", "r10d", "r10w", "r10b"],
  r11: ["r11", "r11d", "r11w", "r11b"],
  r12: ["r12", "r12d", "r12w", "r12b"],
  r13: ["r13", "r13d", "r13w", "r13b"],
  r14: ["r14", "r14d", "r14w", "r14b"],
  r15: ["r15", "r15d", "r15w", "r15b"],
};

const FLAG_INFO: Array<[keyof Flags, string, string]> = [
  ["cf", "CF", "Carry: unsigned overflow / borrow out of the top bit"],
  ["pf", "PF", "Parity: set when the low byte has an even number of 1 bits"],
  ["af", "AF", "Auxiliary carry: carry out of bit 3 (BCD arithmetic)"],
  ["zf", "ZF", "Zero: the result was zero"],
  ["sf", "SF", "Sign: copy of the result's most-significant bit"],
  ["of", "OF", "Overflow: signed overflow occurred"],
];

function toBig(v: number): bigint {
  // Register values are u64. JSON delivers them as numbers; convert defensively.
  return BigInt(Math.trunc(v)) & 0xffff_ffff_ffff_ffffn;
}

// Masks for the four sub-register widths, in [64,32,16,8]-bit order.
const SUB_MASKS = [
  0xffff_ffff_ffff_ffffn,
  0xffff_ffffn,
  0xffffn,
  0xffn,
];

export class RegisterView extends HTMLElement {
  private _registers: Registers = {};
  private _rip = 0n;
  private _flags: Flags | null = null;
  private _written = new Set<string>();
  private _expanded = new Set<string>();
  private built = false;

  connectedCallback(): void {
    if (!this.built) {
      this.built = true;
      this.classList.add("rv");
    }
    this.render();
  }

  /** Set the full register map (name -> u64). `written` glows those names. */
  setState(registers: Registers, rip: number, flags: Flags, written: string[] = []): void {
    this._registers = registers;
    this._rip = toBig(rip);
    this._flags = flags;
    this._written = new Set(written.map((r) => r.toLowerCase()));
    this.render();
  }

  set registers(r: Registers) {
    this._registers = r;
    this.render();
  }
  set flags(f: Flags) {
    this._flags = f;
    this.render();
  }
  set rip(v: number) {
    this._rip = toBig(v);
    this.render();
  }

  private regValue(name: string): bigint {
    const v = this._registers[name];
    return v === undefined ? 0n : toBig(v);
  }

  private render(): void {
    if (!this.built) return;
    this.innerHTML = "";

    const grid = document.createElement("div");
    grid.className = "rv-grid";

    for (const name of GPR_ORDER) {
      const full = this.regValue(name);
      const written = this._written.has(name);
      const expanded = this._expanded.has(name);

      const row = document.createElement("div");
      row.className = "rv-reg";
      if (written) row.classList.add("rv-written");

      const head = document.createElement("button");
      head.className = "rv-head";
      head.setAttribute("aria-expanded", String(expanded));
      head.innerHTML = `<span class="rv-name">${name}</span>` +
        `<span class="rv-val">0x${padHex(full, 16)}</span>` +
        `<span class="rv-caret">${expanded ? "▾" : "▸"}</span>`;
      head.addEventListener("click", () => {
        if (this._expanded.has(name)) this._expanded.delete(name);
        else this._expanded.add(name);
        this.render();
      });
      row.appendChild(head);

      if (expanded) {
        const sub = document.createElement("div");
        sub.className = "rv-sub";
        const views = SUBREGS[name]!;
        const widths = [16, 8, 4, 2];
        views.forEach((view, i) => {
          const val = full & SUB_MASKS[i]!;
          const cell = document.createElement("div");
          cell.className = "rv-subcell";
          cell.innerHTML =
            `<span class="rv-subname">${view}</span>` +
            `<span class="rv-subval">0x${padHex(val, widths[i]!)}</span>`;
          sub.appendChild(cell);
        });
        const note = document.createElement("div");
        note.className = "rv-note";
        note.textContent = "writing the 32-bit view zeroes the upper 32 bits";
        sub.appendChild(note);
        row.appendChild(sub);
      }

      grid.appendChild(row);
    }

    // rip
    const ripRow = document.createElement("div");
    ripRow.className = "rv-reg rv-rip";
    if (this._written.has("rip")) ripRow.classList.add("rv-written");
    ripRow.innerHTML =
      `<div class="rv-head"><span class="rv-name">rip</span>` +
      `<span class="rv-val">0x${padHex(this._rip, 16)}</span></div>`;
    grid.appendChild(ripRow);

    this.appendChild(grid);

    // flags
    if (this._flags) {
      const flagsEl = document.createElement("div");
      flagsEl.className = "rv-flags";
      for (const [key, label, desc] of FLAG_INFO) {
        const on = this._flags[key];
        const bit = document.createElement("span");
        bit.className = "rv-flag" + (on ? " rv-flag-on" : "");
        bit.textContent = `${label}:${on ? 1 : 0}`;
        bit.title = desc;
        flagsEl.appendChild(bit);
      }
      // DF is not in the emulator flag set but is part of the teaching set;
      // show it as informational when absent.
      const df = document.createElement("span");
      df.className = "rv-flag rv-flag-df";
      df.textContent = "DF:?";
      df.title = "Direction: controls whether string ops step forward (0) or back (1)";
      flagsEl.appendChild(df);
      this.appendChild(flagsEl);
    }
  }
}

customElements.define("register-view", RegisterView);
