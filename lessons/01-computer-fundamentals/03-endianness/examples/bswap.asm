; Converting to network byte order (big-endian) is one instruction.
;
; A 16-bit port number, 8080 = 0x1f90, must go on the wire as 1f 90.
; In a little-endian register it is stored 90 1f. htons() is a byte swap.
;
; bswap works on 32- and 64-bit registers, so the 16-bit case is done by
; swapping 32 bits and shifting the result down -- which is precisely what
; an optimising compiler emits for htons().

mov eax, 0x12345678
bswap eax                   ; eax = 0x78563412

mov rbx, 0x1122334455667788
bswap rbx                   ; rbx = 0x8877665544332211

mov ecx, 8080               ; 0x00001f90
bswap ecx                   ; ecx = 0x901f0000
shr  ecx, 16                ; ecx = 0x0000901f  -- htons(8080)

hlt
