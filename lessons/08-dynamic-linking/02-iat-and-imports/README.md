# The Import Address Table

The Windows counterpart to the GOT/PLT lesson. A PE cannot hard-code the address
of a function that lives in a separately-loaded DLL, so it calls through the
Import Address Table — a table of pointers the loader fills in. The lesson traces
the two parallel arrays (the Import Lookup Table and the IAT), shows how the
loader overwrites the IAT in place while the ILT survives to name the calls, and
teaches the `call qword [rip+…]` shape that a reverser reads as "a call into a
DLL." It covers delay loading and binding as the Windows answers to lazy
resolution, and IAT hooking as the echo of the GOT-overwrite.

For sample analysis it adds two reference sections: **API Sets** (`api-ms-win-*`
contract names and the ApiSetMap redirection, which explain why an import need
not match a file on disk) and **reading the import list as signals** — page
protections and `VirtualProtect` framed from the analyst's perspective. That
second section is deliberately descriptive: it explains what the `PAGE_*`
permissions mean and why a protection-change *pattern* draws attention, while
stressing that legitimate software does the same and the observation is one
signal, not evidence. It stops at observable behaviour and does not describe
code-modification or hooking workflows.

Prerequisites: **The GOT and the PLT** (the ELF version of the same problem) and
**PE: From Disk to Memory** (where the import directory lives).
