; push is sub-then-store. pop is load-then-add. Proof:

mov rax, 0x1111
mov rbx, 0x2222

push rax                ; rsp -= 8, [rsp] = rax
                        ; ...which is exactly:
sub rsp, 8
mov qword [rsp], rbx

; Two values on the stack now. Pop them back in reverse order --
; last in, first out.

pop rcx                 ; rcx = 0x2222   (the one pushed last)
pop rdx                 ; rdx = 0x1111

; rsp is back where it started. Watch it in the stack viewer:
; it went down 16 bytes and came back up 16.

hlt
