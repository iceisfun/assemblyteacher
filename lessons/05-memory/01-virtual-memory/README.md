# Virtual Memory and Pages

Why a pointer is a virtual address, not a physical one, and how the MMU
translates it through a per-process page table — presented as the solution to
three otherwise-unfixable problems (address collisions, no isolation, no
relocation) rather than as machinery to memorize.

Covers the conceptual one-level "virtual page → physical frame" lookup and why
real x86_64 uses a four-level tree with a TLB; fixed 4 KiB pages and why fixed
size makes translation a shift-and-index; per-page R/W/X permissions, W^X, and
the NX bit as the answer to injected shellcode; and the page fault as the single
mechanism behind demand paging, lazy allocation, copy-on-write, and file mmap —
and, on a wild pointer, behind SIGSEGV.

Four quiz exercises. No runnable examples (the concepts live in prose and
diagrams); references the Inspector, memory-viewer, and Playground.
