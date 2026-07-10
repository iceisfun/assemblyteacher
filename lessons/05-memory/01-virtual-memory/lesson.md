+++
id = "virtual-memory"
title = "Virtual Memory and Pages"
order = 1
estimated_minutes = 40
objectives = [
  "Explain why every process is given its own private, flat address space instead of sharing physical RAM directly",
  "Describe how the MMU translates a virtual address to a physical one through a page table, and why real x86_64 uses four levels",
  "State why memory is managed in fixed-size 4 KiB pages and what the per-page R/W/X permission bits buy you",
  "Trace a page fault through its useful roles — demand paging, lazy allocation, copy-on-write, file mmap — and through its fatal one, the SIGSEGV",
]
prerequisites = ["registers"]

[[exercises]]
id = "q-why-virtual"
kind = "quiz"
prompt = "A pointer in your program holds the value `0x5555_5555_1000`. Two copies of the same program run at once, and both dereference that same address without colliding. Why don't they clobber each other?"
choices = [
  "The CPU refuses to let two processes use the same address at the same time",
  "Each process has its own page tables, so the identical virtual address maps to a different physical frame in each",
  "The linker guarantees no two programs ever pick the same address",
  "The second process is silently given a copy-on-write snapshot of the first",
]
answer = 1
explanation = "A virtual address means nothing on its own. It is an index into *this* process's page tables, which the kernel swaps in on every context switch. The same number in two processes walks two different tables and lands on two different physical frames."

[[exercises]]
id = "q-page-size"
kind = "quiz"
prompt = "Why does the hardware translate memory in fixed-size 4 KiB pages rather than tracking each byte, or each variable-sized region, individually?"
choices = [
  "4 KiB is the width of the memory bus, so smaller units cannot be addressed",
  "A fixed page size makes translation a shift-and-index instead of a search, and bounds the size of the mapping tables",
  "Programs are required by the ABI to allocate memory in 4 KiB chunks",
  "Smaller pages would exceed the number of bits in a 64-bit address",
]
answer = 1
explanation = "Fixed pages mean the low 12 bits of an address are the offset within a page and the high bits are a page number — translation is just an array lookup on the page number, no searching. Per-byte or variable-length mappings would need a searchable structure on every single access."

[[exercises]]
id = "q-wx"
kind = "quiz"
prompt = "The page holding your program's machine code is mapped read + execute but not write. The page holding the stack is mapped read + write but not execute. What class of attack does this W^X (\"write XOR execute\") split directly frustrate?"
choices = [
  "Reading another process's memory",
  "Injecting bytes into a writable buffer and then jumping to them as code",
  "Exhausting physical RAM by allocating too much",
  "Guessing the address of a library function",
]
answer = 1
explanation = "The classic exploit writes shellcode into a buffer (writable) and transfers control to it (needs execute). If no page is ever both writable and executable, that one-step path is closed: the buffer the attacker can write is not executable, and the code that is executable cannot be written. NX is the hardware bit that enforces it."

[[exercises]]
id = "q-page-fault"
kind = "quiz"
prompt = "You call `malloc` for 1 GiB, and it succeeds instantly on a machine with 512 MiB of free RAM. You then touch only the first few kilobytes. Nothing swaps, nothing fails. How?"
choices = [
  "malloc compresses the unused region until it is touched",
  "The kernel reserves the address range but maps no physical frames until a page fault on first touch actually demands one",
  "The allocation quietly failed and returned a null pointer",
  "1 GiB fits because virtual addresses are 64 bits wide",
]
answer = 1
explanation = "The mapping is a promise about the address space, not a commitment of RAM. Each page stays unbacked until the first access faults; the fault handler then finds a physical frame and wires it in. Touch only a few pages and only a few frames are ever spent — this is demand paging / lazy allocation."
+++

# Virtual Memory and Pages

Every pointer you have written in this course has been a lie — a useful,
universal, carefully maintained lie. When your program loads `[rax]`, the number
in `rax` is *not* an address of a cell in the DRAM chips soldered to the board.
It is a **virtual address**: an index into a private map that belongs to your
process alone, which hardware translates, on every single access, into the real
physical location. This lesson is about that map, why it exists, and the one
mechanism — the page fault — that turns out to power half of what a modern
operating system does.

## The problem virtual memory solves

Imagine there were no translation, and a pointer named a physical byte directly.
Three problems appear immediately, and none of them is fixable in software:

1. **Two programs cannot both use address `0x1000`.** If your program and a
   second program both want to store something at `0x1000`, one of them
   overwrites the other. The compiler and linker would have to negotiate,
   globally, which physical addresses every program in the system is allowed to
   touch — an impossible arrangement, since programs are written independently
   and run in unpredictable combinations.

2. **Any program could read or corrupt any other.** With direct physical
   addressing there is no wall between processes. A bug in one is a bug in all;
   a malicious program owns the machine.

3. **A program is limited to the RAM that is physically free right now**, and
   must know at compile time where that RAM is. But free RAM is a moving target
   that depends on what else is running.

Virtual memory dissolves all three at once by inserting a layer of indirection.
Each process is handed its own **flat address space**: a clean, contiguous range
of addresses from `0` up to the architectural maximum, as though it were the only
program on a machine with an ocean of memory. The address `0x1000` in your
process and the address `0x1000` in mine are *different locations*, because they
are looked up in different maps. Neither of us can name the other's memory,
because the map simply has no entry pointing there.

The flat space is an abstraction the hardware and kernel conspire to maintain.
Underneath, physical RAM is a scarce, fragmented, shared resource. On top, every
process sees the same tidy fiction. That gap is the whole game.

## The MMU and the page table

The translation happens in hardware, in a unit called the **MMU** (memory
management unit), sitting between the CPU core and the memory bus. Every load and
store the core issues carries a virtual address; the MMU rewrites it to a
physical address before it ever reaches the DRAM.

To do that it consults a **page table** — a data structure in memory, built and
owned by the kernel, that records the virtual-to-physical mapping for the current
process. Conceptually, the simplest possible page table is one big array:

```text
   virtual page number  ->  physical frame number
   ------------------------------------------------
        0                ->  (nothing mapped)
        1                ->  frame 8074
        2                ->  frame 8075
        3                ->  (nothing mapped)
        ...
```

Translation, conceptually, is a single lookup: take the virtual address, split
off a *page number* and an *offset*, use the page number to index the table, and
combine the physical frame it names with the offset. One level of "virtual page →
physical frame," and you are done.

The special register that tells the MMU *which* table to use is, on x86_64,
`CR3`. When the kernel switches from one process to another, it loads that
process's page-table root into `CR3`, and from that instant every virtual address
the core produces is interpreted through the new map. This is the mechanical
heart of the private address space: a context switch is, in large part, a change
of `CR3`.

### Why one flat array is not enough

A single flat array would need one entry for *every* page in the address space,
whether or not it is mapped. A 64-bit space is unimaginably large; an array
covering it would be larger than all the RAM in existence, and almost every entry
would say "nothing mapped." That is absurdly wasteful, because real programs use a
few scattered clumps of a vast, mostly empty space.

So real x86_64 does not use one array. It uses a **four-level tree** of tables.
The virtual address is chopped into four 9-bit indices plus the 12-bit offset:

```text
   63        48 47      39 38      30 29      21 20      12 11         0
  +-----------+----------+----------+----------+----------+------------+
  | (sign ext)|  PML4    |   PDPT   |    PD    |    PT    |   offset   |
  +-----------+----------+----------+----------+----------+------------+
                  9 bits     9 bits     9 bits     9 bits    12 bits
```

Each 9-bit index selects one of 512 entries in a table; that entry points at the
next table down; the last one points at the physical frame. The tree is sparse —
you only allocate the branches you actually use — so a program touching a few
regions needs only a handful of small tables, not one colossal array. The cost is
that a translation is now *four* memory reads deep. The MMU hides that cost with a
cache of recent translations called the **TLB** (translation lookaside buffer);
a TLB hit gives you the physical address in effectively zero time, and only a miss
pays for the walk.

The four levels are an implementation detail. The idea you must hold onto is the
one-level version: **a virtual address is looked up in a per-process table to find
a physical frame.** Everything below is that idea, made sparse and cached.

## Pages: why fixed, why 4 KiB

Notice that the low 12 bits of the address never got translated — they passed
straight through as the *offset*. That is the definition of a **page**: memory is
mapped not one byte at a time but one aligned, fixed-size block at a time, and
`2^12 = 4096` bytes = **4 KiB** is that block on x86_64.

Fixed size is not an arbitrary choice; it is what makes translation cheap:

- Because a page is a power of two and aligned, splitting an address into
  (page number, offset) is a **shift and a mask**, not arithmetic. The high bits
  *are* the page number; the low bits *are* the offset. No division, no search.
- Because every page is the same size, the page table can be a flat indexable
  array (or tree of arrays) — entry *N* describes page *N*. Variable-sized
  regions would force a searchable structure consulted on every memory access,
  which the MMU cannot afford to do at hardware speed.
- Because mappings are page-granular, the kernel's bookkeeping is bounded: it
  tracks frames, not bytes.

4 KiB is a balance. Smaller pages would waste less memory on partially used
regions (less *internal fragmentation*) but need larger page tables and more TLB
entries to cover the same span. Larger pages (x86_64 also offers 2 MiB and 1 GiB
"huge pages") cover more with each TLB entry but round every allocation up to a
coarse grain. 4 KiB has been the sweet spot for decades. The number to remember:
**an address's low 12 bits are its offset within its page; align a pointer down to
a 4 KiB boundary by clearing those bits.**

## Permissions live on the page

A page-table entry does not only say *where* a page lives. It carries
**permission bits** that the MMU checks on every access:

| bit | meaning | violated by |
|-----|---------|-------------|
| present | is this page backed by a frame at all? | touching unmapped memory |
| writable (W) | may the page be written? | storing to a read-only page |
| user | may user-mode code touch it? | user code touching a kernel page |
| NX (no-execute) | is instruction fetch *forbidden* here? | jumping into a data page |

The three that matter to a program are **read, write, execute** — R/W/X — and the
kernel sets them per region according to what the region is *for*:

- Your machine code (the *text* segment) is mapped **read + execute**, not
  writable. Code should never be modified while it runs, and forbidding writes
  catches the bug or the attack that tries.
- Your data, heap, and stack are mapped **read + write**, not executable. They
  hold values, not instructions.
- Read-only constants get **read only**.

### W^X and why NX exists

Put those two rules together and you get an invariant the industry calls
**W^X**, "write xor execute": **no page is ever writable and executable at the
same time.** A page you can write, you cannot run; a page you can run, you cannot
write.

This exists because of a specific, devastating attack. The classic exploit
overflows a buffer to inject its own machine code — *shellcode* — into some
writable region (a stack buffer, say), then redirects control to it. For decades
this worked, because the stack was both writable *and* executable: nothing in the
hardware objected to fetching instructions from it.

The **NX bit** ("no-execute", AMD's name; Intel calls it XD, "execute disable")
is the hardware fix. It is a per-page flag that says "instruction fetch from this
page faults." Mark every writable page NX and the injected-shellcode path dies at
the source: the buffer the attacker can write to is not executable, and the code
pages that *are* executable are not writable. You have seen this bit surfaced by
name — the Inspector reports "NX" among a binary's mitigations, and it is reading
exactly this page-permission story out of the ELF's segment flags.

(W^X does not end exploitation — it pushes attackers toward *reusing* existing
executable code, "return-oriented programming," which is why the next lesson's
topic, ASLR, matters. But it closes the simplest door completely.)

## The page fault: the mechanism behind everything

Here is the pivot of the whole lesson. When the MMU tries to translate an address
and something is wrong — the page is not present, or the access violates its
permissions — it cannot proceed. It raises a **page fault**: a hardware exception
that stops the offending instruction and jumps into a kernel handler, handing it
the faulting address and what went wrong.

A page fault is *not* inherently an error. It is a **trap that lets the kernel run
code at the exact moment a program touches a particular page.** That hook is
extraordinarily powerful, and the kernel uses it to implement features that look,
from the program's side, like magic:

- **Demand paging.** When a program starts, the kernel does not load the whole
  executable into RAM. It sets up the mappings and loads *nothing*. The first
  time execution reaches a code page, that page faults; the handler reads it from
  disk, wires it in, and restarts the instruction. Pages never executed are never
  loaded. Startup is fast and RAM is spent only on what runs.

- **Lazy allocation.** `malloc(1 GiB)` returns instantly even on a machine with
  little free RAM, because it only *reserves the address range*; no physical
  frames are committed. Each page is backed on first touch, by a fault. Allocate
  a lot, touch a little, and you pay for a little. (This is why the quiz above is
  not a trick.)

- **Copy-on-write.** After `fork`, parent and child appear to have independent
  copies of all their memory — but copying gigabytes would be ruinous. Instead the
  kernel maps the *same* physical frames into both, marked read-only. As long as
  both only read, they share. The instant either one *writes*, the write faults;
  the handler copies that single page, remaps the writer's copy as writable, and
  resumes. Only pages that actually change are ever duplicated. Copy-on-write is a
  page fault used as a tripwire.

- **Memory-mapped files.** `mmap` a file and it appears in your address space as a
  region of memory. You did not read it. When you touch a page of that region, the
  fault handler pulls the corresponding chunk of the file off disk into a frame and
  maps it in; when you write and the mapping is shared, the change flows back to
  the file. The file *is* the backing store for those pages. Reading a file becomes
  ordinary pointer dereferencing, paged in on demand.

Every one of these is the same trick: **leave a mapping incomplete on purpose, and
let the fault that results be your invitation to finish it.**

## When the fault is fatal: the wild pointer and SIGSEGV

Sometimes the faulting access is one the kernel has no good answer for. You
dereference a pointer into a region your process never mapped — a null pointer, a
freed-and-forgotten pointer, an index far off the end of an array, a stack
pointer smashed by a bug. The MMU faults, exactly as before, and the handler runs,
exactly as before. But now the handler looks up the faulting address in the
process's list of valid mappings and finds **nothing** — no lazy allocation to
satisfy, no file page to fetch, no copy-on-write page to duplicate. The access is
simply illegal.

The kernel's response is to deliver the signal **`SIGSEGV`** — a *segmentation
fault* — to the process. Absent a handler, the default action is to kill it. That
is the entire life cycle of the crash you have seen a thousand times:

```text
   wild pointer dereference
        -> MMU can't translate the address
            -> page fault into the kernel
                -> handler: is this address in any valid mapping?
                    -> no  -> deliver SIGSEGV -> process dies
                    -> yes -> back the page / copy it / fault it in -> resume
```

The profound part is that the *same hardware event* — the page fault — is both the
engine of demand paging and the messenger of a fatal bug. What separates "the
kernel quietly maps a page and you never notice" from "your program dies with a
segfault" is nothing more than whether the faulting address falls inside a mapping
the kernel meant to honor. The permission bits do the same double duty: write to a
read-only *code* page and you get `SIGSEGV`; write to a read-only *copy-on-write*
page and you get a private copy. Same fault, same handler, opposite outcomes,
decided entirely by what the kernel intended for that page.

You can watch the good half of this at work. Load a program in the **Playground**
and observe that touching a large allocation grows resident memory page by page,
not all at once. Use the **memory-viewer** to inspect a mapped region and see the
permission bits the kernel assigned it. And in the next lesson, `/proc/<pid>/maps`
will let you read a live process's entire mapping list — the very table the fault
handler consults to decide your program's fate.

## Key points

- A pointer holds a **virtual address**, meaningful only through the current
  process's page tables. The MMU translates it to a physical address on every
  access; `CR3` selects the table, and a context switch reloads it, which is what
  gives each process a private flat address space.
- Translation is conceptually one lookup, virtual page → physical frame. Real
  x86_64 makes it a **four-level tree** so the map stays sparse, and caches the
  result in the **TLB** so the common case is free.
- Memory is mapped in fixed **4 KiB pages** because a fixed, power-of-two,
  aligned size turns translation into a shift-and-index and bounds the size of the
  tables. The low 12 bits of an address are the offset within its page.
- Each page carries **R/W/X permissions**. Enforcing **W^X** via the **NX bit**
  kills the inject-shellcode-and-jump-to-it attack: no page is both writable and
  executable.
- The **page fault** is a trap into the kernel on a bad or incomplete access. Used
  deliberately it powers **demand paging, lazy allocation, copy-on-write, and file
  mmap**; used on a **wild pointer** it becomes `SIGSEGV`. It is the same event
  either way — the kernel's intent for the page decides which.
