; One comparison. Two interpretations. Two different answers.
;
; rax = 1, rbx = -1.
;
;   signed:    1 >  -1     so `jl` (signed less) is NOT taken
;   unsigned:  1 <  0xffffffffffffffff   so `jb` (unsigned below) IS taken
;
; The `cmp` is identical in both cases. Only the branch differs.
; This is how a disassembly leaks the types the compiler erased.

mov rax, 1
mov rbx, -1

cmp rax, rbx
jl  signed_less         ; not taken:  1 is not less than -1
mov rcx, 1              ; rcx = 1  ("signed says greater")

cmp rax, rbx
jb  unsigned_below      ; taken:      1 is below 0xffff...ffff
mov rdx, 0

done:
    hlt

signed_less:
    mov rcx, 0
    jmp done

unsigned_below:
    mov rdx, 1          ; rdx = 1  ("unsigned says below")
    jmp done
