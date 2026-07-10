# The Import Address Table

The Windows counterpart to the GOT/PLT lesson. A PE cannot hard-code the address
of a function that lives in a separately-loaded DLL, so it calls through the
Import Address Table — a table of pointers the loader fills in. The lesson traces
the two parallel arrays (the Import Lookup Table and the IAT), shows how the
loader overwrites the IAT in place while the ILT survives to name the calls, and
teaches the `call qword [rip+…]` shape that a reverser reads as "a call into a
DLL." It closes with delay loading and binding as the Windows answers to lazy
resolution, and IAT hooking as the echo of the GOT-overwrite.

Prerequisites: **The GOT and the PLT** (the ELF version of the same problem) and
**PE: From Disk to Memory** (where the import directory lives).
