# PE: From Disk to Memory

The Windows counterpart to the ELF lesson. It walks a Portable Executable from
the `MZ` DOS stub through `e_lfanew` to the `PE\0\0` signature and the optional
header, then maps the PE vocabulary onto the ELF concepts the reader already
knows: RVA and ImageBase, sections and their permission characteristics, and the
data directories that serve as the loader's index into imports, relocations, and
TLS.

It is the foundation for the two Windows lessons that follow — the Import Address
Table and base relocations / ASLR — and for the TLS-callbacks lesson in this
part.

Prerequisite: **ELF: From Disk to Memory**.
