+++
id = "pe-disk-to-memory"
title = "PE: From Disk to Memory"
order = 2
estimated_minutes = 35
objectives = [
  "Walk a PE file from the MZ stub through e_lfanew to the PE signature and the optional header",
  "Convert between a file offset, an RVA, and a virtual address using ImageBase and the section table",
  "Explain what the data directories are and why the loader reads them instead of section names",
  "Contrast PE with ELF: the same problem — describe a process — solved with different words",
]
prerequisites = ["elf-disk-to-memory"]

[[exercises]]
id = "q-mz-pe"
kind = "quiz"
prompt = "Every PE file begins with the two bytes `MZ` and a small DOS program. Why is that still there on a 64-bit Windows executable in 2020s?"
choices = [
  "Windows still runs the DOS stub to bootstrap the program",
  "It is a backward-compatible header: the DOS stub only prints 'This program cannot be run in DOS mode', while a field in it (e_lfanew) points to the real PE header",
  "It stores the program's icon",
  "It is the entry point of the program",
]
answer = 1
explanation = "The leading `MZ` and the tiny DOS program are a fossil kept for compatibility. The DOS stub just prints a message if someone runs the file under DOS. What matters to Windows is the 4-byte field `e_lfanew` at offset 0x3C, which holds the file offset of the real `PE\\0\\0` signature. The loader reads MZ, jumps to e_lfanew, and everything modern starts there."

[[exercises]]
id = "q-rva-imagebase"
kind = "quiz"
prompt = "A PE has ImageBase 0x140000000 and a function at RVA 0x1500. The image actually loads at 0x7FF6_0000_0000. What virtual address does the function have at runtime?"
choices = ["0x140001500", "0x7FF600001500", "0x7FF600000000", "0x1500"]
answer = 1
explanation = "An RVA is an offset from wherever the image's base lands — it is base-independent by design. ImageBase (0x140000000) is only the *preferred* base; ASLR moved the actual base to 0x7FF6_0000_0000. The function sits at actual_base + RVA = 0x7FF6_0000_0000 + 0x1500 = 0x7FF6_0000_1500. Add the RVA to the real base, never to the preferred one."

[[exercises]]
id = "q-data-directories"
kind = "quiz"
prompt = "The PE optional header ends with an array of 16 'data directories' (each an RVA + size). What are they for?"
choices = [
  "They list the CPU features the program needs",
  "They are a fixed-index table of pointers to the structures the loader needs — imports, exports, base relocations, TLS, exception data — so the loader finds them without walking section names",
  "They hold the program's command-line arguments",
  "They are debugging symbols",
]
answer = 1
explanation = "The data directories are the loader's index. Slot 1 is the import table, slot 0 the export table, slot 5 the base relocations, slot 9 the TLS directory, and so on — each a well-known index holding an RVA and a size. Like ELF's program headers, this lets the loader reach what it needs by structure, not by a section called `.idata`. Section names are for humans; the directories are for the machine."

[[exercises]]
id = "d-pie-mov"
kind = "disassemble"
prompt = "Modern PEs are position-independent too. This reaches a global the base-independent way: `48 8b 05 10 00 00 00`. What is the instruction?"
hex = "488b0510000000"
expect_text = "mov rax, qword [rip+0x10]"
hints = [
  "`48` is REX.W, `8b` is the load form of `mov`, and the ModRM `05` selects RIP-relative addressing.",
  "The displacement is measured from the end of this instruction, so it is correct no matter where ASLR placed the image — the same trick the ELF lesson showed with `lea`.",
]
+++

# PE: From Disk to Memory

The last lesson took an ELF file apart and watched the loader turn it into a
process. Windows solves the exact same problem — a file on disk is a
*description* of an address space, and something has to build the real thing —
but it uses a different format with different names: the **PE** (Portable
Executable) format. `.exe`, `.dll`, `.sys`, and `.obj` files are all PE. Once
you have read ELF, PE holds no new *ideas*; it is the same concepts wearing a
Windows vocabulary. This lesson is the translation.

Drag any Windows `.exe` or `.dll` into the **Inspector** and it shows the
headers, sections, imports, and relocations described below, next to the same
fields for an ELF — the point is that they line up.

## The walk in from `MZ`

A PE file opens with a relic. The first two bytes are `MZ` (the initials of Mark
Zbikowski, who designed the DOS executable format), followed by a tiny DOS
program — the *DOS stub* — whose entire job is to print "This program cannot be
run in DOS mode" if you ever run the file on DOS. Nobody has done that on purpose
in decades, but the stub stays for compatibility.

What matters is a 4-byte field at file offset `0x3C` called **e_lfanew**. It
holds the file offset of the real header, marked by the signature `PE\0\0`. So
the loader's first move is a redirection:

```text
  offset 0x00   "MZ"  ... DOS stub ...
  offset 0x3C   e_lfanew ───────────┐
                                     ▼
  offset e_lfanew   "PE\0\0"  COFF header   optional header   section table
```

After the signature come two headers. The **COFF file header** is small: the
target machine (0x8664 for x86-64), the number of sections, and characteristic
flags. The **optional header** — misnamed, it is mandatory for an image — is the
important one. It carries the entry point (as an RVA), the **ImageBase**, the
section alignment, the subsystem (console vs GUI), and, at its tail, the *data
directories*.

## RVA, ImageBase, and the same three names

ELF gave every byte a file offset, a virtual address, and an RVA. PE has all
three, and leans on the RVA even harder — almost every pointer inside a PE is
stored as an RVA.

| name | PE meaning |
|------|-----------|
| **file offset** | how many bytes into the file on disk |
| **RVA** | offset from the image's base once loaded |
| **VA** | actual runtime address = actual base + RVA |
| **ImageBase** | the base the file would *prefer* to load at |

The trap is `ImageBase`. It is only a *preference*. A DLL wants to load at, say,
`0x180000000`, but if that address is taken — or ASLR is on — the loader puts it
somewhere else and the whole image slides. RVAs survive that slide untouched
because they are relative; a stored VA would not, which is why PE stores RVAs
everywhere and computes VAs late. Converting is one addition: `VA = actual_base
+ RVA`. Add to the *actual* base, never the preferred one — the exercise above
is exactly this mistake waiting to happen.

## Sections, and permissions by another name

PE sections are ELF sections with different names and their permissions stored
per-section rather than grouped into segments:

```text
  .text    r-x   executable code          (ELF .text)
  .rdata   r--   read-only data, imports   (ELF .rodata)
  .data    rw-   initialised globals       (ELF .data)
  .bss      -    zero-init globals, no file bytes (ELF .bss)
  .reloc   r--   base relocations          (next lesson)
  .rsrc    r--   resources: icons, version, manifests
```

Each section header carries **characteristics** — flag bits for readable,
writable, executable — and the loader maps the section with those permissions.
There is no separate "segment" table as in ELF: in PE the section *is* the unit
the loader maps, and its characteristics *are* its permissions. The result is
the same **W^X** split you saw in ELF — `.text` executes but cannot be written,
`.data` is written but cannot execute — because a section that is both is exactly
the NX violation the loader is built to avoid. The Inspector shows the
`alloc/write/execute` triple for every PE section, the same column it shows for
ELF.

Because `.bss` is all zeroes, it has a virtual size but almost no file size —
the loader maps zero-filled pages for it, precisely as it zero-fills the gap
above an ELF segment's file bytes. Same free zeroes, same reason.

## The data directories: the loader's index

Here is the one PE structure with no exact ELF twin, and it is worth
understanding. The optional header ends with an array of 16 **data directories**,
each just an `(RVA, size)` pair at a fixed index:

```text
  [0]  Export Table          [5]  Base Relocation Table
  [1]  Import Table          [9]  TLS Directory
  [2]  Resource Table       [12]  Import Address Table
  ...
```

This is how the loader finds the structures it must process without ever reading
a section name. Need the imports? Read directory slot 1 — it gives the RVA of the
import descriptors. Need the relocations? Slot 5. Section names like `.idata` or
`.reloc` are a convenience for humans and disassemblers; a linker can merge the
import data into `.rdata` and name nothing, and the loader still finds it,
because it navigates by directory index. It is the same lesson ELF taught with a
stripped binary: the loader works off structural tables, not labels.

The next two lessons are each a walk through one of these directories — the
**import table** (how a PE calls into a DLL) and the **base relocation table**
(how the loader rebases an image when it cannot honour ImageBase).

## What the Windows loader does

The shape of the job is identical to ELF's, step for step:

```text
  1. read MZ, follow e_lfanew to the PE header and optional header
  2. reserve the address range and map each section at its RVA,
       with its characteristics as permissions; zero-fill .bss
  3. if the image did not load at ImageBase, apply base relocations
       (directory 5) to patch every absolute address  — next lesson
  4. walk the import table (directory 1): load each named DLL and
       write the real function addresses into the IAT  — next lesson
  5. run TLS callbacks (directory 9), then jump to the entry point
```

Compare that with the ELF loader's list from the previous lesson: map segments,
hand off to the dynamic linker, resolve imports, jump to the entry point. The
words differ — "base relocations" for what a PIE avoids with RIP-relative code,
"IAT" for the GOT, "TLS callbacks" for a thing ELF has too but rarely uses —
but the skeleton is the same because the problem is the same.

## Key points

- A PE begins with a compatibility `MZ` DOS stub; the real header lives at the
  offset in `e_lfanew`, behind the `PE\0\0` signature.
- Almost everything inside a PE is an **RVA**. The runtime address is
  `actual_base + RVA`, and the actual base is often *not* the preferred
  `ImageBase` — ASLR or a collision moves it.
- PE **sections** carry their own permission characteristics; there is no
  separate segment table, but the W^X split is the same as ELF's.
- The **data directories** are the loader's fixed-index map to imports, exports,
  relocations, and TLS — navigated by index, not by section name.
- PE and ELF are two vocabularies for one job: describe an address space
  precisely enough that a loader can build it.
