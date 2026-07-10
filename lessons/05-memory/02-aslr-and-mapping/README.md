# The Process Address Space and ASLR

What the operating system lays out inside a process's virtual address space, and
why that layout is deliberately scrambled. Walks the Linux process image low to
high — text, data, bss, heap (grows up), the mmap region and shared libraries,
the stack (grows down), and the kernel-mapped vdso/vvar — and derives the
heap-up/stack-down arrangement from the shared free pool between them.

Covers ASLR: what it randomizes (stack, mmap/libraries, and the image itself for
a PIE), why randomizing bases turns reliable exploits into crashes and forces an
info leak, and the fixed preferred load address vs. PIE trade-off (absolute
addressing given up for RIP-relative code and load-time relocations). Ends by
reading `/proc/<pid>/maps` to see the regions, permissions, and randomized bases
directly, tied to the memory-viewer and Inspector.

Four quiz exercises. No runnable examples; assembly appears only as illustrative
RIP-relative snippets in prose. Prerequisites: virtual-memory, the-stack.
