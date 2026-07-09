// A custom x86-64 assembly language for Monaco: a Monarch tokenizer, a
// completion provider (mnemonics + registers), a hover provider describing the
// mnemonic under the cursor, and a matching dark theme. The keyword lists come
// from src/core/asm-lang.ts so they stay in sync with the pure tokenizer used in
// tests.

import * as monaco from "monaco-editor/esm/vs/editor/editor.api";
import {
  DIRECTIVES,
  MNEMONICS,
  REGISTERS,
  SIZE_KEYWORDS,
} from "../core/asm-lang.ts";

export const ASM_LANGUAGE_ID = "x86asm";

// Short descriptions surfaced by the hover provider. Not exhaustive; unknown
// mnemonics fall back to a generic note.
const MNEMONIC_DOCS: Record<string, string> = {
  mov: "Copy the source operand into the destination. No flags affected.",
  lea: "Load Effective Address: compute an address expression and store it — arithmetic without a memory access.",
  add: "Integer add, setting CF/OF/SF/ZF/PF/AF.",
  sub: "Integer subtract, setting the arithmetic flags.",
  cmp: "Subtract for flags only; the result is discarded. Feeds a following jcc/setcc.",
  test: "Bitwise AND for flags only; the result is discarded.",
  and: "Bitwise AND. Clears CF and OF.",
  or: "Bitwise OR. Clears CF and OF.",
  xor: "Bitwise XOR. `xor r, r` is the idiomatic zeroing that also clears the upper 32 bits.",
  push: "Decrement rsp by 8 and store the operand at [rsp].",
  pop: "Load [rsp] into the operand and increment rsp by 8.",
  call: "Push the return address and jump. Falls through on return.",
  ret: "Pop the return address into rip.",
  jmp: "Unconditional jump. Does not fall through.",
  leave: "Tear down the stack frame: `mov rsp, rbp; pop rbp`.",
  syscall: "Enter the kernel. Number in rax; arguments in rdi, rsi, rdx, r10, r8, r9.",
  movzx: "Move with zero-extension to a wider register.",
  movsx: "Move with sign-extension to a wider register.",
  movsxd: "Sign-extend a 32-bit source to 64 bits (opcode 0x63).",
  imul: "Signed multiply.",
  mul: "Unsigned multiply into rdx:rax.",
  idiv: "Signed divide rdx:rax by the operand.",
  div: "Unsigned divide rdx:rax by the operand.",
  shl: "Shift left, filling with zeros; last bit out goes to CF.",
  shr: "Logical shift right, filling with zeros.",
  sar: "Arithmetic shift right, preserving the sign bit.",
  nop: "No operation. Used for alignment padding.",
  endbr64: "Control-flow-enforcement landing pad; a legal indirect branch target.",
  ud2: "Raise an invalid-opcode exception. Marks unreachable code.",
  hlt: "Halt. In this emulator, stops execution.",
};

function conditionalDoc(name: string): string | null {
  if (name.startsWith("j") && name.length > 1) {
    return `Conditional jump (${name}). Taken when the flags match the ${name.slice(1)} condition; falls through otherwise.`;
  }
  if (name.startsWith("set")) {
    return `Set byte to 1 when the ${name.slice(3)} condition holds, else 0.`;
  }
  if (name.startsWith("cmov")) {
    return `Conditional move: copy the source only when the ${name.slice(4)} condition holds.`;
  }
  return null;
}

export function mnemonicDoc(name: string): string {
  const lower = name.toLowerCase();
  return (
    MNEMONIC_DOCS[lower] ??
    conditionalDoc(lower) ??
    "x86-64 instruction. See the Explain tab for its byte-level encoding."
  );
}

let registered = false;

/** Register the language, providers and theme exactly once. */
export function registerAsmLanguage(): void {
  if (registered) return;
  registered = true;

  monaco.languages.register({ id: ASM_LANGUAGE_ID });

  monaco.languages.setMonarchTokensProvider(ASM_LANGUAGE_ID, {
    ignoreCase: true,
    mnemonics: MNEMONICS,
    registers: REGISTERS,
    sizes: SIZE_KEYWORDS,
    directives: DIRECTIVES,
    tokenizer: {
      root: [
        [/[;#].*$/, "comment"],
        [/"([^"\\]|\\.)*"/, "string"],
        [/'([^'\\]|\\.)*'/, "string"],
        [/[A-Za-z_.$][\w.$]*:/, "type.identifier"], // label definition
        [/\b0x[0-9a-fA-F]+\b/, "number.hex"],
        [/\b0b[01]+\b/, "number.binary"],
        [/\b[0-9][0-9a-fA-F]*h\b/, "number.hex"],
        [/\b\d+\b/, "number"],
        [
          /[A-Za-z_.$][\w.$]*/,
          {
            cases: {
              "@mnemonics": "keyword",
              "@registers": "variable.predefined",
              "@sizes": "type",
              "@directives": "keyword.directive",
              "@default": "identifier",
            },
          },
        ],
        [/[[\]]/, "delimiter.bracket"],
        [/[+\-*,:]/, "operator"],
      ],
    },
  });

  monaco.languages.registerCompletionItemProvider(ASM_LANGUAGE_ID, {
    provideCompletionItems(model, position) {
      const word = model.getWordUntilPosition(position);
      const range = new monaco.Range(
        position.lineNumber,
        word.startColumn,
        position.lineNumber,
        word.endColumn,
      );
      const suggestions: monaco.languages.CompletionItem[] = [];
      for (const m of MNEMONICS) {
        suggestions.push({
          label: m,
          kind: monaco.languages.CompletionItemKind.Keyword,
          insertText: m,
          detail: "instruction",
          documentation: mnemonicDoc(m),
          range,
        });
      }
      for (const r of REGISTERS) {
        suggestions.push({
          label: r,
          kind: monaco.languages.CompletionItemKind.Variable,
          insertText: r,
          detail: "register",
          range,
        });
      }
      for (const d of DIRECTIVES) {
        suggestions.push({
          label: d,
          kind: monaco.languages.CompletionItemKind.Snippet,
          insertText: d,
          detail: "directive",
          range,
        });
      }
      return { suggestions };
    },
  });

  monaco.languages.registerHoverProvider(ASM_LANGUAGE_ID, {
    provideHover(model, position) {
      const word = model.getWordAtPosition(position);
      if (!word) return null;
      const w = word.word.toLowerCase();
      if (MNEMONICS.includes(w)) {
        return {
          range: new monaco.Range(
            position.lineNumber,
            word.startColumn,
            position.lineNumber,
            word.endColumn,
          ),
          contents: [
            { value: `**${w}**` },
            { value: mnemonicDoc(w) },
          ],
        };
      }
      if (REGISTERS.includes(w)) {
        return {
          range: new monaco.Range(
            position.lineNumber,
            word.startColumn,
            position.lineNumber,
            word.endColumn,
          ),
          contents: [{ value: `**${w}** — general-purpose / special register` }],
        };
      }
      return null;
    },
  });

  monaco.editor.defineTheme("asm-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "7cc4ff", fontStyle: "bold" },
      { token: "keyword.directive", foreground: "c78bff" },
      { token: "variable.predefined", foreground: "ffb454" },
      { token: "type", foreground: "5ad4a6" },
      { token: "type.identifier", foreground: "f2c94c", fontStyle: "bold" },
      { token: "number.hex", foreground: "ff8f8f" },
      { token: "number.binary", foreground: "ff8f8f" },
      { token: "number", foreground: "ff8f8f" },
      { token: "string", foreground: "9ee493" },
      { token: "comment", foreground: "6b7686", fontStyle: "italic" },
      { token: "operator", foreground: "9aa5b1" },
    ],
    colors: {
      "editor.background": "#0e1116",
      "editor.foreground": "#d7dde5",
      "editorLineNumber.foreground": "#3a4250",
      "editor.lineHighlightBackground": "#161b22",
      "editorCursor.foreground": "#7cc4ff",
    },
  });
}
