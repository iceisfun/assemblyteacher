; Sum the integers 1..10. Result: rax = 55.
;
; Step it in the playground and watch ecx climb while eax accumulates.
; Note the loop exits with ecx = 11, not 10.

        xor eax, eax        ; sum = 0     (2 bytes, and clears all 64 bits)
        mov ecx, 1          ; i = 1

loop_top:
        add eax, ecx        ; sum += i
        inc ecx             ; i++        (leaves CF untouched)
        cmp ecx, 10         ; set flags from i - 10
        jle loop_top        ; signed: jump back while i <= 10

        hlt                 ; rax = 55
