; The classic two-byte infinite loop.
;
;   eb fe
;   │  └── displacement -2, measured from the END of this instruction
;   └───── jmp rel8
;
; The instruction is two bytes long. After fetching it, rip points just past
; it. Adding -2 puts rip back at the start of the jump. Forever.
;
; This is why relative jumps make code position-independent: nothing here
; mentions an address, so these two bytes mean "loop here" no matter where
; they are loaded.
;
; (Do not run this one to completion -- it never halts. The emulator will
; stop it at the step limit, which is itself worth seeing.)

here:
    jmp here
