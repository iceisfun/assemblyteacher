; cmp is sub with the result thrown away.
; test is and with the result thrown away.
;
; Run this and watch: after the `sub`, rbx has changed. After the `cmp`,
; rcx has not -- but the flags are identical in both cases.

mov rbx, 10
sub rbx, 10             ; rbx = 0,  ZF = 1

mov rcx, 10
cmp rcx, 10             ; rcx = 10, ZF = 1   <- same flags, no write

; `test reg, reg` is the idiomatic zero check: x & x == x, so ZF is set
; exactly when x is zero. Shorter than `cmp reg, 0` and needs no immediate.
xor rdx, rdx
test rdx, rdx           ; ZF = 1  ->  "if (!rdx)"

hlt
