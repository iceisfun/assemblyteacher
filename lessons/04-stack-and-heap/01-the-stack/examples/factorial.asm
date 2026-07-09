; Recursive factorial. fact(5) = 120.
;
; Three things happen on every recursive call:
;   1. `call` pushes a return address.
;   2. we push rdi, because the recursive call will destroy it.
;   3. on the way out, we pop rdi and then `ret` pops the return address.
;
; Push and pop must balance exactly, or `ret` will jump into our saved data.
; Scrub the trace and watch rsp descend five levels and climb back.

    mov rdi, 5
    call fact
    hlt                     ; rax = 120

fact:
    cmp rdi, 1
    jbe base_case           ; unsigned <= 1

    push rdi                ; preserve n
    dec rdi
    call fact               ; rax = fact(n-1)
    pop rdi                 ; restore n
    imul rax, rdi           ; rax = n * fact(n-1)
    ret

base_case:
    mov rax, 1
    ret
