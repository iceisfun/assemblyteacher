; The zero-extension rule, demonstrated.
;
; Step through this and watch rax in the register view. Only the 32-bit
; write clears the upper half.

mov rax, -1         ; rax = ffffffffffffffff
mov al, 0x11        ; rax = ffffffffffffff11   <- 8-bit:  merge
mov ax, 0x2222      ; rax = ffffffffffff2222   <- 16-bit: merge
mov eax, 0x33333333 ; rax = 0000000033333333   <- 32-bit: ZERO-EXTEND

; ...and the reason: writing eax breaks the dependency on the old value,
; so the renamer is free to hand you a fresh physical register.

hlt
