// The Playground page: editor on the left; on the right, tabs for the assembled
// bytes (with <insn-explain> for the selected instruction), the disassembly
// listing, and the run trace. "Run" calls /api/emu/run and a scrubber steps
// through the trace, driving the register / stack / memory views forwards and
// backwards.

import {
  disassemble,
  explain,
  run,
  ApiError,
  type AssembleResponse,
  type AssembledLine,
  type DisassembledInsn,
  type Stop,
} from "../api.ts";
import { CodeEditor } from "../components/code-editor.ts";
import { InsnExplain } from "../components/insn-explain.ts";
import { RegisterView } from "../components/register-view.ts";
import { StackView } from "../components/stack-view.ts";
import { MemoryViewer, type Region as MvRegion } from "../components/memory-viewer.ts";
import { reconstruct, type ReconstructedRun } from "../core/emu-state.ts";
import { SAMPLE_SOURCE } from "../core/samples.ts";
import { parseHex } from "../core/hex.ts";
import { parseWord, type Word } from "../core/word.ts";

/** Format a wire word as a padded hex address for a listing. */
function fmtAddr(w: Word): string {
  return "0x" + parseWord(w).toString(16).padStart(8, "0");
}

/** A translucent tint for a region, keyed on its rwx permissions. */
function permColor(perms: string): string {
  const x = perms.includes("x");
  const w = perms.includes("w");
  if (x) return "rgba(124, 196, 255, 0.16)"; // code
  if (w) return "rgba(242, 201, 76, 0.14)"; // writable data / stack
  return "rgba(90, 212, 166, 0.12)"; // read-only data
}

export function renderPlayground(root: HTMLElement): void {
  root.innerHTML = "";
  const page = document.createElement("div");
  page.className = "pg";
  page.innerHTML = `
    <section class="pg-left">
      <div class="pg-editor-bar">
        <button class="btn btn-primary pg-run">▶ Run</button>
        <span class="pg-status" role="status"></span>
      </div>
      <div class="pg-editor-host"></div>
    </section>
    <section class="pg-right">
      <div class="tabs" role="tablist">
        <button class="tab tab-active" data-tab="asm" role="tab">Assembled</button>
        <button class="tab" data-tab="disasm" role="tab">Disassembly</button>
        <button class="tab" data-tab="trace" role="tab">Run trace</button>
      </div>
      <div class="tabpanels">
        <div class="tabpanel tabpanel-active" data-panel="asm"></div>
        <div class="tabpanel" data-panel="disasm"></div>
        <div class="tabpanel" data-panel="trace"></div>
      </div>
    </section>
  `;
  root.appendChild(page);

  const editor = new CodeEditor();
  editor.value = SAMPLE_SOURCE;
  page.querySelector(".pg-editor-host")!.appendChild(editor);

  const statusEl = page.querySelector<HTMLElement>(".pg-status")!;
  const asmPanel = page.querySelector<HTMLElement>('[data-panel="asm"]')!;
  const disasmPanel = page.querySelector<HTMLElement>('[data-panel="disasm"]')!;
  const tracePanel = page.querySelector<HTMLElement>('[data-panel="trace"]')!;

  // ---- tab switching ----
  const tabs = Array.from(page.querySelectorAll<HTMLButtonElement>(".tab"));
  const panels = Array.from(page.querySelectorAll<HTMLElement>(".tabpanel"));
  for (const tab of tabs) {
    tab.addEventListener("click", () => {
      for (const t of tabs) t.classList.toggle("tab-active", t === tab);
      for (const p of panels) {
        p.classList.toggle(
          "tabpanel-active",
          p.dataset.panel === tab.dataset.tab,
        );
      }
      editor.layout();
    });
  }

  // ---- Assembled tab ----
  const explainView = new InsnExplain();
  let lastAssembled: AssembleResponse | null = null;

  function renderAssembled(res: AssembleResponse): void {
    asmPanel.innerHTML = "";
    const list = document.createElement("div");
    list.className = "listing";
    if (res.lines.length === 0) {
      list.textContent = "No instructions emitted.";
    }
    res.lines.forEach((ln: AssembledLine) => {
      const row = document.createElement("button");
      row.className = "listing-row";
      row.innerHTML =
        `<span class="li-addr">${fmtAddr(ln.address)}</span>` +
        `<span class="li-hex">${ln.hex}</span>` +
        `<span class="li-text">${escapeHtml(ln.text)}</span>`;
      row.addEventListener("click", () => {
        for (const r of list.querySelectorAll(".listing-row"))
          r.classList.remove("li-selected");
        row.classList.add("li-selected");
        void showExplain(ln.hex);
      });
      list.appendChild(row);
    });
    asmPanel.append(list, explainView);
    // auto-select the first instruction
    const first = list.querySelector<HTMLButtonElement>(".listing-row");
    if (first) first.click();
  }

  async function showExplain(hex: string): Promise<void> {
    try {
      const res = await explain({ hex });
      explainView.setData(res);
    } catch (e) {
      explainView.setError(
        e instanceof ApiError ? e.message : "explain unavailable (offline?)",
      );
    }
  }

  // ---- Disassembly tab ----
  async function renderDisasm(hex: string): Promise<void> {
    disasmPanel.innerHTML = "";
    try {
      const res = await disassemble({ hex });
      const list = document.createElement("div");
      list.className = "listing";
      res.instructions.forEach((insn: DisassembledInsn) => {
        const row = document.createElement("button");
        row.className = "listing-row";
        row.innerHTML =
          `<span class="li-addr">${fmtAddr(insn.ip)}</span>` +
          `<span class="li-hex">${insn.hex}</span>` +
          `<span class="li-text">${escapeHtml(insn.text)}</span>` +
          `<span class="li-desc">${escapeHtml(insn.description)}</span>`;
        row.addEventListener("click", () => void showExplainInPanel(insn.hex));
        list.appendChild(row);
      });
      disasmPanel.appendChild(list);
      if (res.error) {
        const err = document.createElement("div");
        err.className = "notice notice-warn";
        err.textContent = `decode stopped: ${res.error}`;
        disasmPanel.appendChild(err);
      }
      const ex = new InsnExplain();
      disasmPanel.appendChild(ex);
      // stash so clicks can target it
      disasmExplain = ex;
    } catch (e) {
      disasmPanel.innerHTML = "";
      const err = document.createElement("div");
      err.className = "notice notice-warn";
      err.textContent =
        e instanceof ApiError ? e.message : "disassembly unavailable (offline?)";
      disasmPanel.appendChild(err);
    }
  }

  let disasmExplain: InsnExplain | null = null;
  async function showExplainInPanel(hex: string): Promise<void> {
    if (!disasmExplain) return;
    try {
      disasmExplain.setData(await explain({ hex }));
    } catch (e) {
      disasmExplain.setError(
        e instanceof ApiError ? e.message : "explain unavailable",
      );
    }
  }

  // ---- Run trace tab ----
  const registerView = new RegisterView();
  const stackView = new StackView();
  const memoryView = new MemoryViewer();
  let current: ReconstructedRun | null = null;
  let codeLen = 0;
  let runBase = 0n;

  tracePanel.innerHTML = `
    <div class="trace-controls">
      <span class="trace-stop"></span>
      <input class="trace-scrub" type="range" min="0" max="0" value="0"
             aria-label="Execution step" disabled />
      <span class="trace-pos"></span>
    </div>
    <div class="trace-truncated notice notice-warn" hidden></div>
    <div class="trace-insn"></div>
    <div class="trace-stdout" hidden></div>
    <div class="trace-views">
      <div class="trace-col trace-regs"></div>
      <div class="trace-col trace-stack"></div>
    </div>
    <div class="trace-mem"></div>
  `;
  tracePanel.querySelector(".trace-regs")!.appendChild(registerView);
  tracePanel.querySelector(".trace-stack")!.appendChild(stackView);
  tracePanel.querySelector(".trace-mem")!.appendChild(memoryView);

  const scrub = tracePanel.querySelector<HTMLInputElement>(".trace-scrub")!;
  const posEl = tracePanel.querySelector<HTMLElement>(".trace-pos")!;
  const stopEl = tracePanel.querySelector<HTMLElement>(".trace-stop")!;
  const insnEl = tracePanel.querySelector<HTMLElement>(".trace-insn")!;
  const stdoutEl = tracePanel.querySelector<HTMLElement>(".trace-stdout")!;
  const truncEl = tracePanel.querySelector<HTMLElement>(".trace-truncated")!;

  function applySnapshot(index: number): void {
    if (!current) return;
    const snap = current.snapshots[index]!;
    registerView.setState(
      snap.registers,
      snap.ripNow,
      snap.flags,
      snap.writtenRegs,
    );
    const mem = current.memoryAt(index);
    memoryView.base = current.memBase;
    // colour each mapped region by its permissions
    memoryView.regions = current.windowRegions.map(
      (r): MvRegion => ({
        start: r.start,
        end: r.end,
        color: permColor(r.perms),
        label: `${r.name} ${r.perms}`,
      }),
    );
    memoryView.setBytes(mem);
    stackView.setModel({
      bytes: mem,
      base: current.memBase,
      rsp: snap.registers["rsp"] ?? 0n,
      rbp: snap.registers["rbp"] ?? 0n,
      codeRange: { start: runBase, end: runBase + BigInt(codeLen) },
    });
    insnEl.textContent =
      index === 0
        ? "before first instruction"
        : `step ${index}/${current.snapshots.length - 1}: ${snap.text}`;
    posEl.textContent = `${index} / ${current.snapshots.length - 1}`;
  }

  scrub.addEventListener("input", () => applySnapshot(Number(scrub.value)));

  async function doRun(): Promise<void> {
    const source = editor.value;
    statusEl.textContent = "running…";
    try {
      const res = await run({ source, maxSteps: 100000 });
      codeLen = parseHex(lastAssembled?.hex ?? "")?.length ?? 0;
      runBase = parseWord(res.base);
      current = reconstruct(res);
      const maxIdx = current.snapshots.length - 1;
      scrub.max = String(maxIdx);
      scrub.value = String(maxIdx);
      scrub.disabled = false;
      stopEl.textContent = formatStop(res.stop);
      stopEl.className = "trace-stop " + (res.stop.kind === "fault" ? "bad" : "ok");
      if (res.traceTruncated) {
        truncEl.hidden = false;
        truncEl.textContent =
          `trace truncated to ${res.trace.length} steps (the run itself completed in ${res.steps})`;
      } else {
        truncEl.hidden = true;
      }
      const out = [res.stdout && "stdout: " + res.stdout, res.stderr && "stderr: " + res.stderr]
        .filter(Boolean)
        .join("\n");
      if (out) {
        stdoutEl.hidden = false;
        stdoutEl.textContent = out;
      } else {
        stdoutEl.hidden = true;
      }
      applySnapshot(maxIdx);
      statusEl.textContent = `${res.steps} steps`;
      // switch to the trace tab
      tabs.find((t) => t.dataset.tab === "trace")?.click();
    } catch (e) {
      statusEl.textContent =
        e instanceof ApiError ? `run failed: ${e.message}` : "run unavailable (offline?)";
    }
  }

  page.querySelector<HTMLButtonElement>(".pg-run")!.addEventListener("click", () =>
    void doRun(),
  );

  // ---- react to assembly results ----
  editor.addEventListener("assembled", (e) => {
    const res = (e as CustomEvent<AssembleResponse>).detail;
    lastAssembled = res;
    statusEl.textContent = `${parseHex(res.hex)?.length ?? 0} bytes`;
    renderAssembled(res);
    void renderDisasm(res.hex);
  });
  editor.addEventListener("assemble-error", (e) => {
    const err = (e as CustomEvent<unknown>).detail;
    statusEl.textContent =
      err instanceof ApiError
        ? `assemble error${err.line ? " (line " + err.line + ")" : ""}: ${err.message}`
        : "assembler unavailable (offline?)";
  });
}

function formatStop(stop: Stop): string {
  switch (stop.kind) {
    case "exited":
      return `exited (code ${stop.code ?? 0})`;
    case "halted":
      return "halted";
    case "stepLimit":
      return "hit step limit";
    case "fault":
      return `fault: ${stop.reason ?? "?"}${stop.address !== undefined ? " @ " + fmtAddr(stop.address) : ""}`;
    case "breakpoint":
      return "breakpoint";
    default:
      return stop.kind;
  }
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
