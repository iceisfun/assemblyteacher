// Monaco worker wiring for Vite. We only use the base editor worker: the custom
// asm language runs its Monarch tokenizer on the main thread, and we register no
// TypeScript/JSON/CSS language services, so no other workers are needed.

import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";

let installed = false;

export function installMonacoEnvironment(): void {
  if (installed) return;
  installed = true;
  self.MonacoEnvironment = {
    getWorker() {
      return new EditorWorker();
    },
  };
}
