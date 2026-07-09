; The address of a value is the address of its LOW byte.
;
; Store 0x1122334455667788 to the stack, then read back one byte from the
; same address. On a little-endian machine you get 0x88 -- the least
; significant byte -- because it lives at the lowest address.
;
; Watch the memory viewer: the qword appears in memory as
;
;   88 77 66 55 44 33 22 11
;
; ...which is the value written backwards, and is exactly right.

sub rsp, 16
mov rax, 0x1122334455667788
mov qword [rsp], rax

movzx rbx, byte [rsp]       ; rbx = 0x88, the LOW byte
movzx rcx, word [rsp]       ; rcx = 0x7788, the low two bytes
mov   edx, dword [rsp]      ; rdx = 0x55667788, the low four bytes

add rsp, 16
hlt
