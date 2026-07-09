// The Inspector page: drag-and-drop an executable, POST /api/binfmt/inspect,
// then render the header summary, section table, segment/permission map, a
// filterable symbol table, imports/exports, relocations and a mitigations
// panel. Clicking .text loads its bytes into <memory-viewer> and disassembles.

import {
  inspectFile,
  disassemble,
  ApiError,
  type BinSection,
  type BinMitigations,
  type InspectResponse,
} from "../api.ts";
import { MemoryViewer, type Region } from "../components/memory-viewer.ts";

export function renderInspector(root: HTMLElement): void {
  root.innerHTML = "";
  const page = document.createElement("div");
  page.className = "insp";
  page.innerHTML = `
    <div class="insp-drop" tabindex="0" role="button" aria-label="Drop an executable to inspect">
      <div class="drop-inner">
        <strong>Drop an executable here</strong>
        <span>ELF / PE / Mach-O, up to 16 MiB — or click to choose a file</span>
        <input type="file" class="insp-file" hidden />
      </div>
    </div>
    <div class="insp-result"></div>
  `;
  root.appendChild(page);

  const drop = page.querySelector<HTMLElement>(".insp-drop")!;
  const fileInput = page.querySelector<HTMLInputElement>(".insp-file")!;
  const result = page.querySelector<HTMLElement>(".insp-result")!;

  const openFile = (file: File): void => void inspect(file, result);

  drop.addEventListener("click", () => fileInput.click());
  drop.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") fileInput.click();
  });
  fileInput.addEventListener("change", () => {
    const f = fileInput.files?.[0];
    if (f) openFile(f);
  });
  drop.addEventListener("dragover", (e) => {
    e.preventDefault();
    drop.classList.add("drag-over");
  });
  drop.addEventListener("dragleave", () => drop.classList.remove("drag-over"));
  drop.addEventListener("drop", (e) => {
    e.preventDefault();
    drop.classList.remove("drag-over");
    const f = e.dataTransfer?.files?.[0];
    if (f) openFile(f);
  });
}

async function inspect(file: File, result: HTMLElement): Promise<void> {
  result.innerHTML = `<div class="notice notice-info">inspecting ${escapeHtml(file.name)}…</div>`;
  let info: InspectResponse;
  let fileBytes: Uint8Array;
  try {
    fileBytes = new Uint8Array(await file.arrayBuffer());
    info = await inspectFile(file);
  } catch (e) {
    result.innerHTML = "";
    result.appendChild(
      noticeEl(e instanceof ApiError ? e.message : "inspect failed (offline?)", "warn"),
    );
    return;
  }
  result.innerHTML = "";

  result.appendChild(summaryPanel(info));
  result.appendChild(mitigationsPanel(info.mitigations));

  const memWrap = document.createElement("div");
  memWrap.className = "insp-mem";
  memWrap.hidden = true;
  const mem = new MemoryViewer();
  const disasmEl = document.createElement("div");
  disasmEl.className = "listing insp-disasm";

  result.appendChild(sectionTable(info, fileBytes, mem, disasmEl, memWrap));
  result.appendChild(segmentMap(info));
  result.appendChild(symbolTable(info));
  result.appendChild(importExport(info));
  result.appendChild(relocationTable(info));

  memWrap.append(mem, disasmEl);
  result.appendChild(memWrap);
}

function summaryPanel(info: InspectResponse): HTMLElement {
  const el = panel("Summary");
  const grid = document.createElement("div");
  grid.className = "kv";
  const rows: Array<[string, string]> = [
    ["format", info.format],
    ["arch", info.arch],
    ["entry", "0x" + info.entry.toString(16)],
    ["sections", String(info.sections.length)],
    ["segments", String(info.segments.length)],
    ["symbols", String(info.symbols.length)],
  ];
  for (const [k, v] of rows) {
    grid.insertAdjacentHTML(
      "beforeend",
      `<span class="k">${k}</span><span class="v">${escapeHtml(v)}</span>`,
    );
  }
  el.appendChild(grid);
  return el;
}

function mitigationsPanel(m: BinMitigations | undefined): HTMLElement {
  const el = panel("Mitigations");
  const row = document.createElement("div");
  row.className = "chips";
  if (!m) {
    el.appendChild(noticeEl("no mitigation data reported", "info"));
    return el;
  }
  const chip = (label: string, pass: boolean | undefined, good: boolean): void => {
    const c = document.createElement("span");
    const on = pass === true;
    c.className = "chip " + (pass === undefined ? "chip-unknown" : good === on ? "chip-pass" : "chip-fail");
    c.textContent = `${label}: ${pass === undefined ? "?" : on ? "yes" : "no"}`;
    row.appendChild(c);
  };
  chip("NX", m.nx, true);
  chip("PIE", m.pie, true);
  chip("Canary", m.canary, true);
  chip("CFG", m.cfg, true);
  chip("CET", m.cet, true);
  if (m.relro !== undefined) {
    const c = document.createElement("span");
    const full = m.relro === "full";
    c.className = "chip " + (full ? "chip-pass" : m.relro === "none" ? "chip-fail" : "chip-warn");
    c.textContent = `RELRO: ${m.relro}`;
    row.appendChild(c);
  }
  el.appendChild(row);
  return el;
}

function sectionTable(
  info: InspectResponse,
  fileBytes: Uint8Array,
  mem: MemoryViewer,
  disasmEl: HTMLElement,
  memWrap: HTMLElement,
): HTMLElement {
  const el = panel("Sections");
  const table = document.createElement("table");
  table.className = "tbl";
  table.innerHTML =
    "<thead><tr><th>name</th><th>address</th><th>size</th><th>offset</th><th>flags</th></tr></thead>";
  const tbody = document.createElement("tbody");
  for (const s of info.sections) {
    const tr = document.createElement("tr");
    const clickable = s.flags.includes("execute") || s.name === ".text";
    if (clickable) tr.classList.add("row-clickable");
    tr.innerHTML =
      `<td>${escapeHtml(s.name)}</td>` +
      `<td>0x${s.address.toString(16)}</td>` +
      `<td>${s.size}</td>` +
      `<td>0x${s.offset.toString(16)}</td>` +
      `<td>${s.flags.map((f) => `<span class="flag">${escapeHtml(f)}</span>`).join("")}</td>`;
    if (clickable) {
      tr.addEventListener("click", () =>
        void loadSection(s, fileBytes, info, mem, disasmEl, memWrap),
      );
    }
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  el.appendChild(wrapScroll(table));
  return el;
}

async function loadSection(
  s: BinSection,
  fileBytes: Uint8Array,
  info: InspectResponse,
  mem: MemoryViewer,
  disasmEl: HTMLElement,
  memWrap: HTMLElement,
): Promise<void> {
  memWrap.hidden = false;
  const start = s.offset;
  const end = Math.min(fileBytes.length, s.offset + s.size);
  const slice = fileBytes.slice(start, end);
  mem.base = BigInt(s.address);
  const regions: Region[] = [
    { start: 0, end: slice.length, color: "rgba(124,196,255,0.18)", label: s.name },
  ];
  mem.regions = regions;
  mem.setBytes(slice);
  memWrap.scrollIntoView({ behavior: "smooth", block: "nearest" });

  // disassemble executable sections
  disasmEl.innerHTML = "";
  if (s.flags.includes("execute") || s.name === ".text") {
    try {
      const hex = toHex(slice.slice(0, Math.min(slice.length, 4096)));
      const res = await disassemble({ hex, base: s.address });
      for (const insn of res.instructions) {
        const row = document.createElement("div");
        row.className = "listing-row";
        row.innerHTML =
          `<span class="li-addr">0x${insn.ip.toString(16)}</span>` +
          `<span class="li-hex">${insn.hex}</span>` +
          `<span class="li-text">${escapeHtml(insn.text)}</span>`;
        disasmEl.appendChild(row);
      }
      if (res.error) {
        disasmEl.appendChild(noticeEl(`decode stopped: ${res.error}`, "warn"));
      }
    } catch {
      disasmEl.appendChild(noticeEl("disassembly unavailable (offline?)", "warn"));
    }
  }
}

function segmentMap(info: InspectResponse): HTMLElement {
  const el = panel("Segments");
  const table = document.createElement("table");
  table.className = "tbl";
  table.innerHTML =
    "<thead><tr><th>type</th><th>vaddr</th><th>memsz</th><th>perms</th></tr></thead>";
  const tbody = document.createElement("tbody");
  for (const seg of info.segments) {
    const tr = document.createElement("tr");
    tr.innerHTML =
      `<td>${escapeHtml(seg.type)}</td>` +
      `<td>0x${seg.vaddr.toString(16)}</td>` +
      `<td>${seg.memsz}</td>` +
      `<td class="perms">${permBadges(seg.perms)}</td>`;
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  el.appendChild(wrapScroll(table));
  return el;
}

function permBadges(perms: string): string {
  return ["r", "w", "x"]
    .map((p) => {
      const on = perms.includes(p);
      return `<span class="perm ${on ? "perm-on" : "perm-off"}">${on ? p : "-"}</span>`;
    })
    .join("");
}

function symbolTable(info: InspectResponse): HTMLElement {
  const el = panel(`Symbols (${info.symbols.length})`);
  const filter = document.createElement("input");
  filter.className = "filter";
  filter.type = "text";
  filter.placeholder = "filter symbols…";
  filter.setAttribute("aria-label", "Filter symbols");
  el.appendChild(filter);

  const table = document.createElement("table");
  table.className = "tbl";
  table.innerHTML =
    "<thead><tr><th>name</th><th>address</th><th>size</th><th>kind</th><th>binding</th></tr></thead>";
  const tbody = document.createElement("tbody");
  const render = (needle: string): void => {
    tbody.innerHTML = "";
    const low = needle.toLowerCase();
    for (const sym of info.symbols) {
      if (low && !sym.name.toLowerCase().includes(low)) continue;
      const tr = document.createElement("tr");
      tr.innerHTML =
        `<td>${escapeHtml(sym.name)}</td>` +
        `<td>0x${sym.address.toString(16)}</td>` +
        `<td>${sym.size}</td>` +
        `<td>${escapeHtml(sym.kind)}</td>` +
        `<td>${escapeHtml(sym.binding)}</td>`;
      tbody.appendChild(tr);
    }
  };
  render("");
  filter.addEventListener("input", () => render(filter.value));
  table.appendChild(tbody);
  el.appendChild(wrapScroll(table));
  return el;
}

function importExport(info: InspectResponse): HTMLElement {
  const el = panel("Imports & Exports");
  const cols = document.createElement("div");
  cols.className = "two-col";

  const imp = document.createElement("div");
  imp.innerHTML = `<h4>Imports (${info.imports.length})</h4>`;
  const impList = document.createElement("ul");
  impList.className = "plain-list";
  for (const im of info.imports) {
    const li = document.createElement("li");
    li.textContent = `${im.name}  ←  ${im.library} (${im.kind})`;
    impList.appendChild(li);
  }
  imp.appendChild(impList);

  const exp = document.createElement("div");
  exp.innerHTML = `<h4>Exports (${info.exports.length})</h4>`;
  const expList = document.createElement("ul");
  expList.className = "plain-list";
  for (const ex of info.exports) {
    const li = document.createElement("li");
    li.textContent = `${ex.name}${ex.address ? " @ 0x" + ex.address.toString(16) : ""}`;
    expList.appendChild(li);
  }
  exp.appendChild(expList);

  cols.append(imp, exp);
  el.appendChild(cols);
  return el;
}

function relocationTable(info: InspectResponse): HTMLElement {
  const el = panel(`Relocations (${info.relocations.length})`);
  const table = document.createElement("table");
  table.className = "tbl";
  table.innerHTML =
    "<thead><tr><th>offset</th><th>kind</th><th>symbol</th><th>addend</th></tr></thead>";
  const tbody = document.createElement("tbody");
  for (const r of info.relocations) {
    const tr = document.createElement("tr");
    tr.innerHTML =
      `<td>0x${r.offset.toString(16)}</td>` +
      `<td>${escapeHtml(r.kind)}</td>` +
      `<td>${escapeHtml(r.symbol)}</td>` +
      `<td>${r.addend}</td>`;
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  el.appendChild(wrapScroll(table));
  return el;
}

// ---- small helpers ----

function panel(title: string): HTMLElement {
  const el = document.createElement("section");
  el.className = "panel";
  el.innerHTML = `<h3 class="panel-title">${escapeHtml(title)}</h3>`;
  return el;
}
function wrapScroll(inner: HTMLElement): HTMLElement {
  const w = document.createElement("div");
  w.className = "tbl-scroll";
  w.appendChild(inner);
  return w;
}
function noticeEl(text: string, kind: "warn" | "info"): HTMLElement {
  const el = document.createElement("div");
  el.className = `notice notice-${kind}`;
  el.textContent = text;
  return el;
}
function toHex(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i++) s += bytes[i]!.toString(16).padStart(2, "0");
  return s;
}
function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
