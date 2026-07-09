// <code-editor> — Monaco with the custom x86-64 assembly language. On change it
// debounces and POSTs to /api/asm/assemble, mapping any {error, line} into a
// Monaco marker on the offending line. Emits `assembled` with the successful
// response and `assemble-error` with the ApiError so pages can react.
//
// Because Monaco needs a real DOM it is created lazily in connectedCallback.
// The component degrades gracefully offline: an assemble failure is shown as a
// marker, never as a blanked-out editor.

import * as monaco from "monaco-editor/esm/vs/editor/editor.api";
import { installMonacoEnvironment } from "../asm/monaco-env.ts";
import { ASM_LANGUAGE_ID, registerAsmLanguage } from "../asm/monarch.ts";
import { assemble, ApiError, type AssembleResponse } from "../api.ts";

const OWNER = "asm-assemble";

export class CodeEditor extends HTMLElement {
  private editor: monaco.editor.IStandaloneCodeEditor | null = null;
  private model: monaco.editor.ITextModel | null = null;
  private debounceTimer: number | undefined;
  private _debounceMs = 400;
  private _autoAssemble = true;
  private _pendingValue = "";

  connectedCallback(): void {
    installMonacoEnvironment();
    registerAsmLanguage();

    const host = document.createElement("div");
    host.className = "ce-host";
    this.classList.add("ce");
    this.appendChild(host);

    this.model = monaco.editor.createModel(
      this._pendingValue || this.getAttribute("value") || "",
      ASM_LANGUAGE_ID,
    );
    this.editor = monaco.editor.create(host, {
      model: this.model,
      theme: "asm-dark",
      fontSize: 14,
      fontFamily:
        "ui-monospace, SFMono-Regular, 'JetBrains Mono', Menlo, monospace",
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      automaticLayout: true,
      lineNumbers: "on",
      renderWhitespace: "selection",
      tabSize: 8,
      wordWrap: "off",
    });

    this.model.onDidChangeContent(() => {
      this.dispatchEvent(new CustomEvent("change", { detail: this.value }));
      if (this._autoAssemble) this.scheduleAssemble();
    });

    if (this._autoAssemble) this.scheduleAssemble();
  }

  disconnectedCallback(): void {
    if (this.debounceTimer) clearTimeout(this.debounceTimer);
    this.editor?.dispose();
    this.model?.dispose();
    this.editor = null;
    this.model = null;
  }

  get value(): string {
    return this.model?.getValue() ?? this._pendingValue;
  }
  set value(v: string) {
    this._pendingValue = v;
    if (this.model && this.model.getValue() !== v) this.model.setValue(v);
  }

  set autoAssemble(on: boolean) {
    this._autoAssemble = on;
  }
  set debounceMs(ms: number) {
    this._debounceMs = ms;
  }

  layout(): void {
    this.editor?.layout();
  }

  focusEditor(): void {
    this.editor?.focus();
  }

  private scheduleAssemble(): void {
    if (this.debounceTimer) clearTimeout(this.debounceTimer);
    this.debounceTimer = window.setTimeout(
      () => void this.runAssemble(),
      this._debounceMs,
    );
  }

  /** Assemble now, updating markers and emitting events. Returns the result. */
  async runAssemble(): Promise<AssembleResponse | null> {
    if (!this.model) return null;
    const source = this.model.getValue();
    try {
      const res = await assemble({ source });
      monaco.editor.setModelMarkers(this.model, OWNER, []);
      this.dispatchEvent(new CustomEvent("assembled", { detail: res }));
      return res;
    } catch (e) {
      this.showError(e);
      this.dispatchEvent(new CustomEvent("assemble-error", { detail: e }));
      return null;
    }
  }

  private showError(e: unknown): void {
    if (!this.model) return;
    const err = e instanceof ApiError ? e : null;
    const message = err ? err.message : String((e as Error)?.message ?? e);
    const line = err?.line ?? 1;
    const lineContent = this.model.getLineContent(
      Math.min(line, this.model.getLineCount()),
    );
    monaco.editor.setModelMarkers(this.model, OWNER, [
      {
        severity: monaco.MarkerSeverity.Error,
        message,
        startLineNumber: line,
        startColumn: 1,
        endLineNumber: line,
        endColumn: Math.max(2, lineContent.length + 1),
      },
    ]);
  }
}

customElements.define("code-editor", CodeEditor);
