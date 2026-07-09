; One adder. Two readings. The CPU sets both flags and lets you choose.
;
; 0x7fffffffffffffff is the largest positive signed 64-bit value.
; Adding 1 to it:
;
;   as signed   -> overflow into negative.   OF = 1
;   as unsigned -> just a bigger number.     CF = 0
;
; 0xffffffffffffffff is -1 signed, or the largest unsigned value.
; Adding 1 to it:
;
;   as signed   -> -1 + 1 = 0. Correct.      OF = 0
;   as unsigned -> wraps to 0, carry out.    CF = 1

mov rax, 0x7fffffffffffffff
add rax, 1                  ; OF=1 CF=0  -- signed overflow only

mov rbx, -1                 ; 0xffffffffffffffff
add rbx, 1                  ; OF=0 CF=1  -- unsigned overflow only

; inc is add-by-one that deliberately leaves CF alone, so a loop can
; count with inc while carrying a bit across iterations in CF.
mov rcx, -1
inc rcx                     ; ZF=1, and CF is untouched

hlt
