// <stack-view> — the stack drawn downward-growing, one row per qword, with rsp
// and rbp markers, a best-effort meaning for each slot, and frame delimiters
// derived from the saved-rbp chain.

import { padHex } from "../core/hex.ts";
import { readUnsigned } from "../core/endian.ts";

export interface StackModel {
  /** Bytes of the memory image that contains the stack. */
  bytes: Uint8Array;
  /** Virtual address of bytes[0]. */
  base: bigint;
  rsp: bigint;
  rbp: bigint;
  /** Range treated as executable code, for return-address heuristics. */
  codeRange?: { start: bigint; end: bigint };
}

export class StackView extends HTMLElement {
  private model: StackModel | null = null;
  private rowsAbove = 2; // qwords to show above rsp
  private rowsBelow = 24; // qwords to show below rsp
  private built = false;

  connectedCallback(): void {
    if (!this.built) {
      this.built = true;
      this.classList.add("sv");
    }
    this.render();
  }

  setModel(model: StackModel): void {
    this.model = model;
    this.render();
  }

  private inCode(v: bigint): boolean {
    const r = this.model?.codeRange;
    return !!r && v >= r.start && v < r.end;
  }

  private qwordAt(addr: bigint): bigint | null {
    if (!this.model) return null;
    const off = Number(addr - this.model.base);
    if (off < 0 || off + 8 > this.model.bytes.length) return null;
    return readUnsigned(this.model.bytes, off, 8, "little");
  }

  private render(): void {
    if (!this.built) return;
    this.innerHTML = "";
    if (!this.model) {
      this.textContent = "no stack";
      return;
    }
    const { rsp, rbp } = this.model;

    const list = document.createElement("div");
    list.className = "sv-list";

    // Follow the saved-rbp chain to know which addresses are frame bases.
    const frameBases = new Set<bigint>();
    let fp = rbp;
    for (let i = 0; i < 32; i++) {
      frameBases.add(fp);
      const saved = this.qwordAt(fp);
      if (saved === null || saved <= fp) break;
      fp = saved;
    }

    const top = rsp - BigInt(this.rowsAbove * 8);
    const count = this.rowsAbove + this.rowsBelow;
    for (let i = 0; i < count; i++) {
      const addr = top + BigInt(i * 8);
      const val = this.qwordAt(addr);
      const row = document.createElement("div");
      row.className = "sv-row";

      const markers: string[] = [];
      if (addr === rsp) markers.push("rsp");
      if (addr === rbp) markers.push("rbp");
      const marker = document.createElement("span");
      marker.className = "sv-marker";
      marker.textContent = markers.join("/");
      if (addr === rsp) row.classList.add("sv-rsp");
      if (addr === rbp) row.classList.add("sv-rbp");
      if (frameBases.has(addr)) row.classList.add("sv-frame");

      const addrEl = document.createElement("span");
      addrEl.className = "sv-addr";
      addrEl.textContent = "0x" + padHex(addr, 16);

      const valEl = document.createElement("span");
      valEl.className = "sv-val";
      valEl.textContent = val === null ? "····" : "0x" + padHex(val, 16);

      const meaning = document.createElement("span");
      meaning.className = "sv-meaning";
      meaning.textContent = this.guess(addr, val, frameBases);

      row.append(marker, addrEl, valEl, meaning);
      list.appendChild(row);
    }

    this.appendChild(list);
  }

  private guess(addr: bigint, val: bigint | null, frameBases: Set<bigint>): string {
    if (val === null) return "";
    const model = this.model!;
    if (frameBases.has(addr)) return "saved rbp (frame link)";
    if (this.inCode(val)) return "possible return address →";
    if (addr < model.rsp) return "below rsp (unallocated)";
    if (val >= model.base && val < model.base + BigInt(model.bytes.length)) {
      return "pointer into memory";
    }
    if (val === 0n) return "zero";
    return "";
  }
}

customElements.define("stack-view", StackView);
