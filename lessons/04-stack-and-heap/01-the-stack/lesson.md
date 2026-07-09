+++
id = "the-stack"
title = "The Stack and Call Frames"
order = 1
estimated_minutes = 45
objectives = [
  "Trace rsp through push, pop, call and ret",
  "Explain why the stack grows downwards",
  "Read a standard function prologue and epilogue, and say what each instruction restores",
  "Explain how a return address on the stack becomes an attacker's instruction pointer",
]
prerequisites = ["addressing-modes"]

[[exercises]]
id = "q-ret"
kind = "quiz"
prompt = "What does `ret` do?"
choices = [
  "Jumps to the address in rax",
  "Pops an 8-byte value off the stack and jumps to it",
  "Restores rsp from rbp and jumps to the caller",
  "Returns to the operating system",
]
answer = 1
explanation = "`ret` is `pop rip`. It takes whatever eight bytes rsp points at and jumps there — with no check that a `call` put them there. That is the entire basis of return-oriented programming."

[[exercises]]
id = "q-push-order"
kind = "quiz"
prompt = "`rsp` is `0x7000`. You execute `push rax`. What is `rsp` afterwards, and where did the value go?"
choices = [
  "rsp = 0x7008, value at 0x7000",
  "rsp = 0x6ff8, value at 0x6ff8",
  "rsp = 0x6ff8, value at 0x7000",
  "rsp = 0x7008, value at 0x7008",
]
answer = 1
explanation = "push subtracts 8 from rsp *first*, then stores at the new rsp. The stack grows downwards, and rsp always points at the most recently pushed item."

[[exercises]]
id = "a-prologue"
kind = "assemble"
prompt = "Write the two-instruction standard function prologue: save the caller's frame pointer, then establish your own."
starter = ""
solution = """
push rbp
mov rbp, rsp
"""
expect_hex = "554889e5"
hints = [
  "First push the old rbp so it can be restored later.",
  "Then point rbp at the top of the stack, so it stays fixed while rsp moves.",
]

[[exercises]]
id = "e-factorial"
kind = "emulate"
prompt = "Write a *recursive* factorial. Put 5 in `rdi`, call your function, and halt with `rax = 120`. Your function must call itself, use the stack to preserve `rdi` across the recursive call, and end with `ret`."
starter = """
    mov rdi, 5
    call fact
    hlt
fact:
    ; if rdi <= 1, return 1
    ; otherwise return rdi * fact(rdi - 1)
    ret
"""
solution = """
    mov rdi, 5
    call fact
    hlt

fact:
    cmp rdi, 1
    jbe base_case
    push rdi                ; preserve n across the recursive call
    dec rdi
    call fact               ; rax = fact(n-1)
    pop rdi                 ; restore n
    imul rax, rdi           ; rax = n * fact(n-1)
    ret

base_case:
    mov rax, 1
    ret
"""
expect_registers = { rax = 120 }
max_steps = 10000
hints = [
  "`call` pushes a return address; `ret` pops it. Anything else you push must be popped before you `ret`, or `ret` will jump to your data.",
  "`rdi` is destroyed by the recursive call, so push it first and pop it after.",
  "The base case is `rdi <= 1`, so `cmp rdi, 1` then `jbe`.",
]
+++

# The Stack and Call Frames

A function needs somewhere to keep its local variables, and somewhere to
remember where it was called from. It cannot use fixed addresses, because a
function may be executing several times at once — that is what recursion is.

The answer is a stack: a region of memory with a single pointer, `rsp`, marking
its top. Every function that starts running claims a bit of it, and every
function that finishes gives it back, in exactly the reverse order. Last in,
first out.

## It grows downwards

```text
   high addresses
   ┌────────────────┐  0x7fff_0000_0000   ← the stack starts here
   │  ...caller...  │
   ├────────────────┤
   │  return addr   │
   ├────────────────┤
   │  saved rbp     │  ← rbp
   ├────────────────┤
   │  local var     │
   ├────────────────┤
   │  local var     │  ← rsp   the "top" of the stack, at the LOWEST address
   └────────────────┘
   low addresses            ...and it grows this way ↓
```

`push` **subtracts** from `rsp` and then stores. `pop` loads and then **adds**.
The top of the stack is at the lowest address, which is why every stack diagram
you will ever see appears upside down.

Why downwards? A historical accident that turned out useful. Put the stack at
the top of the address space growing down, and the heap at the bottom growing
up, and the two share one pool of free space in the middle. Neither has to
declare a maximum size in advance. On a machine with 128 KiB of RAM that was
worth a great deal, and by the time it stopped mattering, every calling
convention on earth depended on it.

## push and pop

```asm
    push rax        ; rsp -= 8;  [rsp] = rax
    pop  rax        ; rax = [rsp];  rsp += 8
```

Exactly equivalent to:

```asm
    sub rsp, 8
    mov qword [rsp], rax        ; ...this is `push rax`

    mov rax, qword [rsp]
    add rsp, 8                  ; ...this is `pop rax`
```

`push` is one byte, though (`50+r`), and the pair is two. Compilers still emit
the `sub`/`mov` form when they want to write several locals without moving `rsp`
each time.

Note the order. `push` decrements *first*, so `rsp` always points at the most
recently pushed item — never at free space.

## call and ret

```asm
    call fn         ; push the address of the NEXT instruction; jump to fn
    ret             ; pop an address; jump to it
```

That is all they are. `call` is "push rip; jmp", and `ret` is "pop rip".

The consequences are worth dwelling on:

- The return address is **data on the stack**. It is not protected, not tagged,
  not in a special register. It is eight bytes sitting in writable memory.
- `ret` will jump to whatever eight bytes `rsp` points at. It does not verify
  that a `call` put them there. It cannot.
- Therefore **anything you push, you must pop before you `ret`**, or `ret` will
  jump into your saved data. This is the single most common way for hand-written
  assembly to crash, and the crash address looks like nonsense because it *is*
  your data being executed.

And there is the other consequence. If a function writes past the end of a local
buffer, it walks upward through its own frame and overwrites the return address.
When the function returns, the CPU faithfully jumps wherever the attacker wrote.
That is the stack buffer overflow, and it is not a bug in `ret`. `ret` is doing
exactly what it was designed to do. Every mitigation we will study later —
canaries, NX, ASLR, shadow stacks — exists because return addresses live in
writable memory next to arrays.

## The frame

Locals live at fixed offsets from a pointer. `rsp` moves around as things are
pushed, so a second register, `rbp`, is conventionally parked at the frame's
base and left alone:

```asm
    push rbp            ; save the caller's frame pointer
    mov  rbp, rsp       ; our frame starts here
    sub  rsp, 32        ; make room for 32 bytes of locals
    ...
    mov qword [rbp-8], rax      ; a local, at a fixed offset from rbp
    ...
    leave               ; mov rsp, rbp ; pop rbp
    ret
```

`leave` is exactly `mov rsp, rbp` followed by `pop rbp`: it discards the locals
by resetting `rsp`, then restores the caller's frame pointer. One byte.

Because every frame saves the previous `rbp` at a known place, the saved values
form a linked list running back through the call stack. That chain is how a
debugger prints a backtrace.

Modern compilers usually **omit the frame pointer** (`-fomit-frame-pointer`,
which is on by default at `-O1` and above). They know each local's offset from
`rsp` statically, so `rbp` is freed up as a general register. The cost is that
the linked list is gone, and stack unwinding has to consult DWARF `.eh_frame`
tables instead. This is why a release build's backtrace is sometimes wrong, and
why profilers ask you to rebuild with frame pointers.

## Watch it happen

Recursion is where the stack earns its name. Trace `fact(3)`:

```text
    fact(3)  push 3        stack: [ret, 3]
      fact(2)  push 2      stack: [ret, 3, ret, 2]
        fact(1)            stack: [ret, 3, ret, 2, ret]      -> returns 1
      pop 2                rax = 1 * 2 = 2
    pop 3                  rax = 2 * 3 = 6
```

Three simultaneous live copies of `n`, each in its own frame, each found at the
same offset from its own `rsp`. No fixed address could have done that.

Run the factorial exercise below and scrub through the trace with the stack
viewer open. Watch `rsp` descend on the way in and climb back on the way out,
and watch the return addresses stack up.

## Key points

- The stack grows downwards. `push` subtracts then stores; `pop` loads then adds.
- `call` pushes the return address and jumps; `ret` pops it and jumps. Nothing
  more.
- The return address is ordinary writable memory. Unbalanced pushes corrupt it;
  so do overflowing buffers. Every stack mitigation descends from this fact.
- `rbp` marks a fixed base for locals; `leave` undoes a prologue in one byte.
  Optimised builds often omit it and use DWARF tables to unwind instead.
