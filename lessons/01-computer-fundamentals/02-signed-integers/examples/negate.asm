; Two ways to negate, and proof they are the same operation.
;
; -x  ==  ~x + 1
;
; Run this and watch rax and rbx. They finish holding the same value.

mov rax, 5
not rax             ; flip every bit:  ~5
inc rax             ; add one:         ~5 + 1  =  -5

mov rbx, 5
neg rbx             ; the single instruction that does both

; rax == rbx == -5 == 0xfffffffffffffffb

hlt
