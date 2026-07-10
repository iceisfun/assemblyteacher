# ELF: From Disk to Memory

The gap between an executable file and a running process. Sections (linker's
view) versus segments (loader's view) and why a stripped binary still runs; the
file-offset / virtual-address / RVA distinction and how to convert; the loader's
actual steps; and position-independent code as the machinery behind ASLR.
Pairs with the Inspector, which shows the headers, sections and segments of any
uploaded binary. Quiz + a RIP-relative disassemble exercise.
