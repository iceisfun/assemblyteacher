; Four ways to put zero in rax. Assemble this and compare the byte counts
; in the listing.
;
;   xor eax, eax        2 bytes   31 c0
;   xor rax, rax        3 bytes   48 31 c0        (the REX.W is pointless here)
;   mov eax, 0          5 bytes   b8 00 00 00 00
;   mov rax, 0          7 bytes   48 c7 c0 00 00 00 00
;
; All four leave rax = 0. The first is what every compiler emits: it is the
; shortest, it needs no immediate, and since the Pentium Pro the CPU
; recognises it in the register renamer and never actually executes it.
;
; Note that `xor eax, eax` also clears CF and OF, while `mov` touches no
; flags at all. Occasionally that difference matters.

xor eax, eax
xor rax, rax
mov eax, 0
mov rax, 0

hlt
