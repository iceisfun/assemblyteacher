+++
id = "iat-and-imports"
title = "The Import Address Table"
order = 2
estimated_minutes = 35
objectives = [
  "Explain why a PE cannot hard-code the address of a DLL function, and how the import table solves it",
  "Trace the two parallel arrays — the lookup table and the IAT — and say what each holds before and after loading",
  "Recognise a call through the IAT in disassembly and contrast it with an ELF PLT/GOT call",
  "Describe delay loading and binding as the Windows answers to lazy resolution",
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
