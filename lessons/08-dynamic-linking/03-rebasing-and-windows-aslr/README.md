# Rebasing, Relocations, and Windows ASLR

The mechanical companion to the Part V ASLR lesson, told on Windows. If an image
can load at a random address, what happens to the absolute addresses baked inside
it? This lesson answers with the base relocation table: it contrasts
position-independent code (no fixups needed) with the relocation approach (ship
the location of every absolute address and patch each by the load delta), reads
the per-page `.reloc` block layout, and explains Windows ASLR's defining
difference from Linux — a DLL's base is chosen once per boot and shared across
processes, which saves memory but changes what an information leak is worth.

It is deliberately *not* a mitigations survey (Part XIV covers which mitigation
stops which attack); it is about the rebasing mechanism.

Prerequisites: **The Process Address Space and ASLR** (the why) and **PE: From
Disk to Memory** (where `.reloc` and `ImageBase` live).
