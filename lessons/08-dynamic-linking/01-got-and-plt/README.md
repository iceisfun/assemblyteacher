# The GOT and the PLT

How a call to a shared-library function actually reaches it: the Procedure
Linkage Table (per-symbol code stubs) and the Global Offset Table (per-symbol
runtime pointers), lazy binding and what the GOT holds before and after the
first call, why a writable table of code pointers is an attacker's target, and
how full RELRO answers it. Windows' IAT / delay-load is drawn as the same idea.
Quiz + an indirect-jump disassemble exercise. Pairs with the Inspector's
relocation/import view.
