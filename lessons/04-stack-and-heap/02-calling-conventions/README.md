# Calling Conventions

The System V AMD64 ABI — argument registers, the return register, callee- vs
caller-saved registers, the red zone, and the 16-byte alignment rule — presented
as the *agreement* that `call` itself does not encode, then contrasted with
Microsoft x64 to show why the two cannot be mixed. Recognising the "movs into
rdi/rsi/rdx before a call" pattern is framed as half of reading compiled code.

Two emulate exercises (add3 via the argument registers, a branchless signed
max2) plus quizzes on the argument order, the red zone, and alignment. Every
reference solution runs on the emulator.
