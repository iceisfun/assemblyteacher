+++
id = "iat-and-imports"
title = "The Import Address Table"
order = 2
estimated_minutes = 40
objectives = [
  "Explain why a PE cannot hard-code the address of a DLL function, and how the import table solves it",
  "Trace the two parallel arrays — the lookup table and the IAT — and say what each holds before and after loading",
  "Recognise a call through the IAT in disassembly and contrast it with an ELF PLT/GOT call",
  "Describe delay loading and binding as the Windows answers to lazy resolution",
  "Explain why an import named api-ms-win-*.dll need not exist on disk, and how the ApiSetMap redirects it at load time",
  "Read an import list as analysis signals — treating each observation as one signal to weigh, not as proof",
]
prerequisites = ["got-and-plt", "pe-disk-to-memory"]

[[exercises]]
id = "q-why-iat"
kind = "quiz"
prompt = "A Windows program calls `MessageBoxA`, which lives in `user32.dll`. Why can the compiler not emit `call <address of MessageBoxA>`?"
choices = [
  "MessageBoxA is written in C++",
  "user32.dll is a separate image, loaded at an address not known until runtime (and randomised by ASLR), so its function addresses cannot be baked into the .exe",
  "The call would violate W^X",
  "MessageBoxA is too large to call directly",
]
answer = 1
explanation = "user32.dll is its own PE, mapped at load time at an address the .exe cannot predict — ASLR moves it, and even the set of loaded DLLs is a runtime fact. This is the same reason ELF needs the GOT. The address must come from a table the loader fills in, not from an immediate baked into the code."

[[exercises]]
id = "q-two-arrays"
kind = "quiz"
prompt = "Each imported DLL has TWO parallel arrays in the import descriptor: the Import Lookup Table (ILT, also called the name table) and the Import Address Table (IAT). Why two, when they start out identical?"
choices = [
  "One is for 32-bit and one for 64-bit code",
  "The ILT is a read-only record of *what* to import (names/ordinals) that survives loading; the IAT starts as the same list but the loader *overwrites* it in place with resolved addresses",
  "The ILT holds exports and the IAT holds imports",
  "They are redundant; linkers emit two by mistake",
]
answer = 1
explanation = "Both arrays begin holding the same entries — each pointing at a hint/name (or an ordinal). The loader reads the ILT to learn *what* each slot wants, resolves it, and writes the real function address into the matching IAT slot, destroying the names there. Keeping the ILT untouched leaves a clean record of the imports after the IAT has been overwritten — which is exactly what a disassembler reads to label the calls."

[[exercises]]
id = "q-hint-name"
kind = "quiz"
prompt = "An import-by-name entry points at a hint/name structure: a 2-byte 'hint' followed by the function's ASCII name. What is the hint?"
choices = [
  "A checksum of the name",
  "A guessed index into the DLL's export table — the loader tries it first and falls back to a binary search by name if it is wrong",
  "The function's ordinal, which replaces the name",
  "The number of arguments the function takes",
]
answer = 1
explanation = "The hint is a best-guess index into the target DLL's export name table. The loader checks whether the export at that index has the matching name; if so it skips the search entirely. If the guess is stale (the DLL changed), it falls back to a binary search. It is a pure speed optimisation — the name is the source of truth."

[[exercises]]
id = "d-iat-call"
kind = "disassemble"
prompt = "This is how compiled code calls an imported function: an indirect call through an IAT slot. `ff 15 e2 2f 00 00`. What is it?"
hex = "ff15e22f0000"
expect_text = "call qword [rip+0x2fe2]"
hints = [
  "`ff /2` is an indirect `call`; the ModRM byte `15` selects the RIP-relative memory form.",
  "The target is *read from* the IAT slot at `[rip+0x2fe2]`, whose contents the loader wrote — compare the ELF PLT's `jmp qword [rip+...]` through the GOT.",
]

[[exercises]]
id = "q-api-set"
kind = "quiz"
prompt = "You are analysing a PE and its import table lists `api-ms-win-core-memory-l1-1-0.dll`, but that file is nowhere on the disk. What is going on?"
choices = [
  "The binary is corrupt — the import points at a missing file",
  "It is an API Set: a virtual contract name, not a physical DLL. The loader consults the ApiSetMap during process init and redirects it to a real implementation DLL (often KernelBase.dll), which version varies by Windows release",
  "The DLL was deleted by malware to hide its tracks",
  "It is an encrypted DLL that decrypts itself at runtime",
]
answer = 1
explanation = "Names like api-ms-win-core-*-l1-1-0.dll are API Set contracts, not files. Modern Windows decouples the logical API surface from the DLL that implements it: at process initialisation the loader reads the ApiSetMap and rewrites the contract name to whatever DLL provides it on this Windows version (commonly KernelBase.dll). The redirection happens before ordinary import resolution, which is why the 'file' never appears on disk. Compilers and the SDK emit these for everyday programs, so their presence says nothing on its own."

[[exercises]]
id = "q-protect-signal"
kind = "quiz"
prompt = "During analysis you observe a code region's protection change from executable-only to writable and then, shortly after, back to its original protection. How should you weigh that observation?"
choices = [
  "As proof the program is malicious",
  "As one signal worth a closer look, not evidence by itself — legitimate software (JIT compilers, instrumentation, debuggers, security products) performs the same operation, so it must be corroborated",
  "As irrelevant; protection changes are never interesting",
  "As proof the program is a JIT compiler",
]
answer = 1
explanation = "A brief transition from a settled protection to writable and back is uncommon in typical business applications and may warrant closer inspection during malware analysis. But legitimate software — JIT compilers, garbage collectors, allocators, debuggers, hot-patchers, emulators, anti-cheat — performs similar operations, so the observation is one signal to weigh among others, not evidence of malicious behaviour on its own. Good analysis corroborates; it does not convict on a single API."
+++

# The Import Address Table

The previous part ended with a Windows executable mapped into memory, its
sections placed and its data directories waiting to be walked. One of those
directories, slot 1, is the **import table**, and it answers the same question
the ELF GOT/PLT lesson did: your program calls `printf`, or `MessageBoxA`, or
`CreateFileW` — functions that live in a *different* file, loaded at an address
you cannot know when you compile. How does a `call` reach them?

Windows and ELF give the same shaped answer — an indirection table the loader
fills in — with different machinery. If you have read the GOT/PLT lesson, you
already know the plot; this is the Windows cast.

## The problem, restated for DLLs

`user32.dll` exports `MessageBoxA`. Your `.exe` wants to call it. But:

- The DLL is a separate PE, mapped at load time. ASLR randomises where.
- Which DLLs load, and in what order, is a runtime fact.
- So the address of `MessageBoxA` does not exist until the process is built.

A baked-in `call 0x00007FFname...` would be wrong on the next boot. The compiler
instead emits a call that reads its target *from a table slot*, and leaves the
job of filling that slot to the loader. The table is the **Import Address
Table**, the IAT.

## Two parallel arrays

The import directory has one descriptor per DLL you depend on. Each descriptor
names the DLL (`"USER32.dll"`) and points at **two arrays that start out
identical**:

```text
  Import Lookup Table (ILT)        Import Address Table (IAT)
  ─────────────────────────        ─────────────────────────
  → hint/name "MessageBoxA"        → hint/name "MessageBoxA"
  → hint/name "GetMessageW"        → hint/name "GetMessageW"
  → ordinal   0x0072               → ordinal   0x0072
  → 0  (null terminator)           → 0  (null terminator)
```

Before loading, both point at the same *descriptions* of what to import: each
entry is either a pointer to a **hint/name** structure (a 2-byte hint plus the
ASCII function name) or, with the high bit set, a bare **ordinal** (import by
number, no name). Why keep two copies of the same list?

Because the loader is about to destroy one of them. It walks the IAT, resolves
each entry to the real runtime address of the function, and **overwrites the IAT
slot in place** with that address. After loading, the IAT is an array of live
function pointers and the names that were there are gone. The **ILT survives
untouched** — a permanent record of what each slot was supposed to be. That is
why a disassembler can still label a call `MessageBoxA` long after the IAT slot
became a raw pointer: it reads the name from the ILT.

## What a call looks like

Compiled code reaches an import with a single indirect call *through* the IAT
slot:

```asm
    call qword [rip+0x2fe2]   ; call [IAT slot for MessageBoxA]
```

The bytes are `ff 15 ..` — `ff /2`, the indirect-call form, with a RIP-relative
operand. The CPU loads 8 bytes from the IAT slot and calls that address.
Recognising this is a core reversing skill: **an indirect `call qword [rip+…]`
whose target lands in the imports section is a call into a DLL**, and the ILT
tells you which function. Contrast the ELF PLT, which used the same `ff /` family
but as a `jmp` through the GOT inside a per-function stub; Windows usually skips
the stub for calls and jumps straight through the IAT slot the compiler
referenced.

The `__declspec(dllimport)` you may have seen in Windows headers is exactly the
compiler's promise to emit this `call [slot]` form instead of a direct call —
telling it "this symbol lives in another image, go through the table."

## When the import isn't a file: API Sets

Read the imports of almost any modern PE and you will find entries like these:

```text
  api-ms-win-core-memory-l1-1-0.dll
  api-ms-win-core-synch-l1-2-0.dll
  api-ms-win-core-file-l2-1-0.dll
```

The natural assumption — that these are DLLs on disk — is usually wrong. Go
looking in `System32` and you often will not find them. They are **API Sets**:
logical *contract* names, not physical files.

Historically a program imported `kernel32.dll`, `user32.dll`, and their siblings
directly, and those files really existed. Modern Windows added a layer of
indirection between the API surface and the DLL that implements it. An API Set
name like `api-ms-win-core-memory-l1-1-0.dll` names a *contract* — "the level-1
core memory API, version 1.0" — and says nothing about which file provides it.

The resolution happens early. During process initialisation, before ordinary
import resolution, the loader consults the process's **ApiSetMap** (a table the
system supplies) and rewrites each contract name to the real implementation DLL:

```text
  api-ms-win-core-memory-l1-1-0.dll   ──ApiSetMap──►   kernelbase.dll
```

Three things matter to a reverse engineer:

- **These are virtual names.** An import that resolves fine at runtime can have
  no matching file on disk; do not treat a "missing" API-Set DLL as suspicious.
- **The redirection is version-dependent.** The same contract may map to
  different implementation DLLs on different Windows releases, so the file an
  import ends up in is not fixed.
- **Everyone uses them.** The compiler and SDK emit API-Set imports for ordinary
  programs; malware, packers and business apps alike carry them. Their presence
  is background noise, not a signal.

This is the main reason an import table does not always correspond to files on
disk — and knowing it saves you from chasing a DLL that was never meant to exist.

## Eager by default; delay loading for lazy

The ELF lesson made a point of *lazy binding* — the GOT slot resolved on first
use through the PLT resolver. Windows chose the opposite default: the loader
resolves **every** import **eagerly**, before your entry point runs. Simpler and
predictable, at the cost of resolving functions you might never call.

For programs that import a large DLL they rarely touch, Windows offers
**delay-loaded imports**: the DLL is not loaded until the first call, and a small
helper stub (`__delayLoadHelper2`) does the resolution on demand and patches the
slot — the same lazy-binding idea as the PLT, now opt-in rather than the default.

There is also **binding** (the old `bind.exe`): pre-computing import addresses
into the IAT on disk, betting the target DLLs load at their preferred bases so
the loader can skip resolution. ASLR made that bet almost always lose — the bases
move — so bound imports are largely historical, but you will still meet the
timestamp fields that supported them.

## The IAT as a target

Because the IAT is a table of function pointers that *executing code reads to
decide where to jump*, an attacker who can write to it redirects those calls —
**IAT hooking**. It is the Windows echo of the GOT-overwrite from the last
lesson, and the same class of tool answers it: keep the pointer tables read-only
after the loader has filled them. It is also, turned around, a standard
*instrumentation* technique — debuggers, sandboxes and API monitors hook the IAT
deliberately to watch or reroute a program's library calls. Point the Inspector
at a PE and it lists each import, its library, and its IAT slot address; that
list is both the map a reverser reads and the surface a hooker writes.

## Reading the import list as signals

The import table is not only a mechanism; it is the first thing an analyst reads,
because *what a program imports hints at what it can do*. A binary that imports
only `CreateFileW`, `ReadFile` and `printf`-style functions has a small
vocabulary; one that also imports `GetProcAddress`, `LoadLibraryW` and the memory
functions below has the vocabulary to assemble behaviour at runtime that its
static call graph does not show. Neither is proof of anything — it is context you
carry into the rest of the analysis.

One import worth understanding in its own right is **`VirtualProtect`**. The
virtual-memory lesson established that every page carries read/write/execute
permission bits the MMU checks on every access, and that the **W^X** rule keeps
no page both writable and executable. `VirtualProtect` is the Windows API that
**changes the protection of an existing region of virtual memory**. Windows names
the protection with `PAGE_*` constants that are just combinations of those same
R/W/X bits:

| constant | meaning |
|---|---|
| `PAGE_READONLY` | read only |
| `PAGE_READWRITE` | read + write, not executable |
| `PAGE_EXECUTE` | execute only |
| `PAGE_EXECUTE_READ` | read + execute — a normal code page |
| `PAGE_EXECUTE_READWRITE` | read + write + execute — the W^X-violating combination |
| `PAGE_GUARD` | a one-shot tripwire: the next access faults, then the flag clears |

These are fundamental operating-system concepts, and legitimate software changes
page protection constantly. A **JIT compiler** writes machine code into a buffer
and then must make it executable; **garbage collectors** and **memory
allocators** flip protections to track or protect pages; **debuggers**,
**instrumentation frameworks**, **hot-patchers**, **emulators**, and **anti-cheat
and security products** all legitimately do the same. `PAGE_GUARD` in particular
is how a thread stack detects that it needs to grow. So the *presence* of
`VirtualProtect` in an import table is ordinary.

What draws an analyst's eye is a *pattern* in how protections move at runtime:

> A transition from read-only (or execute-only) to writable, followed shortly by
> restoration of the original protection, is uncommon in typical business
> applications and may warrant closer inspection during malware analysis.
> Legitimate software such as JIT compilers, instrumentation frameworks,
> debuggers, and security products also perform similar operations, so the
> observation should be considered **one signal rather than evidence** of
> malicious behaviour.

That is the whole discipline in one sentence: describe the *observable* — a
region briefly became writable and then went back — weigh it against how common
it is in benign software, and corroborate before concluding. An analyst learns
*what to look for*; the meaning comes from the surrounding behaviour, not from
the single API. (This lesson stays at the level of what you can observe in a
binary; the actual mechanics of modifying running code belong to a different
discussion and are not the subject here.)

## Key points

- A PE calls DLL functions through the **IAT**, a table of pointers the loader
  fills in — the same indirection the ELF GOT provides, because the problem
  (addresses unknown until load) is the same.
- Each imported DLL has two parallel arrays: the **ILT** (a durable record of the
  names/ordinals to import) and the **IAT** (the same list, overwritten in place
  with resolved addresses). The surviving ILT is how tools still name the calls.
- A `call qword [rip+…]` landing in the imports is a call into a DLL. That single
  pattern unlocks most of the library calls in a Windows disassembly.
- Windows resolves imports **eagerly** by default; **delay loading** is the
  opt-in lazy path, and **binding** is the mostly-dead precomputation ASLR killed.
- **API Sets** (`api-ms-win-*`) are virtual contract names, not files; the loader
  rewrites them via the **ApiSetMap** to a real implementation DLL (version
  dependent). They are why an import need not match a file on disk, and their
  presence signals nothing on its own.
- The import list is an analyst's first read: it hints at a program's
  vocabulary. `VirtualProtect` changes an existing region's page protection
  (the `PAGE_*` R/W/X combinations); legitimate software does this constantly, so
  a protection change is **one signal to weigh, not evidence** — corroborate
  before concluding.
