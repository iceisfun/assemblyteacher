+++
id = "threads-and-synchronization"
title = "Threads and Synchronization"
order = 2
estimated_minutes = 35
objectives = [
  "Distinguish process isolation from threads sharing one address space",
  "Explain a data race at the instruction level",
  "Describe why locks and atomic operations exist",
]
prerequisites = ["processes-and-files", "first-instructions"]

[[exercises]]
id = "q-thread-sharing"
kind = "quiz"
prompt = "What do two threads in the same process normally share?"
choices = ["The same virtual address space", "The same stack pointer", "The same instruction pointer", "No memory at all"]
answer = 0
explanation = "Threads have their own register state and stacks, but they share the process address space."

[[exercises]]
id = "q-race"
kind = "quiz"
prompt = "Why is `counter++` not automatically safe between threads?"
choices = ["It is usually load, add, store; another thread can interleave between those steps", "The CPU refuses to add shared values", "Stacks cannot hold counters", "Only the compiler can modify memory"]
answer = 0
explanation = "At the machine level, one source statement becomes multiple effects. The race lives in the gap between them."

[[exercises]]
id = "q-lock"
kind = "quiz"
prompt = "What does a lock protect?"
choices = ["A region of code by itself", "A shared invariant or resource that must not be updated by two threads at once", "The CPU pipeline", "Only stack memory"]
answer = 1
explanation = "The lock is not the data. It is the rule everyone agrees to follow before touching the protected data."

[[exercises]]
id = "e-lost-update"
kind = "emulate"
prompt = "Simulate one thread's half of `counter++`: load 41 from memory into eax, add one, and halt with rax = 42. This is safe alone but not safe when another thread can do the same load before the store."
starter = """
    mov rax, 41
    ; increment the loaded copy
    hlt
"""
solution = """
    mov rax, 41
    add rax, 1
    hlt
"""
expect_registers = { rax = 42 }
hints = ["The point is that the register copy is private, while the memory counter is shared."]
+++

# Threads and Synchronization

A process gives isolation from other processes. A thread is a separate path of
execution inside that process. Threads share the address space, so they can
communicate cheaply, and they can corrupt each other's assumptions cheaply too.

## Shared memory, private registers

Each thread has its own `rip`, general-purpose registers, and stack. The heap,
globals, mappings, and file descriptors belong to the process and are shared.

That split is the whole model: private execution state, shared memory.

## Races

`counter++` looks indivisible in a high-level language. At the machine level it
is a sequence: load the old value, add one, store the new value. If two threads
both load the old value before either stores, one increment disappears.

The bug is not that addition is wrong. The bug is that the larger operation was
not atomic.

## Locks and atomics

A lock serializes a critical section: only one thread at a time may update the
protected invariant. Atomic operations ask the hardware to make a small read-
modify-write sequence indivisible with respect to other cores.

Both are about creating one visible order where the program requires one.

## Key points

- Threads share a process address space but keep private register state.
- Races appear when multi-instruction updates interleave.
- Locks protect invariants, not just lines of code.
- Atomic operations are hardware-supported indivisible memory operations.
