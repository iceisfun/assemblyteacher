; What happens when a push has no matching pop.
;
; `ret` does not check anything. It loads eight bytes from [rsp] and jumps
; there. If the thing at [rsp] is your saved rax rather than the return
; address `call` pushed, then rax is now your instruction pointer.
;
; Run this. It faults, and the fault address is 0x2222 -- the value we
; pushed. That is not a coincidence; that is the whole mechanism behind
; return-oriented programming, demonstrated in five instructions.

    call broken
    hlt                     ; we never get here

broken:
    mov rax, 0x2222
    push rax                ; pushed...
    ; ...and never popped.
    ret                     ; so `ret` jumps to 0x2222
