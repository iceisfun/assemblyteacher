// <memory-viewer> — a synchronised, virtualised hex + ASCII dump for teaching
// and CTF-style memory inspection.
//
// Rendering strategy (virtualisation): the scroll container holds a single
// spacer div whose height is totalRows * ROW_H, giving a real scrollbar for the
// whole buffer without building a row per byte. Only the rows intersecting the
// viewport (plus a small overscan) are materialised as absolutely-positioned
// nodes and rebuilt on scroll. A 16 MiB buffer is ~1M rows but only ~40 nodes
// ever exist, so scrolling stays smooth. Per-byte state (regions, selection,
// dirty flags, search hits, cursor) is looked up while building each visible
// row, so there is no separate 1M-entry DOM to keep in sync.

import {
  asciiChar,
  byteHex,
  formatAddress,
  isPrintable,
} from "../core/hex.ts";
import {
  buildNeedle,
  findAll,
  findNext,
  findPrev,
} from "../core/search.ts";
import {
  interpret,
  readUnsigned,
  type Endianness,
} from "../core/endian.ts";
import { diffBytes } from "../core/diff.ts";

const ROW_H = 20; // px, must match .mv-row height in style.css
const OVERSCAN = 6; // rows rendered beyond the viewport on each side

export interface Region {
  start: number; // byte offset, inclusive
  end: number; // byte offset, exclusive
  color: string; // any CSS color
  label: string;
}

export interface Annotation {
  addr: number | bigint; // absolute virtual address
  size: number; // bytes covered
  label: string;
  type?: string;
}

export interface Bookmark {
  name: string;
  addr: bigint;
}

interface Selection {
  anchor: number;
  focus: number;
}

function selRange(s: Selection): [number, number] {
  return s.anchor <= s.focus ? [s.anchor, s.focus] : [s.focus, s.anchor];
}

export class MemoryViewer extends HTMLElement {
  private _bytes: Uint8Array = new Uint8Array(0);
  private _base = 0n;
  private _bpr = 16;
  // When true, the row width tracks the viewport: 16 bytes on desktop, 8 on a
  // phone (where a 16-byte row would need horizontal scrolling in a tiny box).
  // A caller that sets bytesPerRow explicitly opts out of this.
  private _bprAuto = true;
  private mq: MediaQueryList | null = null;
  private onMqChange = (): void => this.applyAutoBpr();
  private _endianness: Endianness = "little";
  private _regions: Region[] = [];
  private _annotations: Annotation[] = [];
  private _dirty = new Set<number>();
  private _searchHits: number[] = [];
  private _currentHit = -1;
  private _selection: Selection | null = null;
  private _dragging = false;
  private _cursor = 0;
  private _activePointer: { from: number; to: number } | null = null;
  private _bookmarks: Bookmark[] = [];
  private _addrDigits = 8;

  // DOM handles
  private scrollEl!: HTMLDivElement;
  private spacerEl!: HTMLDivElement;
  private rowsEl!: HTMLDivElement;
  private overlayEl!: SVGSVGElement;
  private statusEl!: HTMLDivElement;
  private legendEl!: HTMLDivElement;
  private bookmarksEl!: HTMLDivElement;
  private tooltipEl!: HTMLDivElement;
  private searchInput!: HTMLInputElement;
  private searchMode: "hex" | "ascii" = "hex";
  private built = false;

  connectedCallback(): void {
    if (!this.built) this.build();
    this.render();
  }

  disconnectedCallback(): void {
    this.mq?.removeEventListener("change", this.onMqChange);
  }

  /** Pick a viewport-appropriate row width unless a caller pinned one. */
  private applyAutoBpr(render = true): void {
    if (!this._bprAuto) return;
    const target = this.mq?.matches ? 8 : 16;
    if (target !== this._bpr) {
      this._bpr = target;
      if (render) this.render();
    }
  }

  // ----- public properties -------------------------------------------------

  get bytes(): Uint8Array {
    return this._bytes;
  }
  set bytes(next: Uint8Array) {
    this._bytes = next;
    this._dirty.clear();
    this.recomputeAddrDigits();
    this.refreshSearch();
    this.render();
  }

  get base(): bigint {
    return this._base;
  }
  set base(v: number | bigint) {
    this._base = BigInt(v);
    this.recomputeAddrDigits();
    this.render();
  }

  get bytesPerRow(): number {
    return this._bpr;
  }
  set bytesPerRow(v: number) {
    this._bprAuto = false;
    this._bpr = Math.max(1, v | 0);
    this.render();
  }

  get endianness(): Endianness {
    return this._endianness;
  }
  set endianness(v: Endianness) {
    this._endianness = v;
    this.render();
  }

  get regions(): Region[] {
    return this._regions;
  }
  set regions(v: Region[]) {
    this._regions = v.slice();
    this.renderLegend();
    this.render();
  }

  get annotations(): Annotation[] {
    return this._annotations;
  }
  set annotations(v: Annotation[]) {
    this._annotations = v.slice();
    this.render();
  }

  get bookmarks(): Bookmark[] {
    return this._bookmarks;
  }

  /**
   * Replace the buffer, diff against the previous one, flash the changed bytes
   * and keep them dirty-tinted until clearDirty(). This is what an execution
   * scrubber calls on every step so memory writes light up.
   */
  setBytes(next: Uint8Array): void {
    const changed = diffBytes(this._bytes, next);
    this._bytes = next;
    this._dirty = changed;
    this.recomputeAddrDigits();
    this.refreshSearch();
    this.render();
  }

  clearDirty(): void {
    this._dirty.clear();
    this.render();
  }

  // ----- navigation ---------------------------------------------------------

  offsetOf(addr: bigint): number {
    return Number(addr - this._base);
  }
  addrOf(offset: number): bigint {
    return this._base + BigInt(offset);
  }

  scrollToOffset(offset: number, highlight = true): void {
    const row = Math.floor(offset / this._bpr);
    const target = row * ROW_H - this.scrollEl.clientHeight / 2 + ROW_H;
    this.scrollEl.scrollTop = Math.max(0, target);
    if (highlight) {
      this._cursor = Math.max(0, Math.min(offset, this._bytes.length - 1));
    }
    this.render();
  }

  scrollToAddress(addr: bigint): void {
    this.scrollToOffset(this.offsetOf(addr));
  }

  addBookmark(name: string, addr: bigint): void {
    this._bookmarks.push({ name, addr });
    this.renderBookmarks();
  }

  // ----- construction -------------------------------------------------------

  private build(): void {
    this.built = true;
    this.classList.add("mv");

    // Track the phone breakpoint so rows narrow to 8 bytes there. Set before the
    // first render so the initial paint already uses the right width.
    if (typeof window !== "undefined" && window.matchMedia) {
      this.mq = window.matchMedia("(max-width: 600px)");
      this.mq.addEventListener("change", this.onMqChange);
      this.applyAutoBpr(false);
    }
    this.innerHTML = `
      <div class="mv-toolbar">
        <div class="mv-search">
          <select class="mv-search-mode" aria-label="Search mode">
            <option value="hex">hex</option>
            <option value="ascii">ascii</option>
          </select>
          <input class="mv-search-input" type="text" placeholder="search…"
                 aria-label="Search bytes" />
          <button class="mv-btn mv-prev" title="Previous match (Shift+Enter)">◀</button>
          <span class="mv-search-count" aria-live="polite"></span>
          <button class="mv-btn mv-next" title="Next match (Enter)">▶</button>
        </div>
        <div class="mv-bmadd">
          <button class="mv-btn mv-bookmark-add" title="Bookmark the cursor">☆ bookmark</button>
        </div>
      </div>
      <div class="mv-legend" aria-label="Region legend"></div>
      <div class="mv-bookmarks" aria-label="Bookmarks"></div>
      <div class="mv-scroll" tabindex="0" role="grid" aria-label="Memory hex dump">
        <div class="mv-spacer">
          <svg class="mv-overlay" aria-hidden="true"></svg>
          <div class="mv-rows"></div>
        </div>
      </div>
      <div class="mv-status" role="status" aria-live="polite"></div>
      <div class="mv-tooltip" role="tooltip" hidden></div>
    `;

    this.scrollEl = this.querySelector(".mv-scroll")!;
    this.spacerEl = this.querySelector(".mv-spacer")!;
    this.rowsEl = this.querySelector(".mv-rows")!;
    this.overlayEl = this.querySelector(".mv-overlay")!;
    this.statusEl = this.querySelector(".mv-status")!;
    this.legendEl = this.querySelector(".mv-legend")!;
    this.bookmarksEl = this.querySelector(".mv-bookmarks")!;
    this.tooltipEl = this.querySelector(".mv-tooltip")!;
    this.searchInput = this.querySelector(".mv-search-input")!;

    this.scrollEl.addEventListener("scroll", () => this.renderRows());
    this.scrollEl.addEventListener("keydown", (e) => this.onKeyDown(e));
    this.rowsEl.addEventListener("mousedown", (e) => this.onMouseDown(e));
    this.rowsEl.addEventListener("mousemove", (e) => this.onMouseMove(e));
    this.rowsEl.addEventListener("mouseleave", () => this.hideTooltip());
    window.addEventListener("mouseup", () => this.onMouseUp());
    this.rowsEl.addEventListener("click", (e) => this.onClick(e));

    this.searchInput.addEventListener("input", () => this.refreshSearch());
    this.searchInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        e.shiftKey ? this.gotoPrevMatch() : this.gotoNextMatch();
      }
    });
    this.querySelector(".mv-search-mode")!.addEventListener("change", (e) => {
      this.searchMode = (e.target as HTMLSelectElement).value as "hex" | "ascii";
      this.refreshSearch();
    });
    this.querySelector(".mv-next")!.addEventListener("click", () =>
      this.gotoNextMatch(),
    );
    this.querySelector(".mv-prev")!.addEventListener("click", () =>
      this.gotoPrevMatch(),
    );
    this.querySelector(".mv-bookmark-add")!.addEventListener("click", () => {
      const name = prompt(
        "Bookmark name for " + this.formatAddr(this.addrOf(this._cursor)),
      );
      if (name) this.addBookmark(name, this.addrOf(this._cursor));
    });
  }

  private recomputeAddrDigits(): void {
    const top = this._base + BigInt(Math.max(0, this._bytes.length - 1));
    this._addrDigits = Math.max(8, top.toString(16).length);
  }

  private formatAddr(addr: bigint): string {
    return "0x" + formatAddress(addr, this._addrDigits);
  }

  // ----- rendering ----------------------------------------------------------

  private render(): void {
    if (!this.built) return;
    const rows = Math.ceil(this._bytes.length / this._bpr) || 1;
    this.spacerEl.style.height = rows * ROW_H + "px";
    this.renderRows();
    this.renderStatus();
  }

  private renderLegend(): void {
    if (!this.built) return;
    this.legendEl.innerHTML = "";
    for (const r of this._regions) {
      const chip = document.createElement("span");
      chip.className = "mv-legend-chip";
      const sw = document.createElement("span");
      sw.className = "mv-swatch";
      sw.style.background = r.color;
      chip.append(sw, document.createTextNode(r.label));
      chip.title = `${this.formatAddr(this.addrOf(r.start))} – ${this.formatAddr(
        this.addrOf(r.end),
      )}`;
      chip.addEventListener("click", () => this.scrollToOffset(r.start));
      this.legendEl.appendChild(chip);
    }
    this.legendEl.hidden = this._regions.length === 0;
  }

  private renderBookmarks(): void {
    if (!this.built) return;
    this.bookmarksEl.innerHTML = "";
    for (const b of this._bookmarks) {
      const chip = document.createElement("button");
      chip.className = "mv-btn mv-bm";
      chip.textContent = `${b.name} @ ${this.formatAddr(b.addr)}`;
      chip.addEventListener("click", () => this.scrollToAddress(b.addr));
      this.bookmarksEl.appendChild(chip);
    }
    this.bookmarksEl.hidden = this._bookmarks.length === 0;
  }

  private regionAt(offset: number): Region | null {
    for (const r of this._regions) {
      if (offset >= r.start && offset < r.end) return r;
    }
    return null;
  }

  private renderRows(): void {
    if (!this.built) return;
    const total = this._bytes.length;
    const totalRows = Math.ceil(total / this._bpr) || 1;
    const scrollTop = this.scrollEl.scrollTop;
    const viewH = this.scrollEl.clientHeight || ROW_H * 24;
    const first = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN);
    const last = Math.min(
      totalRows,
      Math.ceil((scrollTop + viewH) / ROW_H) + OVERSCAN,
    );

    const sel = this._selection ? selRange(this._selection) : null;
    const frag = document.createDocumentFragment();

    for (let row = first; row < last; row++) {
      const rowStart = row * this._bpr;
      const rowEl = document.createElement("div");
      rowEl.className = "mv-row";
      rowEl.style.top = row * ROW_H + "px";
      rowEl.setAttribute("role", "row");

      const addrEl = document.createElement("span");
      addrEl.className = "mv-addr";
      addrEl.textContent = this.formatAddr(this.addrOf(rowStart));
      rowEl.appendChild(addrEl);

      const hexEl = document.createElement("span");
      hexEl.className = "mv-hex";
      const asciiEl = document.createElement("span");
      asciiEl.className = "mv-ascii";

      const half = this._bpr >> 1;
      for (let i = 0; i < this._bpr; i++) {
        const off = rowStart + i;
        if (i === half && half > 0) {
          const gap = document.createElement("span");
          gap.className = "mv-gap";
          gap.textContent = " ";
          hexEl.appendChild(gap);
        }
        if (off >= total) {
          const pad = document.createElement("span");
          pad.className = "mv-cell mv-pad";
          pad.textContent = "  ";
          hexEl.appendChild(pad);
          continue;
        }
        const b = this._bytes[off]!;
        const cell = document.createElement("span");
        cell.className = "mv-cell";
        cell.dataset.off = String(off);
        cell.textContent = byteHex(b);
        this.decorate(cell, off, sel);
        hexEl.appendChild(cell);

        const ac = document.createElement("span");
        ac.className = "mv-achar";
        ac.dataset.off = String(off);
        ac.textContent = asciiChar(b);
        if (!isPrintable(b)) ac.classList.add("mv-nonprint");
        this.decorate(ac, off, sel);
        asciiEl.appendChild(ac);
      }

      rowEl.append(hexEl, asciiEl);

      // annotation underlines beneath the hex columns + a label per annotation
      // that begins on this row (kept off the fixed-height grid so virtualised
      // row positions stay exact).
      const { bars, labels } = this.annotationsForRow(rowStart);
      if (bars.length > 0) {
        rowEl.classList.add("mv-has-ann");
        for (const bar of bars) hexEl.appendChild(bar);
      }
      if (labels.length > 0) {
        const labelWrap = document.createElement("span");
        labelWrap.className = "mv-annlabels";
        for (const l of labels) labelWrap.appendChild(l);
        rowEl.appendChild(labelWrap);
      }

      frag.appendChild(rowEl);
    }

    this.rowsEl.replaceChildren(frag);
    this.drawPointer();
  }

  private decorate(el: HTMLElement, off: number, sel: [number, number] | null): void {
    const region = this.regionAt(off);
    if (region) {
      el.style.background = region.color;
      el.classList.add("mv-in-region");
    }
    if (this._dirty.has(off)) el.classList.add("mv-dirty", "mv-flash");
    if (sel && off >= sel[0] && off <= sel[1]) el.classList.add("mv-sel");
    if (off === this._cursor) el.classList.add("mv-cursor");
    if (this._searchHits.length) {
      const nlen = this.currentNeedleLen();
      for (let h = 0; h < this._searchHits.length; h++) {
        const s = this._searchHits[h]!;
        if (off >= s && off < s + nlen) {
          el.classList.add("mv-match");
          if (h === this._currentHit) el.classList.add("mv-match-cur");
          break;
        }
      }
    }
  }

  private annotationsForRow(
    rowStart: number,
  ): { bars: HTMLElement[]; labels: HTMLElement[] } {
    const rowEnd = rowStart + this._bpr;
    const bars: HTMLElement[] = [];
    const labels: HTMLElement[] = [];
    const half = this._bpr >> 1;
    for (const a of this._annotations) {
      const aOff = this.offsetOf(BigInt(a.addr));
      const aEnd = aOff + a.size;
      const from = Math.max(aOff, rowStart);
      const to = Math.min(aEnd, rowEnd);
      if (from >= to) continue;
      const col = from - rowStart;
      const span = to - from;
      // Each hex column is 3ch (2 digits + 1 gap); a wider gap sits after the
      // halfway mark. Convert byte columns to ch offsets to line the underline
      // up under the exact bytes it covers.
      const left = col * 3 + (half > 0 && col >= half ? 1 : 0);
      const width = span * 3 - 1 + (half > 0 && col < half && col + span > half ? 1 : 0);
      const bar = document.createElement("span");
      bar.className = "mv-ann";
      if (a.type) bar.dataset.annType = a.type;
      bar.style.left = `${left}ch`;
      bar.style.width = `${width}ch`;
      bar.title = `${a.label}${a.type ? " : " + a.type : ""} @ ${this.formatAddr(
        this.addrOf(aOff),
      )} (${a.size} bytes)`;
      bars.push(bar);

      if (aOff >= rowStart && aOff < rowEnd) {
        const label = document.createElement("span");
        label.className = "mv-annlabel";
        if (a.type) label.dataset.annType = a.type;
        label.textContent = a.type ? `${a.label}:${a.type}` : a.label;
        labels.push(label);
      }
    }
    return { bars, labels };
  }

  // ----- pointer visualisation ---------------------------------------------

  private pointerTargetOf(offset: number): number | null {
    if (offset + 8 > this._bytes.length) return null;
    const value = readUnsigned(this._bytes, offset, 8, this._endianness);
    const off = value - this._base;
    if (off < 0n || off >= BigInt(this._bytes.length)) return null;
    return Number(off);
  }

  private drawPointer(): void {
    this.overlayEl.innerHTML = "";
    const ptr = this._activePointer;
    if (!ptr) return;
    const fromCell = this.cellFor(ptr.from);
    const toCell = this.cellFor(ptr.to);
    if (!fromCell && !toCell) return;
    const box = this.spacerEl.getBoundingClientRect();
    const point = (cell: HTMLElement | null, off: number): [number, number] => {
      if (cell) {
        const r = cell.getBoundingClientRect();
        return [r.left - box.left + r.width / 2, r.top - box.top + r.height / 2];
      }
      // off-screen: clamp to its row's y at the left edge
      const row = Math.floor(off / this._bpr);
      return [40, row * ROW_H + ROW_H / 2];
    };
    const [x1, y1] = point(fromCell, ptr.from);
    const [x2, y2] = point(toCell, ptr.to);
    const svgns = "http://www.w3.org/2000/svg";
    const path = document.createElementNS(svgns, "path");
    const midx = Math.max(x1, x2) + 40;
    path.setAttribute(
      "d",
      `M ${x1} ${y1} C ${midx} ${y1}, ${midx} ${y2}, ${x2} ${y2}`,
    );
    path.setAttribute("class", "mv-arrow");
    const dot = document.createElementNS(svgns, "circle");
    dot.setAttribute("cx", String(x2));
    dot.setAttribute("cy", String(y2));
    dot.setAttribute("r", "3");
    dot.setAttribute("class", "mv-arrow-head");
    this.overlayEl.appendChild(path);
    this.overlayEl.appendChild(dot);
  }

  private cellFor(off: number): HTMLElement | null {
    return this.rowsEl.querySelector(`.mv-cell[data-off="${off}"]`);
  }

  // ----- interaction --------------------------------------------------------

  private offsetFromEvent(e: MouseEvent): number | null {
    const el = (e.target as HTMLElement).closest("[data-off]");
    if (!el) return null;
    const raw = (el as HTMLElement).dataset.off;
    return raw ? Number(raw) : null;
  }

  private onMouseDown(e: MouseEvent): void {
    const off = this.offsetFromEvent(e);
    if (off === null) return;
    this._dragging = true;
    this._selection = { anchor: off, focus: off };
    this._cursor = off;
    this.renderRows();
    this.renderStatus();
  }

  private onMouseMove(e: MouseEvent): void {
    const off = this.offsetFromEvent(e);
    if (off === null) return;
    if (this._dragging && this._selection) {
      this._selection.focus = off;
      this._cursor = off;
      this.renderRows();
      this.renderStatus();
    }
    this.showTooltip(e, off);
  }

  private onMouseUp(): void {
    if (this._dragging) {
      this._dragging = false;
      this.renderStatus();
    }
  }

  private onClick(e: MouseEvent): void {
    const off = this.offsetFromEvent(e);
    if (off === null) return;
    const target = this.pointerTargetOf(off);
    if (target !== null) {
      this._activePointer = { from: off, to: target };
      this._cursor = target;
      this.scrollToOffset(target, false);
      this.renderRows();
      this.renderStatus();
    } else {
      this._activePointer = null;
      this.drawPointer();
    }
  }

  private onKeyDown(e: KeyboardEvent): void {
    const bpr = this._bpr;
    let next = this._cursor;
    const rowsPerPage = Math.max(1, Math.floor(this.scrollEl.clientHeight / ROW_H) - 1);
    switch (e.key) {
      case "ArrowLeft": next -= 1; break;
      case "ArrowRight": next += 1; break;
      case "ArrowUp": next -= bpr; break;
      case "ArrowDown": next += bpr; break;
      case "PageUp": next -= bpr * rowsPerPage; break;
      case "PageDown": next += bpr * rowsPerPage; break;
      case "Home": next = 0; break;
      case "End": next = this._bytes.length - 1; break;
      case "g":
      case "G": {
        const ans = prompt("Go to address (hex ok, e.g. 0x401000):");
        if (ans) {
          const parsed = ans.trim().toLowerCase().startsWith("0x")
            ? BigInt(ans.trim())
            : BigInt("0x" + ans.trim().replace(/^0x/, ""));
          this.scrollToAddress(parsed);
        }
        e.preventDefault();
        return;
      }
      default:
        return;
    }
    e.preventDefault();
    this._cursor = Math.max(0, Math.min(next, this._bytes.length - 1));
    this.ensureCursorVisible();
    this.renderRows();
    this.renderStatus();
  }

  private ensureCursorVisible(): void {
    const row = Math.floor(this._cursor / this._bpr);
    const y = row * ROW_H;
    const top = this.scrollEl.scrollTop;
    const bottom = top + this.scrollEl.clientHeight - ROW_H;
    if (y < top) this.scrollEl.scrollTop = y;
    else if (y > bottom) this.scrollEl.scrollTop = y - this.scrollEl.clientHeight + ROW_H;
  }

  // ----- tooltip ------------------------------------------------------------

  private showTooltip(e: MouseEvent, off: number): void {
    if (off >= this._bytes.length) return this.hideTooltip();
    const b = this._bytes[off]!;
    const words = interpret(this._bytes, off, this._endianness);
    const lines = [
      `offset ${off} · ${this.formatAddr(this.addrOf(off))}`,
      `dec ${b} · hex 0x${byteHex(b)} · char '${asciiChar(b)}'`,
    ];
    if (words.u16 !== undefined) lines.push(`u16 0x${words.u16.toString(16)}`);
    if (words.u32 !== undefined) lines.push(`u32 0x${words.u32.toString(16)}`);
    if (words.u64 !== undefined) lines.push(`u64 0x${words.u64.toString(16)}`);
    this.tooltipEl.textContent = lines.join("\n");
    this.tooltipEl.hidden = false;
    const box = this.getBoundingClientRect();
    this.tooltipEl.style.left = e.clientX - box.left + 12 + "px";
    this.tooltipEl.style.top = e.clientY - box.top + 16 + "px";
  }

  private hideTooltip(): void {
    this.tooltipEl.hidden = true;
  }

  // ----- search -------------------------------------------------------------

  private currentNeedleLen(): number {
    const needle = buildNeedle(this.searchInput.value, this.searchMode);
    return needle ? needle.length : 0;
  }

  private refreshSearch(): void {
    if (!this.built) return;
    const needle = buildNeedle(this.searchInput.value, this.searchMode);
    if (!needle) {
      this._searchHits = [];
      this._currentHit = -1;
    } else {
      this._searchHits = findAll(this._bytes, needle);
      this._currentHit = this._searchHits.length ? 0 : -1;
      if (this._currentHit === 0) this.scrollToOffset(this._searchHits[0]!, false);
    }
    this.updateSearchCount();
    this.renderRows();
  }

  private updateSearchCount(): void {
    const el = this.querySelector(".mv-search-count")!;
    if (!this.searchInput.value) el.textContent = "";
    else if (this._searchHits.length === 0) el.textContent = "0/0";
    else el.textContent = `${this._currentHit + 1}/${this._searchHits.length}`;
  }

  private gotoNextMatch(): void {
    const needle = buildNeedle(this.searchInput.value, this.searchMode);
    if (!needle || this._searchHits.length === 0) return;
    const from = this._currentHit >= 0 ? this._searchHits[this._currentHit]! + 1 : 0;
    const idx = findNext(this._bytes, needle, from);
    this._currentHit = this._searchHits.indexOf(idx);
    this.scrollToOffset(idx, false);
    this.updateSearchCount();
    this.renderRows();
  }

  private gotoPrevMatch(): void {
    const needle = buildNeedle(this.searchInput.value, this.searchMode);
    if (!needle || this._searchHits.length === 0) return;
    const from = this._currentHit >= 0 ? this._searchHits[this._currentHit]! : this._bytes.length;
    const idx = findPrev(this._bytes, needle, from);
    this._currentHit = this._searchHits.indexOf(idx);
    this.scrollToOffset(idx, false);
    this.updateSearchCount();
    this.renderRows();
  }

  // ----- status bar ---------------------------------------------------------

  private renderStatus(): void {
    if (!this.built) return;
    if (this._selection) {
      const [a, b] = selRange(this._selection);
      const len = b - a + 1;
      const words = interpret(this._bytes, a, this._endianness);
      const parts = [
        `sel @ ${this.formatAddr(this.addrOf(a))}`,
        `len ${len}`,
      ];
      if (words.u8 !== undefined) parts.push(`u8 ${words.u8}`);
      if (words.u16 !== undefined) parts.push(`u16 ${words.u16}`);
      if (words.u32 !== undefined) parts.push(`u32 ${words.u32}`);
      if (words.u64 !== undefined) parts.push(`u64 ${words.u64}`);
      if (words.i64 !== undefined) parts.push(`i64 ${words.i64}`);
      if (words.f64 !== undefined) parts.push(`f64 ${words.f64}`);
      let s = parts.join("   ");
      const target = this.pointerTargetOf(a);
      if (target !== null) {
        s += `   → points into buffer @ ${this.formatAddr(this.addrOf(target))}`;
      }
      this.statusEl.textContent = s;
    } else {
      this.statusEl.textContent = `${this._bytes.length} bytes · base ${this.formatAddr(
        this._base,
      )} · cursor ${this.formatAddr(this.addrOf(this._cursor))}`;
    }
  }
}

customElements.define("memory-viewer", MemoryViewer);
