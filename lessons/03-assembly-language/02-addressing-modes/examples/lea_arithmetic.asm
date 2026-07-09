; lea is not really about addresses.
;
; It is the only three-operand arithmetic instruction in the integer ISA:
; it reads two registers, applies a shift and an add, writes a third
; register, and touches no flags and no memory.

mov rbx, 10
mov rcx, 7

lea rax, [rbx+rcx]          ; rax = 17          -- add, preserving rbx
lea rdx, [rbx+rbx*4]        ; rdx = rbx * 5     -- multiply by a constant
lea rsi, [rcx*8+16]         ; rsi = 7*8 + 16 = 72

; And it never dereferences, so this is fine even though nothing is
; mapped at address 0:
lea rdi, [0]                ; rdi = 0. No fault. No memory access.

; Because lea sets no flags, it can sit between a cmp and its branch:
cmp rbx, rcx
lea r8, [rbx+1]             ; flags survive this
jg  bigger

mov r9, 0
hlt

bigger:
mov r9, 1
hlt
