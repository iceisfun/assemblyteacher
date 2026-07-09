; A standard frame. Step through it with the stack viewer open.

    mov rdi, 7
    call square
    hlt

square:
    push rbp                ; save the caller's frame pointer
    mov  rbp, rsp           ; our frame base
    sub  rsp, 16            ; room for locals

    mov  qword [rbp-8], rdi ; a local variable, at a fixed offset from rbp
    mov  rax, qword [rbp-8] ; ...read it back
    imul rax, rax           ; rax = 49

    leave                   ; mov rsp, rbp ; pop rbp   -- one byte
    ret                     ; pop the return address and jump to it
