; The assembler forgets the base you typed as soon as it has the value.
; All three of these instructions assemble to exactly the same two bytes:
;
;   b0 0b
;
; Assemble this file in the playground and read the output.

mov al, 0b1011      ; binary
mov al, 0x0b        ; hexadecimal
mov al, 11          ; decimal

hlt
