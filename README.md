# Assembly Teacher

An end-to-end platform for learning how modern software actually executes —
x86_64 architecture, assembly language, executable formats, process memory,
operating-system interaction, debugging, and reverse engineering — from first
principles, through interactive tools and executable lessons.

It is not a syntax tutorial. The aim is to teach the *machine*: not only what an
instruction does, but why the machine is built the way it is, and how that
explains the behaviour of every program above it. Two's complement is derived
from wanting one adder rather than two; little-endian from wanting a narrowing
cast to be free; `xor eax, eax` from a partial-register dependency stall. The
decoder, the emulator, and the executable parsers are all first-party, readable
code, because the lessons point directly at them.

## What's here

```
asmteacher/
├── crates/
│   ├── asm-core/   x86_64 decoder, encoder and assembler — written to be read
│   ├── asm-emu/    a step-by-step interpreter that records every effect
│   ├── binfmt/     ELF64 and PE32+ parsers, for inspection
│   ├── lesson/     the lesson framework: load, validate, and grade
│   └── server/     the REST API and static host (axum)
├── web/            the browser frontend (TypeScript + Vite, no framework)
├── lessons/        the curriculum — one self-contained directory per lesson
├── docs/           architecture and API reference
└── contrib/        Dockerfile, build.sh, test.sh
```

The dependency arrow runs one way: `server` and `lesson` build on `asm-core`,
`asm-emu` and `binfmt`, and those core crates depend on nothing but the standard
library, `serde` and `thiserror`. The website is an interface over reusable
libraries, not a place where logic lives — a handler that starts wanting to know
something about x86 is a sign that knowledge belongs in a crate.

## Quick start

```sh
# Build the workspace and the frontend bundle.
contrib/build.sh

# Serve the API and the built site. TLS is assumed to be terminated upstream.
target/release/asmteacher --listen 127.0.0.1:8080 --web web/dist --lessons lessons
```

Then open <http://127.0.0.1:8080>.

For development, run the API and the frontend dev server separately so the
frontend hot-reloads:

```sh
# Terminal 1 — the API on :8080
cargo run -p server -- --lessons lessons

# Terminal 2 — the Vite dev server on :5173, proxying /api to :8080
cd web && npm install && npm run dev
```

The server refuses to start if the curriculum does not validate. That is
deliberate: a lesson whose reference answer is wrong is worse than a missing
lesson.

## The tools

**Playground.** Write assembly in a Monaco editor with live diagnostics, see it
assembled with a byte-by-byte breakdown of any instruction, then run it and
*scrub the execution forwards and backwards* — the register view, stack view and
memory viewer all follow the scrubber, so you can watch `rsp` descend through a
recursive call and climb back out.

**Inspector.** Drag in an ELF or PE executable and see its sections, segments
and their permissions, symbol table, imports, exports, relocations, and a
security-mitigations panel (NX, PIE, RELRO, stack canary, CFG, CET). Click
`.text` to disassemble it.

**Register reference.** A linkable page (`#/registers/<name>`) showing the whole
general-purpose register file — every family and width — with an interactive
card that reveals which bits each name owns, how it aliases the others, and what
a write to it does. The hover cards in lessons link straight to it.

**Memory viewer.** A synchronised hex/ASCII dump built for demonstrations and
CTF-style challenges: coloured regions, structure overlays, pointer arrows,
modified-byte highlighting, selectable endianness, search and bookmarks. It
stays smooth on a 16 MiB buffer.

## Building and testing

```sh
contrib/build.sh              # release build + frontend bundle
contrib/build.sh --debug      # fast debug build, Rust only

contrib/test.sh               # the whole suite: fmt, clippy, Rust tests, web tests, build
contrib/test.sh --fast        # skip clippy and the frontend
contrib/test.sh --rust        # Rust only
```

The test suite is unusually load-bearing, in two ways worth knowing about:

- **The decoder and assembler are validated against real tools.** `asm-core`'s
  differential tests assemble with `nasm` and check instruction boundaries
  against `objdump`, because a decoder checked only by its own encoder shares
  its blind spots. These tests *skip themselves* if the tools are absent, so a
  green run on a bare machine proves less than it looks — `contrib/test.sh`
  warns loudly when that happens, and `contrib/Dockerfile` provides a complete
  environment where they always run.

- **The lessons are executed by the tests.** Every code example in every lesson
  is assembled, and every exercise's stated answer is graded by the same code
  that grades a student. If the assembler ever grows a shorter encoding and a
  chapter's printed bytes drift out of date, `cargo test` fails. See
  [`lessons/README.md`](lessons/README.md) and
  [`SKILL.md`](SKILL.md) for how to write one.

For a fully reproducible environment (Rust, Node, nasm, binutils, gcc, gdb):

```sh
docker build -t asmteacher-dev -f contrib/Dockerfile .
docker run --rm -v "$PWD":/work asmteacher-dev contrib/test.sh
```

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — how the pieces fit together
  and why the boundaries fall where they do.
- [`docs/api.md`](docs/api.md) — the REST API reference.
- [`docs/deployment.md`](docs/deployment.md) — exposing it publicly: the
  security model, the caps the app enforces, and the rate limiting the reverse
  proxy must add.
- [`SKILL.md`](SKILL.md) — a step-by-step guide to authoring a new lesson.
- [`lessons/README.md`](lessons/README.md) — the lesson directory format.
- Each crate's `lib.rs` carries a module-level overview; `cargo doc --open` is
  worth your time.

## Status

The foundation is complete and the curriculum grows on top of it. Implemented so
far: the full core-crate stack with differential validation, the REST API, the
frontend shell with all its visualisations, the lesson framework, and twenty-seven
lessons spanning Parts I–XV — through computer fundamentals, stack and heap,
processes, virtual memory and ASLR, ELF and dynamic linking, reverse engineering,
debugging internals, memory structures, compiler behavior, OS interaction,
exploit mitigations, and a capstone workflow. The
[curriculum](docs/architecture.md#curriculum) is intentionally broad and expands
lesson by lesson on this proven spine.

## Licence

MIT OR Apache-2.0.
