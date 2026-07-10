# Reading Compiled Code

A field guide to the shapes a compiler leaves behind: the frame-pointer
prologue that marks a function start, the back-edge that marks a loop, the
inverted forward branch of an `if`, the branch mnemonic that recovers erased
signedness, and struct/array layout read straight out of the addressing modes.
Ends with a top-down method. Quiz + two disassemble exercises (recognise the
prologue, recognise the loop condition).
