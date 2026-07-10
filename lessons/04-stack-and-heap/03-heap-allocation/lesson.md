+++
id = "heap-allocation"
title = "Heap Allocation"
order = 3
estimated_minutes = 40
objectives = [
  "Explain why the heap exists when the stack already holds memory",
  "Describe chunks, metadata, allocation, free and reuse",
  "Distinguish leaks, use-after-free, double-free and heap overflows",
]
prerequisites = ["the-stack", "calling-conventions"]

[[exercises]]
id = "q-why-heap"
kind = "quiz"
prompt = "Why allocate an object on the heap instead of the stack?"
choices = ["Heap memory is always faster", "The object must outlive the current call frame or have a size/lifetime not known by the caller", "The stack cannot store pointers", "The heap has no addresses"]
answer = 1
explanation = "Stack storage is tied to a call frame. Heap storage is explicit: it lives until the allocator is asked to release it."

[[exercises]]
id = "q-uaf"
kind = "quiz"
prompt = "What is a use-after-free?"
choices = ["Forgetting to free memory", "Freeing the same pointer twice", "Using a pointer after its chunk has been returned to the allocator", "Writing one byte past a stack buffer"]
answer = 2
explanation = "After `free`, the pointer value may still exist, but ownership of the chunk has gone back to the allocator. Reuse can make the stale pointer refer to a different object."

[[exercises]]
id = "e-bump-allocator"
kind = "emulate"
prompt = "Implement a tiny bump allocator step: start with heap pointer 0x1000 in rax, allocate 24 bytes by returning the old pointer in rbx and advancing rax to 0x1018. Halt with rax = 0x1018 and rbx = 0x1000."
starter = """
    mov rax, 0x1000
    ; rbx should receive the returned pointer
    ; rax should advance by 24 bytes
    hlt
"""
solution = """
    mov rax, 0x1000
    mov rbx, rax
    add rax, 24
    hlt
"""
expect_registers = { rax = 4120, rbx = 4096 }
hints = ["A bump allocator returns the current frontier, then moves the frontier forward."]

[[exercises]]
id = "q-overflow-target"
kind = "quiz"
prompt = "Why can a heap overflow corrupt a different object instead of a return address?"
choices = ["Heap chunks are adjacent to other heap chunks and allocator metadata", "The heap stores return addresses", "The stack and heap are the same region", "Heap memory cannot be corrupted"]
answer = 0
explanation = "A heap write past the end walks into whatever chunk or metadata the allocator placed next. The target is layout-dependent."
+++

# Heap Allocation

The stack is automatic. A call creates a frame, and returning destroys it. That
is perfect for temporary locals and return addresses, but wrong for data whose
lifetime crosses calls or whose size is chosen at runtime.

The heap is the answer: a region managed by an allocator rather than by `call`
and `ret`.

## Chunks

Allocators do not hand out abstract objects. They hand out ranges of bytes,
usually called chunks or blocks. Around those chunks they keep metadata: size,
state, links in free lists, or enough information to coalesce adjacent free
space.

```text
metadata | user bytes ... | metadata | user bytes ...
```

The pointer returned to the program normally points at the user bytes, not the
metadata. But the metadata is nearby, so out-of-bounds writes matter.

## Allocation and free

A minimal allocator can be a bump pointer: return the current frontier and move
it forward. Real allocators also need `free`, which means they must remember
which chunks can be reused.

Reuse is where heap bugs become interesting. A freed address can later hold a
different object with the same address but a completely different meaning.

## Lifetime bugs

A leak loses the only pointer to live allocation. A double-free returns the same
chunk twice. A use-after-free keeps using a stale pointer after the allocator has
retaken ownership. A heap overflow writes beyond one chunk into the next chunk or
the allocator's bookkeeping.

The common thread is ownership. The pointer is just a number; the allocator's
state decides whether that number still names an object you own.

## Key points

- Stack lifetime follows calls; heap lifetime follows explicit allocation and free.
- Allocators manage chunks plus metadata, not language-level objects.
- Freed addresses can be reused for unrelated data.
- Heap bugs are ownership and layout bugs.
