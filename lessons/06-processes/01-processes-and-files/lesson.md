+++
id = "processes-and-files"
title = "Processes and File Descriptors"
order = 1
estimated_minutes = 40
objectives = [
  "Describe a process as an address space, register state and kernel resource table",
  "Explain why file descriptors are small per-process handles rather than pointers",
  "Distinguish fork-style copying from exec-style replacement",
]
prerequisites = ["address-space-layout"]

[[exercises]]
id = "q-process"
kind = "quiz"
prompt = "What does a process primarily give a running program?"
choices = ["A private virtual address space and kernel-tracked resources", "A dedicated CPU forever", "Direct access to physical memory", "A copy of the compiler"]
answer = 0
explanation = "The process abstraction combines virtual memory with kernel bookkeeping: open files, credentials, signal state, threads and more."

[[exercises]]
id = "q-fd"
kind = "quiz"
prompt = "Why is file descriptor 1 conventionally stdout but not a pointer to stdout?"
choices = ["It is an index into the process's kernel-managed descriptor table", "It is the address of the terminal buffer", "It is always a socket", "It is a CPU register"]
answer = 0
explanation = "A descriptor is a small integer handle. The kernel resolves it through the process's descriptor table."

[[exercises]]
id = "q-exec"
kind = "quiz"
prompt = "What does exec-style program loading do to the current process image?"
choices = ["Creates a second process and keeps the old code running", "Replaces the current address space with a new program image while preserving selected process resources", "Copies every page into the parent", "Only changes rip"]
answer = 1
explanation = "`execve` keeps the process identity but replaces the program image. Some resources survive by rule; memory mappings from the old image do not."

[[exercises]]
id = "e-fd-table"
kind = "emulate"
prompt = "Model a descriptor lookup: rdi holds fd 1, a table base is 0x1000, each entry is 8 bytes. Compute the entry address in rax and halt with rax = 0x1008."
starter = """
    mov rdi, 1
    mov rax, 0x1000
    ; add fd * 8
    hlt
"""
solution = """
    mov rdi, 1
    mov rax, 0x1000
    lea rax, [rax + rdi*8]
    hlt
"""
expect_registers = { rax = 4104 }
hints = ["A descriptor table lookup is naturally base + index * entry_size."]
+++

# Processes and File Descriptors

A process is not just code that is running. It is the operating system's record
of a running program: virtual memory, register state, credentials, open files,
signal dispositions, threads, and resource limits.

## Isolation

Two processes can both use address `0x401000` without sharing the same physical
memory. Virtual memory makes each process see its own map. The kernel switches
which map the MMU uses when it switches between processes.

That is why a crash in one ordinary process does not scribble over another
ordinary process. The page tables do not give it permission.

## File descriptors

File descriptors are small integers in user space and table indexes in the
kernel. `0`, `1`, and `2` are conventionally stdin, stdout, and stderr, but the
numbers themselves are not devices. They are handles.

The useful consequence is redirection. If descriptor 1 points at a terminal,
`write(1, ...)` prints to the terminal. If descriptor 1 points at a pipe or file,
the same machine code writes there instead.

## fork and exec

Unix process creation is usually two ideas composed: `fork` creates a new
process that begins as a copy of the old one, and `execve` replaces the current
program image with a different executable.

Modern kernels avoid copying every page immediately. Copy-on-write lets parent
and child share pages until one side writes.

## Key points

- A process is an address space plus kernel-owned state.
- File descriptors are per-process handles, not pointers.
- Redirection works because programs write to descriptors, not to terminals directly.
- `fork` copies a process view; `exec` replaces the program image.
