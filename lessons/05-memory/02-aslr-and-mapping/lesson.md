+++
id = "address-space-layout"
title = "The Process Address Space and ASLR"
order = 2
estimated_minutes = 40
objectives = [
  "Lay out a Linux process from low to high addresses — text, data, bss, heap, mmap region, stack, vdso — and say which way the heap and stack grow and why",
  "Explain what ASLR randomizes (stack, mmap and libraries, and, for a PIE, the executable image itself) and how randomizing base addresses raises the cost of exploitation",
  "Contrast a fixed preferred load address with a position-independent executable, and state what PIE gives up to gain a randomized image base",
  "Read /proc/<pid>/maps to observe a real process's regions, permissions, and randomized bases directly",
]
prerequisites = ["virtual-memory", "the-stack"]

[[exercises]]
id = "q-grow-toward"
kind = "quiz"
prompt = "In a classic Linux process layout, the heap grows toward higher addresses and the stack grows toward lower addresses. Why arrange them to grow *toward each other* from opposite ends of the address space?"
choices = [
  "So the CPU can tell heap addresses from stack addresses by their sign bit",
  "So one pool of unused address space in the middle can be consumed by whichever of the two needs it, without pre-splitting it",
  "Because the stack must be physically above the heap in RAM",
  "So a stack overflow can never reach the heap",
]
answer = 1
explanation = "Growing from opposite ends lets the heap and stack share a single expanse of free virtual space in between. Neither has to be given a fixed budget up front; whichever grows more simply eats more of the shared middle."

[[exercises]]
id = "q-what-aslr"
kind = "quiz"
prompt = "With ASLR enabled on a modern Linux system running a position-independent executable (PIE), which of these has its base address randomized from run to run?"
choices = [
  "Only the stack",
  "Only shared libraries loaded via mmap",
  "The stack, the mmap region (and the libraries in it), and the executable image itself",
  "Nothing — ASLR only randomizes the order of functions within the binary",
]
answer = 2
explanation = "Full ASLR randomizes the stack base, the mmap region base (so every shared library lands at an unpredictable address), and — because the executable is a PIE — the image's own load base. Only a non-PIE (fixed-base) executable keeps its text and data at predictable addresses."

[[exercises]]
id = "q-why-aslr-helps"
kind = "quiz"
prompt = "ASLR does not fix the memory-safety bug that lets an attacker hijack control flow. What does randomizing base addresses actually accomplish against an exploit?"
choices = [
  "It encrypts the process's memory so the payload cannot be read",
  "It makes the attacker's needed addresses (of code to reuse, of a buffer to jump to) unpredictable, so a hardcoded exploit hits an unmapped page and crashes instead of succeeding",
  "It prevents buffer overflows from occurring in the first place",
  "It marks every page NX so injected code cannot run",
]
answer = 1
explanation = "ASLR is a probabilistic mitigation, not a fix. With W^X forcing attackers to reuse existing executable code, they must know *where* that code is. Randomizing the bases means a hardcoded address is wrong on the next run, turning a reliable exploit into one that usually just crashes the target — and often forces the attacker to first find an information leak."

[[exercises]]
id = "q-pie-tradeoff"
kind = "quiz"
prompt = "A position-independent executable (PIE) can be loaded at a randomized base, which a traditional fixed-base executable cannot. What does the PIE give up to get that?"
choices = [
  "It cannot use the heap",
  "Its code must reference globals and functions relatively (e.g. RIP-relative) and be relocated at load, costing a little size and startup work, instead of baking in absolute addresses",
  "It can no longer be linked against shared libraries",
  "It must be fully re-compiled every time it runs",
]
answer = 1
explanation = "A fixed-base executable can hardcode the absolute address of every global because it always loads at one preferred address. A PIE does not know its base until load time, so its code addresses memory relative to the instruction pointer and the loader applies relocations at startup — a small cost in size and load time, paid to allow the image base to be randomized."
+++

# The Process Address Space and ASLR

The previous lesson gave every process a private, flat expanse of virtual
addresses and the machinery to translate them. This lesson asks the next
question: *what does the operating system put in that expanse, and where?* The
answer is the **process address space** — a specific arrangement of regions with a
long history — and the deliberate scrambling of that arrangement, **ASLR**, which
is one of the quiet reasons modern systems are hard to exploit.

You will finish able to read a real process's map with your own eyes.

## The layout of a Linux process

When the kernel and dynamic loader finish setting up a process, its virtual
address space is carved into regions, each a run of pages with its own purpose and
its own permissions. From low addresses to high, the classic picture is:

```text
   high addresses
   0x7fff_ffff_ffff  +---------------------------+
                     |          [stack]          |  rw-   grows DOWN  |
                     |             |             |                    v
                     |             v             |
                     +---------------------------+
                     |            ...            |  <- large unused gap
                     +---------------------------+
                     |   [vdso] / [vvar]         |  r-x   kernel-provided
                     +---------------------------+
                     |   shared libraries        |  r-x / rw-
                     |   (libc, ld.so, ...)      |
                     |   the mmap region         |  grows toward LOWER addrs
                     +---------------------------+
                     |             ^             |
                     |             |             |
                     |          [heap]           |  rw-   grows UP    ^
                     +---------------------------+                    |
                     |   bss   (zero-init data)  |  rw-
                     +---------------------------+
                     |   data  (init'd globals)  |  rw-
                     +---------------------------+
                     |   text  (machine code)    |  r-x
   0x0000_5555_...   +---------------------------+
                     |   (unmapped, incl. NULL)  |
   0x0000_0000_0000  +---------------------------+
   low addresses
```

Read it from the bottom up, because that is roughly the order it was built:

- **text** — your program's machine code. Mapped **read + execute**, never
  writable, exactly as the previous lesson's W^X rule demands. This is the segment
  the Inspector shows you when it lists a binary's sections.
- **data** — global and static variables with an initial value (`int n = 42;`).
  Mapped **read + write**. Its initial contents are loaded from the executable
  file.
- **bss** — global and static variables that start at zero (`int table[4096];`).
  It occupies *no space in the file* — there is nothing to store but "all zero" —
  and the kernel simply maps zero-filled pages for it on demand. (This is a page
  fault used for lazy allocation, from the previous lesson.)
- **heap** — the region `malloc` grows for dynamic allocation. It starts just
  above bss and **grows toward higher addresses** as the program asks for more
  (historically via the `brk` system call, which moves the top of the heap up).
- **mmap region** — where `mmap` places large allocations, and where the dynamic
  loader maps **shared libraries** like the C library. It sits high in the space
  and, on Linux, **grows toward lower addresses** as more mappings are added.
- **stack** — the call stack you studied earlier. It lives near the top of the
  user address space and **grows toward lower addresses**: every `push` and every
  `call` moves `rsp` *down*.
- **vdso / vvar** — a small region the *kernel* maps into every process,
  containing a few pages of code and data that let certain system calls (like
  reading the clock) run in user space without a full kernel entry. You did not
  ask for it; it is always there.

### Why heap up, stack down

The single most important structural fact here is that the **heap grows up and the
stack grows down, toward each other**, with a large unused gap between them. This
is not arbitrary. It lets a single region of free virtual space serve *both*.

If the heap and stack each grew the same direction, the system would have to
decide in advance how much space to give each, and a program that used lots of
one and little of the other would waste the difference. By placing them at
opposite ends of a shared middle and growing them inward, whichever one needs more
simply consumes more of the common pool. Neither is pre-budgeted. The stack grows
down because it was the natural complement to a heap growing up — put the two
growable regions back-to-back and let them meet in the middle only if the program
truly exhausts memory.

You already saw the local consequence of the stack's direction: `push` decrements
`rsp`, `ret` is a `pop rip`, and a buffer overflow writes *upward* in memory
toward the saved return address. Now you can see the global reason the stack is
oriented that way at all.

## ASLR: scrambling the map on purpose

Look again at the layout and notice how *predictable* it is. For decades, the
stack always started at the same address, libraries always loaded at the same
place, and the executable's code always sat at one fixed base. Predictability is
convenience — and, to an attacker, it is a gift.

Recall the arc from the previous lesson: W^X (via NX) stopped attackers from
injecting and running their own code, which pushed them toward **reusing code that
is already executable** — jumping into a useful library function, or stitching
together fragments of existing code ("return-oriented programming"). But every
one of those techniques needs the same thing: **the attacker must know the address
of the code or data they intend to use.** In a fully predictable layout, they
simply look it up once and hardcode it.

**ASLR — Address Space Layout Randomization** — takes that away. At each program
start, the kernel and loader place the major regions at randomized base addresses:

- The **stack** base is randomized, so stack addresses differ every run.
- The **mmap region** base is randomized, so every shared library — libc
  included — lands at an unpredictable address, and so do large `mmap`
  allocations.
- The **executable image itself** is randomized — *if* it is built as a
  **position-independent executable (PIE)**. A PIE has no fixed base, so its text,
  data, and bss can be slid to a random base like a library. A non-PIE executable
  keeps a fixed base and is the one part ASLR cannot move.

Crucially, ASLR randomizes the **base** of each region, not the internal layout.
Offsets *within* a library are unchanged; what moves is where the whole library
starts. That is enough, because the attacker needs an absolute address, and the
absolute address is base + offset with an unknown base.

### Why randomizing bases raises the bar

ASLR fixes no bug. The memory-safety flaw that lets an attacker redirect control
flow is still there. What ASLR changes is the **economics** of the exploit:

- A hardcoded address that was correct on the developer's machine is now wrong on
  the target, because the base moved. The exploit's jump lands in an **unmapped
  page** and the process takes a `SIGSEGV` — it *crashes* instead of being
  *owned*. A crash is a failed attack.
- To succeed reliably, the attacker must first **learn** the randomized base,
  usually by finding a second bug — an *information leak* that discloses a live
  pointer — and computing everything relative to it. That is a much higher bar
  than reading an address out of a debugger once.

ASLR is therefore a **probabilistic mitigation**: it does not make exploitation
impossible, it makes it unreliable and expensive, and it turns many bugs that
would have been clean compromises into mere crashes. Combined with W^X, it is why
a single memory-safety bug is usually no longer enough on its own. The Inspector
reports whether a binary opts into these protections (PIE, NX, stack canaries,
RELRO); reading that report, you now know it is describing exactly how much of the
map an attacker can predict.

## Preferred base vs. PIE: the trade

To randomize the executable's own base, the binary has to be built to *not care*
where it loads. That is the difference between a traditional executable and a PIE.

A traditional executable has a **preferred (fixed) load address** — on x86_64
Linux, historically around `0x400000`. The linker bakes that assumption
throughout the code: when the program refers to a global variable or calls one of
its own functions, the **absolute address** is written directly into the
instruction, because the linker *knows* where everything will be. This is fast and
simple, and completely defeats ASLR for that image: it can only ever load in one
place.

A **position-independent executable** gives up the fixed base. It does not know
where it will load until the loader picks a (randomized) base at startup. To cope,
its code cannot contain absolute addresses of its own symbols. Instead it
references them **relative to the instruction pointer** — RIP-relative addressing,
`[rip + offset]`, where the offset is a fixed distance the linker *can* know — and
the loader applies a set of **relocations** at startup to fix up the pointers that
genuinely need absolute values. The payoffs and costs:

| | fixed-base executable | PIE |
|---|---|---|
| load address | one preferred address, always | randomized each run |
| own-symbol references | absolute, baked in | RIP-relative + load-time relocations |
| ASLR of the image | none — image is predictable | full — image base is randomized |
| cost | none | slightly larger, small startup relocation work, a spare register/indirection for globals |

So the PIE "gives up" the fixed base and the free absolute addressing that came
with it, paying a little size and a little startup time and a little runtime
indirection — in exchange for letting the single most valuable target in the
process, its own code, hide behind a randomized base. On modern distributions PIE
is the default precisely because that trade is worth it.

```asm
    ; A PIE cannot do this — 0x404040 is not known until load time:
    ;   mov  rax, [0x404040]      ; absolute address of a global

    ; It does this instead — the *distance* to the global is known,
    ; the base is not, and the CPU adds rip at run time:
    mov  rax, [rip + my_global]   ; RIP-relative: base-independent
```

That `[rip + label]` form is the same one the assembler in this course accepts,
and it is the concrete shape of "position independent": an address expressed as a
distance from *here*, so it stays correct no matter where *here* turns out to be.

## Seeing it: /proc/<pid>/maps

None of this has to be taken on faith. Linux exposes every process's live mapping
table as a readable text file at **`/proc/<pid>/maps`**. Each line is one
contiguous region: its address range, its permissions, and what backs it. A
typical excerpt:

```text
  555555554000-555555555000 r-xp  ...  /home/you/a.out      <- text  (r-x)
  555555754000-555555755000 rw-p  ...  /home/you/a.out      <- data  (rw-)
  555555756000-555555777000 rw-p  ...  [heap]               <- heap  (rw-)
  7ffff7dc0000-7ffff7f50000 r-xp  ...  /usr/lib/libc.so.6   <- libc text (r-x)
  7ffffffde000-7ffffffff000 rw-p  ...  [stack]              <- stack (rw-)
  7ffff7fc0000-7ffff7fc3000 r--p  ...  [vvar]
  7ffff7fc3000-7ffff7fc5000 r-xp  ...  [vdso]
```

Everything from this lesson is visible in those lines:

- The **permission column** (`r-xp`, `rw-p`) is the page-permission story of the
  previous lesson made literal: the text and libc regions are `r-x` (no `w`), the
  heap and stack are `rw-` (no `x`). W^X, right there — no region is both `w` and
  `x`.
- The **layout** is exactly the diagram above: image low, heap above it, libraries
  and stack high, `[vdso]`/`[vvar]` mapped in for free.
- **ASLR is directly observable**: run the same program twice and compare the base
  addresses. With ASLR on, the `[stack]` base, the libc base, and — for a PIE —
  the `a.out` base all *change* between runs. With ASLR off (or for a non-PIE),
  they stay put. Two runs, two maps, a diff: that is the randomization, measured.

The **memory-viewer** in this platform gives you the same view for a program you
run here, and the **Inspector** reads the *static* side of the story out of the
ELF — which segments exist, their permissions, whether the binary is a PIE, and
which mitigations it requests — before the process ever starts. Between the file
(Inspector) and the live map (`/proc/<pid>/maps`, memory-viewer), you can watch a
static executable become a running, randomized address space.

## Key points

- A Linux process is laid out low-to-high as **text (r-x), data (rw-), bss, heap
  (grows up), the mmap region and shared libraries, the stack (grows down), and
  the kernel-provided vdso/vvar.**
- **Heap grows up and stack grows down toward each other** so a single pool of
  free space between them serves whichever needs it, with no fixed budget.
- **ASLR** randomizes the **base** of the stack, the mmap region (and thus every
  library), and — for a **PIE** — the executable image itself. Only a fixed-base
  non-PIE keeps predictable code addresses.
- ASLR fixes no bug; it makes the addresses an exploit needs **unpredictable**, so
  hardcoded attacks crash on an unmapped page and attackers must first find an
  info leak. It is a probabilistic mitigation that pairs with W^X.
- A **PIE** trades the fixed **preferred load address** (and cheap absolute
  addressing) for **RIP-relative code plus load-time relocations**, buying a
  randomizable image base for a small size and startup cost.
- **`/proc/<pid>/maps`** shows a live process's regions, permissions, and bases —
  diff two runs to *see* ASLR move them; the memory-viewer and Inspector give the
  live and static views.
