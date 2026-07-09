# REST API

The server exposes the internal Rust libraries over HTTP. It contains no
assembly, parsing or emulation logic of its own — every endpoint is a thin
adapter over `asm-core`, `asm-emu`, `binfmt` or `lesson`. If you find yourself
wanting to add logic to a handler, it belongs in a crate instead.

All requests and responses are `application/json` unless stated otherwise.
Errors use the shape:

```json
{ "error": "unknown mnemonic `frobnicate`", "line": 2, "kind": "assemble" }
```

`line` is present only when the error can be attributed to a source line.
Status is `400` for user error, `413` for oversized input, `500` otherwise.

The server assumes TLS is terminated by a reverse proxy in front of it. It
speaks plain HTTP and does not read certificates.

## Conventions

- Machine code crosses the wire as a lowercase hex string, no separators:
  `"488b442408"`. Whitespace and `0x` prefixes are accepted on input.
- Addresses are JSON numbers when they fit a double exactly, which every
  address in this project does; treat them as `u64` server-side.
- `base` is the virtual address the first byte is assumed to live at, and
  defaults to `0`. It affects RIP-relative operands and branch targets.

---

## `GET /api/health`

```json
{ "status": "ok", "version": "0.1.0" }
```

---

## `POST /api/asm/assemble`

Assemble Intel-syntax source.

```json
{ "source": "mov rax, 1\nret", "origin": 0 }
```

```json
{
  "hex": "48c7c001000000c3",
  "origin": 0,
  "labels": { "_start": 0 },
  "lines": [
    { "line": 1, "address": 0, "hex": "48c7c001000000", "text": "mov rax, 1" },
    { "line": 2, "address": 7, "hex": "c3", "text": "ret" }
  ]
}
```

`lines` drives the side-by-side source/machine-code view. A source line that
emits nothing (a label, a comment, `bits 64`) has no entry.

---

## `POST /api/asm/disassemble`

```json
{ "hex": "488b442408", "base": 4198400 }
```

```json
{
  "instructions": [
    {
      "ip": 4198400,
      "hex": "488b442408",
      "text": "mov rax, qword [rsp+0x8]",
      "description": "copy qword [rsp+0x8] into rax",
      "length": 5,
      "mnemonic": "mov",
      "branchTarget": null,
      "fallsThrough": true
    }
  ],
  "error": null
}
```

Linear sweep. If a byte sequence fails to decode, the instructions decoded so
far are returned and `error` describes where it stopped — a partial answer is
more useful than none, and where the sweep derails is itself the lesson.

---

## `POST /api/asm/explain`

Byte-by-byte breakdown of one instruction. This is what the instruction-decoder
visualisation renders.

```json
{ "hex": "488b442408" }
```

```json
{
  "text": "mov rax, qword [rsp+0x8]",
  "length": 5,
  "fields": [
    { "name": "REX", "hex": "48", "offset": 0,
      "explanation": "REX: W=1 (operand size 64-bit), R=0 (reg field +8), X=0 (index +8), B=0 (rm/base +8)" },
    { "name": "opcode", "hex": "8b", "offset": 1,
      "explanation": "opcode: selects the operation" },
    { "name": "ModRM", "hex": "44", "offset": 2,
      "explanation": "ModRM: mod=01 reg=000 rm=100" },
    { "name": "SIB", "hex": "24", "offset": 3,
      "explanation": "SIB: scale=1 index=100 base=100" },
    { "name": "displacement", "hex": "08", "offset": 4,
      "explanation": "1-byte signed displacement, little-endian" }
  ]
}
```

---

## `POST /api/emu/run`

Assemble (or accept) code, execute it, and return the full trace. The emulator
is a plain interpreter over `asm-core`'s decoder; it has no host access beyond
the syscalls listed below.

```json
{
  "source": "mov eax, 1\nadd eax, 2\nhlt",
  "maxSteps": 1000,
  "stdin": ""
}
```

Either `source` (assembled first) or `hex` must be given.

```json
{
  "stop": { "kind": "halted" },
  "steps": 3,
  "stdout": "",
  "final": { "registers": { "rax": 3, "rsp": 8388608, "...": 0 },
             "rip": 8, "flags": { "zf": false, "cf": false, "sf": false, "of": false, "pf": true, "af": false } },
  "trace": [
    {
      "ip": 0,
      "text": "mov eax, 0x1",
      "regWrites":  [ { "reg": "rax", "before": 0, "after": 1 } ],
      "memWrites":  [],
      "flagsAfter": { "zf": false, "cf": false, "sf": false, "of": false, "pf": false, "af": false }
    }
  ]
}
```

`stop.kind` is one of `halted`, `exited` (with `code`), `stepLimit`,
`fault` (with `reason` and `address`), `breakpoint`.

A 32-bit register write is reported against its 64-bit name with the
zero-extension already applied, because that is what the machine does.

Supported syscalls: `write` (1) to fd 1 and 2, `exit` (60), `exit_group` (231).
Everything else faults, deliberately: a lesson about syscalls should not be able
to open a file.

---

## `POST /api/emu/step`

Stateless single-step. The client sends the machine state, the server returns
the next one. Keeping the state on the client means no sessions, no expiry, and
a trivially shareable URL for any point in an execution.

```json
{ "hex": "...", "base": 0, "state": { "registers": {...}, "rip": 0, "flags": {...}, "memory": [...] } }
```

Response is one `trace` entry plus the resulting `state`.

---

## `POST /api/binfmt/inspect`

Parse an executable. Accepts `multipart/form-data` with a `file` part, or JSON
`{ "hex": "..." }` for small inputs. Maximum 16 MiB.

```json
{
  "format": "elf",
  "arch": "x86_64",
  "entry": 4198400,
  "sections": [ { "name": ".text", "address": 4198400, "size": 500, "offset": 4096,
                  "flags": ["alloc", "execute"] } ],
  "segments": [ { "type": "load", "vaddr": 4194304, "memsz": 8192, "perms": "r-x" } ],
  "symbols": [ { "name": "_start", "address": 4198400, "size": 0, "kind": "func", "binding": "global" } ],
  "imports": [ { "name": "puts", "library": "libc.so.6", "kind": "function" } ],
  "exports": [],
  "relocations": [ { "offset": 4210688, "kind": "R_X86_64_JUMP_SLOT", "symbol": "puts", "addend": 0 } ]
}
```

Unsupported or malformed files return `400` with a specific reason. The parser
never panics on hostile input; it is fuzzed.

---

## `GET /api/lessons`

The curriculum index.

```json
{
  "parts": [
    { "number": 1, "title": "Computer Fundamentals",
      "lessons": [ { "id": "binary", "title": "Binary", "order": 1,
                     "objectives": ["..."], "exerciseCount": 4 } ] }
  ]
}
```

## `GET /api/lessons/{id}`

One lesson: metadata, the rendered markdown body, its examples, and its
exercises (without the answers).

## `POST /api/lessons/{id}/exercises/{exerciseId}/check`

Grade a submission. The answer key never leaves the server.

```json
{ "answer": "mov rax, 1" }
```

```json
{ "correct": true, "message": "...", "hints": [] }
```

For `assemble` exercises the submission is source, and it is checked by
assembling it and comparing against the expected bytes — not against the
expected text, so any correct encoding passes. For `emulate` exercises the
submission is run and its final state is compared. For `quiz` exercises the
submission is the choice index.
