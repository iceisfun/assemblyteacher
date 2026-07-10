+++
id = "elf-disk-to-memory"
title = "ELF: From Disk to Memory"
order = 1
estimated_minutes = 35
objectives = [
  "Explain the difference between a section and a segment, and who uses each",
  "Convert between a file offset, a virtual address, and an RVA",
  "Describe what the loader does to turn a file on disk into a running image",
  "Recognise position-independent code and say why RIP-relative addressing makes it cheap",
]
prerequisites = ["addressing-modes", "the-stack"]

[[exercises]]
id = "q-section-vs-segment"
kind = "quiz"
prompt = "A section (like `.text`) and a segment (a PT_LOAD entry) can describe the same bytes. Who is each one *for*?"
choices = [
  "Sections are for the linker and debugger; segments are for the loader",
  "Sections are for the loader; segments are for the linker",
  "They are the same thing under two names",
  "Sections are on disk; segments are only in memory",
]
answer = 0
explanation = "The linker and debuggers work in terms of named sections (.text, .data, .rodata). The loader ignores section names entirely and maps PT_LOAD *segments*, each of which groups sections that share the same permissions. A stripped binary can drop its section table and still run — the loader never needed it."

[[exercises]]
id = "q-rva"
kind = "quiz"
prompt = "A symbol sits at file offset 0x1240. Its section is loaded at virtual address 0x401000 from file offset 0x1000. What virtual address does the symbol end up at?"
choices = ["0x401240", "0x402240", "0x401040", "0x400240"]
answer = 2
explanation = "The section's bytes move as a block. The symbol is 0x1240 − 0x1000 = 0x240 into the section, and the section loads at 0x401000, so the symbol lands at 0x401000 + 0x240 = 0x401040. That 0x240 is the symbol's offset within its mapped region."

[[exercises]]
id = "q-bss"
kind = "quiz"
prompt = "The `.bss` section (zero-initialised globals) has a size in memory but takes up almost no space in the file. Why?"
choices = [
  "It is compressed on disk",
  "Its contents are all zero, so the loader just maps zeroed pages — there is nothing to store",
  "It is loaded from a separate file",
  "It only exists in debug builds",
]
answer = 1
explanation = "A segment has a file size and a (possibly larger) memory size. The loader maps the file bytes, then zero-fills the rest. Since .bss is all zeroes, storing them would be wasteful — the file records only the size, and the loader supplies the zeroes."

[[exercises]]
id = "d-rip-global"
kind = "disassemble"
prompt = "This is how position-independent code reaches a global variable: `48 8d 05 10 00 00 00`. What is the instruction? (It computes an address relative to the next instruction.)"
hex = "488d0510000000"
expect_text = "lea rax, [rip+0x10]"
hints = [
  "`48` is REX.W, `8d` is `lea`, and the ModRM `05` selects the RIP-relative form.",
  "The address is measured from the *end* of this instruction — the linker fills in the displacement so it never depends on where the image loaded.",
]
+++

# ELF: From Disk to Memory

An executable on disk is not a running program. It is a *description* of one: a
set of bytes, plus instructions to the operating system for turning those bytes
into an address space. The ELF format — Executable and Linkable Format, used on
Linux and most Unix systems — is that description. This lesson is about the gap
between the file and the process, and how the loader closes it.

You can follow along in the **Inspector**: drag any Linux binary in and it shows
the headers, sections, and segments described below.

## Two views of the same bytes

ELF describes its contents twice, for two different audiences.

The **section** view is the linker's. Sections have names and purposes:

```text
  .text     executable code
  .rodata   read-only constants (string literals, jump tables)
  .data     initialised writable globals
  .bss      zero-initialised writable globals (occupies no file space)
  .symtab   the symbol table       .debug_*  debugging information
```

The **segment** view is the loader's. A segment (a `PT_LOAD` program header)
groups whatever sections share a permission set into one span to be mapped:

```text
  segment 1  r-x   ← .text and .rodata: read + execute, never written
  segment 2  rw-   ← .data and .bss:    read + write, never executed
```

The loader does not care that `.text` is called `.text`. It reads the program
headers, maps each segment at its virtual address with its permissions, and
jumps to the entry point. This is why you can `strip` a binary — throw away the
entire section table and the symbol names — and it still runs perfectly: the
loader never consulted them. It is also the first wall you hit reverse
engineering a stripped binary: the code is all there, but nothing is labelled.

Notice the permission split is exactly **W^X** from the memory lesson: no
segment is both writable and executable. That is not an accident of layout; it
is the NX defence, baked into how the file is organised.

## Three ways to name a location

The same byte has up to three addresses, and confusing them is a rite of
passage:

| name | what it is | used by |
|------|-----------|---------|
| **file offset** | how many bytes into the file | reading the file on disk |
| **virtual address (VA)** | where the byte lives once loaded | the running program |
| **RVA** | VA minus the image base | relocations, position independence |

A section is a contiguous block that moves as a unit. If its file offset is
`0x1000` and it loads at VA `0x401000`, then every byte keeps its position
*within* the section: the byte at file offset `0x1240` lands at VA `0x401040`,
because it is `0x240` into a section that now starts at `0x401000`. Converting
between the two is just "subtract the section's file offset, add its virtual
address." The Inspector shows both numbers for every section so you can do the
arithmetic by eye.

## What the loader actually does

Turning the file into a process is a short list:

```text
  1. read the ELF header, find the program header table
  2. for each PT_LOAD segment:
       mmap it at its virtual address, with its permissions
       map file_size bytes from the file; zero-fill up to mem_size
  3. if the file names a dynamic linker (PT_INTERP), hand off to it
       — it maps shared libraries and resolves imports (next chapter)
  4. set up the initial stack (argc, argv, environment, auxiliary vector)
  5. jump to e_entry, the entry point
```

Step 2 is where `.bss` gets its zeroes for free: the segment's memory size is
larger than its file size, and the loader fills the difference with zero pages.
Step 2 is also where copy-on-write from the memory lesson earns its keep — the
read-only code segment is shared, page for page, across every process running
the same binary.

## Position independence

An old executable was linked to load at one fixed address, say `0x400000`, and
its code contained absolute addresses that were only correct there. That is
fatal for ASLR, which wants to load the image *somewhere random*.

The fix is **position-independent code**: code that contains no absolute
addresses at all. Instead of "load the global at `0x404028`", it says "load the
global at `rip + 0x2fe0`" — a distance from the current instruction. Distances
within an image never change no matter where it loads, so the loader has nothing
to patch.

The instruction in the exercise below, `lea rax, [rip+0x10]`, is the shape you
will see everywhere in modern binaries: reach a global by its offset from the
next instruction, not by its address. It is the same RIP-relative addressing
from the addressing-modes lesson, and it is the machinery that lets a PIE binary
load at a fresh random base every time it runs.

## Key points

- ELF describes its bytes as **sections** (for the linker and debugger) and as
  **segments** (for the loader). A stripped binary keeps its segments and runs
  fine.
- Every byte has a file offset, a virtual address, and an RVA; convert by
  shifting relative to the section's start.
- The loader maps each segment with its permissions, zero-fills `.bss`, sets up
  the stack, and jumps to the entry point.
- Position-independent code names data by its distance from `rip`, so the image
  can load anywhere — which is what makes ASLR practical.
