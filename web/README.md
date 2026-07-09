# Assembly Teacher — Web Frontend

The browser frontend for the Assembly Teacher platform: an interactive x86-64
playground, a lesson runner, and a binary inspector. Plain TypeScript + Vite,
**no UI framework** — the reusable pieces are native Web Components, so a lesson
author can drop `<memory-viewer>` or `<insn-explain>` into any page and script it
with ordinary DOM.

## Develop

```bash
npm install
npm run dev          # Vite dev server on http://localhost:5173
```

Run the Rust API server on `127.0.0.1:8080`; the dev server proxies `/api/*` to
it (see `vite.config.ts`). Every component degrades gracefully when the API is
unreachable: the Playground still renders with a built-in sample buffer and
program, so the UI can be developed fully offline.

```bash
npm run build        # tsc --noEmit (strict) + vite build → dist/
npm run typecheck    # strict type-check only
npm test             # node:test unit tests for the pure logic
```

`npm test` runs the tests directly with Node's TypeScript type-stripping
(Node 22+); there is no test build step. Covered: hex/ASCII formatting, endian
interpretation, the byte-search routine, the byte-diff, the assembly tokenizer's
classification of a sample line, and the run-trace state reconstruction.

## Layout

```
web/
├── index.html
├── vite.config.ts            # /api proxy, dist outDir
├── tsconfig.json             # strict
├── src/
│   ├── main.ts               # hash router + shell/nav
│   ├── api.ts                # typed client for every endpoint in docs/api.md
│   ├── style.css             # the single stylesheet
│   ├── core/                 # pure, browser-free, unit-tested logic
│   │   ├── hex.ts            # byte/address/ASCII formatting, hex parse
│   │   ├── endian.ts         # u8..u64 / i8..i64 / f32 / f64 interpretation
│   │   ├── search.ts         # hex/ASCII needle search (all/next/prev)
│   │   ├── diff.ts           # byte-buffer diffing for modified-byte highlight
│   │   ├── asm-lang.ts       # mnemonic/register tables + a pure tokenizer
│   │   ├── markdown.ts       # tiny safe Markdown→HTML renderer
│   │   ├── emu-state.ts      # per-step state reconstruction for the scrubber
│   │   └── samples.ts        # offline sample buffer + program
│   ├── asm/
│   │   ├── monarch.ts        # Monaco language: tokenizer, completion, hover, theme
│   │   └── monaco-env.ts     # Monaco web-worker wiring for Vite
│   ├── components/           # the Web Components
│   │   ├── memory-viewer.ts  # <memory-viewer>
│   │   ├── register-view.ts  # <register-view>
│   │   ├── stack-view.ts     # <stack-view>
│   │   ├── insn-explain.ts   # <insn-explain>
│   │   └── code-editor.ts    # <code-editor>
│   └── pages/
│       ├── playground.ts     # #/playground
│       ├── lessons.ts        # #/lessons, #/lessons/:id
│       └── inspector.ts      # #/inspector
└── test/                     # node:test suites
```

## Components for lesson authors

All components are light-DOM custom elements styled by `src/style.css`; import the
module once (which registers the element) and then create it and set properties.
Attributes are intentionally minimal — rich state is passed as JS properties.

### `<memory-viewer>`

A synchronised, **virtualised** hex + ASCII dump. Only the rows intersecting the
viewport are ever in the DOM (rows are absolutely positioned inside a full-height
spacer), so a 16 MiB buffer scrolls smoothly.

Properties: `bytes: Uint8Array`, `base: number|bigint`, `bytesPerRow` (default
16), `endianness: 'little'|'big'`, `regions: {start,end,color,label}[]` (byte
**offsets**), `annotations: {addr,size,label,type}[]` (absolute **addresses**).
Methods: `setBytes(next)` (diffs, flashes changed bytes, keeps a dirty tint until
`clearDirty()`), `scrollToAddress(addr)`, `scrollToOffset(off)`,
`addBookmark(name, addr)`. Keyboard: arrows / PageUp / PageDown / Home / End move
a cursor; `g` prompts for an address. Click+drag selects across both panes; the
status bar interprets the selection as u8/u16/u32/u64/i64/f64. Clicking a qword
that points inside the buffer draws an arrow to its target.

```js
import "./components/memory-viewer.js";
const mv = document.createElement("memory-viewer");
mv.base = 0x601000n;
mv.regions = [{ start: 0x10, end: 0x21, color: "rgba(94,212,166,.3)", label: "string" }];
mv.annotations = [{ addr: 0x601048n, size: 8, label: "next", type: "ptr" }];
mv.setBytes(myUint8Array);
document.body.append(mv);
```

### `<register-view>`

The 16 GPRs + rip + flags. Each register expands to show its 32/16/8-bit
sub-registers so the zero-extension rule is visible; registers written on the
last step glow; flags render as individual bits with explanatory tooltips.

```js
const rv = document.createElement("register-view");
rv.setState({ rax: 3, rsp: 0x800000 }, /*rip*/ 8, flags, /*written*/ ["rax"]);
```

### `<stack-view>`

The stack drawn downward-growing, one row per qword, with rsp/rbp markers, a
meaning guess per slot (values that look like code addresses are flagged as
possible return addresses), and frames delimited by the saved-rbp chain.

```js
const sv = document.createElement("stack-view");
sv.setModel({ bytes, base: 0x7ff000n, rsp: 0x7ffe00n, rbp: 0x7ffe10n,
              codeRange: { start: 0x401000n, end: 0x401100n } });
```

### `<insn-explain>`

The byte-by-byte instruction breakdown from `POST /api/asm/explain`: coloured
byte chips (one hue per field), a card with the server's explanation per field,
and a labelled bit ruler splitting ModRM/SIB into mod/reg/rm (or
scale/index/base). Hovering a chip or card highlights the other. Fetch the data
yourself and hand it over:

```js
import { explain } from "./api.js";
const ie = document.createElement("insn-explain");
ie.setData(await explain({ hex: "488b442408" }));   // or ie.setError("…")
```

### `<code-editor>`

Monaco with the custom x86-64 language (Monarch tokenizer + completion + hover).
On change it debounces and POSTs to `/api/asm/assemble`, rendering `{error, line}`
as a marker on the offending line. Events: `assembled` (detail:
`AssembleResponse`), `assemble-error` (detail: `ApiError`), `change` (detail:
source string). Properties: `value`, `autoAssemble` (default true), `debounceMs`.

```js
const ed = document.createElement("code-editor");
ed.value = "mov rax, 1\nret";
ed.addEventListener("assembled", (e) => console.log(e.detail.hex));
```

## Notes on the API contract

The typed client in `src/api.ts` follows `docs/api.md` exactly. A few shapes in
the doc are underspecified; see the handoff notes for where the client made an
assumption (memory chunks for `/api/emu/step`, the mitigations panel and the
lesson/exercise shapes are the main ones).
