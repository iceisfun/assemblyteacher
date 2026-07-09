; Every addressing shape is a subset of  [base + index*scale + disp].
;
; Assemble this and read the ModRM/SIB bytes in the listing. Everything
; below is a single instruction.

mov rbx, 0x2000
mov rcx, 3

mov rax, qword [rbx]                ; *rbx
mov rax, qword [rbx+8]              ; rbx->second_field
mov rax, qword [rbx+rcx*8]          ; rbx[rcx], elements 8 bytes wide
mov rax, qword [rbx+rcx*4+16]       ; rbx->arr[rcx], arr at offset 16
mov rax, qword [rip+0x100]          ; a global, relative to the next instruction

; No base, only a scaled index -- legal, and costs a 4-byte displacement
; because the encoding has no "no base" form without one.
mov rax, qword [rcx*8]

hlt
