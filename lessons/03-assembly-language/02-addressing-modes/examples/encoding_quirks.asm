; Three registers. One operation. Three different instruction lengths.
;
;   mov rax, qword [rax]    48 8b 00        3 bytes
;   mov rax, qword [rsp]    48 8b 04 24     4 bytes  <- SIB byte forced
;   mov rax, qword [rbp]    48 8b 45 00     4 bytes  <- zero displacement forced
;
; [rsp]: ModRM rm=100 means "a SIB byte follows", so naming rsp as a base
;        cannot avoid one.
;
; [rbp]: ModRM mod=00 rm=101 was repurposed for RIP-relative addressing,
;        so [rbp] must be spelled [rbp+0].
;
; Assemble this file and read the byte counts in the listing. The encoding
; is not uniform, and the irregularities are all historical.

mov rbx, 0x2000
mov rax, qword [rbx]
mov rax, qword [rsp]
mov rax, qword [rbp]

; rsp can never be an index -- its encoding was spent on "no index".
; Uncommenting the next line is a good way to see the assembler say so:
;
;   mov rax, qword [rbx+rsp*1]
;
; r12 CAN be an index, because REX.X rescues that one bit pattern.
mov r12, 2
mov rax, qword [rbx+r12*8]

hlt
