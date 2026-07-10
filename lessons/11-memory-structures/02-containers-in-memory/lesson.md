+++
id = "containers-in-memory"
title = "Containers in Memory"
order = 2
estimated_minutes = 45
objectives = [
  "Recognise pointer-length-capacity triples used by vector-like containers",
  "Distinguish contiguous containers from pointer-linked containers",
  "Explain why hash tables and trees produce different memory access patterns",
]
prerequisites = ["arrays-strings-structs", "heap-allocation"]

[[exercises]]
id = "q-vec-triple"
kind = "quiz"
prompt = "What three facts does a typical vector-like container need?"
choices = ["Pointer, length and capacity", "Only a pointer", "Filename, mode and descriptor", "Base pointer, stack pointer and return address"]
answer = 0
explanation = "The pointer names the allocation, length is initialized elements, and capacity is allocation size in elements."

[[exercises]]
id = "q-list-cost"
kind = "quiz"
prompt = "Why is a linked list often slower to scan than an array even if both hold the same values?"
choices = ["A list node is always smaller", "Each step follows a pointer to another allocation, defeating contiguous cache-friendly access", "Arrays cannot be scanned", "Lists use no memory"]
answer = 1
explanation = "Arrays walk predictable adjacent memory. Lists chase pointers, and each node may live on a different cache line or page."

[[exercises]]
id = "e-capacity-left"
kind = "emulate"
prompt = "A vector has length 5 in rax and capacity 8 in rbx. Compute spare capacity in rax and halt with rax = 3."
starter = """
    mov rax, 5
    mov rbx, 8
    ; rax = capacity - length
    hlt
"""
solution = """
    mov rax, 5
    mov rbx, 8
    sub rbx, rax
    mov rax, rbx
    hlt
"""
expect_registers = { rax = 3 }
hints = ["Capacity minus length is how much room remains before growth."]

[[exercises]]
id = "q-hash-table"
kind = "quiz"
prompt = "What does a hash table usually compute before choosing where to look?"
choices = ["A hash of the key, reduced to a bucket index", "The return address", "The signedness of the pointer", "The executable segment"]
answer = 0
explanation = "Hash tables turn keys into bucket indexes, then handle collisions inside or near that bucket."
+++

# Containers in Memory

Containers are not magic. They are ordinary allocations plus metadata and
invariants. Once you know the shape, their machine code becomes recognizable.

## Vector-like containers

A vector is usually a pointer, a length, and a capacity. The pointer names a
contiguous allocation. The length counts initialized elements. The capacity says
how many elements fit before the allocation must grow.

Growth is expensive because it may allocate a larger chunk and copy or move the
old elements. That is why capacity exists separately from length.

## Strings

Many string types reuse the same idea: pointer plus length, sometimes with
capacity. Some add a small-string optimization, storing short text directly
inside the string object to avoid a heap allocation.

That optimization is great for performance and annoying in reverse engineering,
because the same type has two layouts selected by a tag or size bit.

## Lists, trees and hash tables

Linked lists store pointers to other nodes. Trees store child pointers and
ordering invariants. Hash tables store bucket arrays plus collision handling.

The access pattern is the giveaway. Arrays stride. Lists chase one next pointer.
Trees branch left or right. Hash tables compute a bucket and then probe or chase
inside it.

## Key points

- Vector-like containers are usually pointer, length and capacity.
- Contiguous layouts favor cache and simple addressing modes.
- Pointer-linked layouts favor insertion/removal but cost extra memory accesses.
- Container invariants are visible as comparisons, branches and pointer updates.
