+++
id = "syscalls-signals"
title = "Syscalls, Exceptions and Signals"
order = 1
estimated_minutes = 45
objectives = [
  "Describe a syscall as a controlled transition from user mode to kernel mode",
  "Distinguish syscalls, exceptions, interrupts and signals",
  "Explain how page faults and debugger traps reach user-space handlers or debuggers",
]
prerequisites = ["processes-and-files", "how-debuggers-work", "virtual-memory"]

[[exercises]]
id = "q-syscall"
kind = "quiz"
prompt = "What is a syscall?"
choices = ["A controlled entry into the kernel to request privileged work", "A normal function call into libc only", "A jump to physical memory", "An instruction that disables virtual memory"]
answer = 0
explanation = "Libraries may wrap syscalls, but the syscall itself is the CPU/kernel transition where the kernel validates and performs the request."

[[exercises]]
id = "q-exception"
kind = "quiz"
prompt = "A user process reads an unmapped page. What kind of event starts the kernel's involvement?"
choices = ["A timer interrupt", "A page-fault exception caused by the instruction", "A successful syscall", "A dynamic linker relocation"]
answer = 1
explanation = "The faulting instruction causes an exception. The kernel decides whether to map a page, deliver a signal, or terminate the process."

[[exercises]]
id = "q-signal"
kind = "quiz"
prompt = "What is a Unix signal in this context?"
choices = ["A CPU register", "A process-level notification delivered by the kernel, often reflecting an exception or external event", "An ELF section", "A branch predictor entry"]
answer = 1
explanation = "Signals are the Unix process abstraction for events such as SIGSEGV, SIGTRAP, SIGINT and timers."

[[exercises]]
id = "q-debugger-trap"
kind = "quiz"
prompt = "Why does a debugger care about SIGTRAP?"
choices = ["Breakpoints and single-step traps are reported through it on Unix-like systems", "It means stdout is closed", "It always means a heap allocation failed", "It is emitted by the assembler"]
answer = 0
explanation = "The CPU trap becomes a kernel event, and the kernel reports it to the traced process/debugger path as SIGTRAP."
+++

# Syscalls, Exceptions and Signals

User programs do not get to touch disks, devices, page tables, or other
processes directly. The kernel owns those privileges. OS interaction is the set
of controlled ways user mode crosses that boundary.

## Syscalls

A syscall is a request. The program places a syscall number and arguments where
the ABI says, executes the syscall instruction, and the CPU enters the kernel at
a configured entry point.

The kernel does not trust the process. It validates pointers, permissions,
descriptor numbers, sizes, and credentials before doing privileged work.

## Exceptions and interrupts

An exception is caused by the current instruction: page fault, divide error,
invalid opcode, breakpoint trap. An interrupt is caused by something external,
such as a timer or device.

Both enter the kernel, but their cause is different. That difference matters for
debugging: the instruction that faulted is evidence.

## Signals

Unix exposes many events to processes as signals. A page fault with no valid
resolution becomes SIGSEGV. A breakpoint trap becomes SIGTRAP. Ctrl-C commonly
arrives as SIGINT.

Signals are process-level delivery. The low-level CPU event has already been
handled enough by the kernel to decide what user space should see.

## Key points

- Syscalls are explicit requests for kernel work.
- Exceptions are caused by the current instruction; interrupts are external.
- Signals are Unix's process-level delivery mechanism for many events.
- Debuggers live on this boundary: traps, faults, register state and memory maps.
