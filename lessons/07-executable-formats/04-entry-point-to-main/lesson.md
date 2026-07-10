+++
id = "entry-point-to-main"
title = "Entry Point to main"
order = 4
estimated_minutes = 35
objectives = [
  "Distinguish an executable's entry point from the source-level main function",
  "Trace the normal startup chain through loader work, runtime startup, initializers, main, and exit",
  "Explain why TLS callbacks and static constructors can run before main",
  "Recognise CRT startup frames in a debugger or backtrace without mistaking them for application logic",
]
prerequisites = ["elf-disk-to-memory", "pe-disk-to-memory", "tls-callbacks"]

[[exercises]]
id = "q-entry-main"
kind = "quiz"
prompt = "In a normal C or C++ executable, why is the PE AddressOfEntryPoint or ELF e_entry usually not the user's `main`?"
choices = [
  "The OS does not know the language's `main` contract; it jumps to an address, and the language runtime startup code later calls `main`",
  "The loader cannot execute code",
  "The source-level `main` always lives in a DLL",
  "The entry point is only used by debuggers",
]
answer = 0
explanation = "The executable format gives the loader an address. C and C++ add a higher-level contract: arguments, environment, static constructors, TLS, cleanup and exit. Runtime startup code bridges those two worlds before calling `main`."

[[exercises]]
id = "q-before-main"
kind = "quiz"
prompt = "Which of these may run before source-level `main`?"
choices = [
  "Only the first line of main",
  "TLS callbacks, runtime startup, and static/global constructors",
  "Only code in shared libraries after main returns",
  "Nothing; main is always the first user-mode instruction",
]
answer = 1
explanation = "The loader and runtime have work to do before `main`: resolve imports, apply relocations, run TLS callbacks, prepare the CRT, and run constructors."

[[exercises]]
id = "q-windows-entry-names"
kind = "quiz"
prompt = "On Windows, what does a subsystem and CRT choice affect?"
choices = [
  "Whether the binary has an IAT",
  "Which runtime startup stub is used and which user entry function it eventually calls, such as `main`, `wmain`, `WinMain`, or `wWinMain`",
  "Whether ASLR is disabled",
  "Whether the PE header starts with MZ",
]
answer = 1
explanation = "The OS still enters at AddressOfEntryPoint. The CRT startup variant adapts that low-level entry to the source-level function shape expected by the program."

[[exercises]]
id = "e-start-calls-main"
kind = "emulate"
prompt = "Model startup code calling `main`: call `main`, then halt with the value returned by `main` still in rax. Make `main` return 42."
starter = """
start:
    call main
    hlt

main:
    ; return 42 in rax
    ret
"""
solution = """
start:
    call main
    hlt

main:
    mov rax, 42
    ret
"""
expect_registers = { rax = 42 }
hints = [
  "The startup code is just another caller from the CPU's point of view.",
  "Return values use `rax` in both System V AMD64 and Microsoft x64 for integer-sized values.",
]
+++

# Entry Point to main

The executable header does not say "call `main`". It says "start executing at
this address." That address is the **entry point**: `e_entry` in ELF, or
`AddressOfEntryPoint` in a PE optional header.

For most C and C++ programs, that entry point is runtime startup code, not the
function the programmer wrote as `main`.

## What the loader promises

The loader's contract is low level. It maps the image, applies relocations,
resolves imports, sets up initial thread state, runs any loader-defined early
callbacks, and transfers control to one address.

It does not know C argument arrays, C++ constructors, standard I/O setup, or the
language's cleanup rules. Those are runtime responsibilities.

## What the runtime adds

The C runtime startup stub bridges the gap:

```text
loader entry point
  -> CRT startup
       -> initialize runtime state
       -> run static/global constructors
       -> prepare argc, argv, envp or Windows entry arguments
       -> call main / wmain / WinMain / wWinMain
       -> pass the return value to exit processing
```

That is why a debugger backtrace at process start often shows frames whose names
look like `_start`, `__libc_start_main`, `mainCRTStartup`, or
`__scrt_common_main_seh`. They are not your application's business logic; they
are the adapter from executable-format entry to language-level entry.

## Code before main

The TLS callback lesson already showed the most surprising Windows case: PE TLS
callbacks run before the entry point. C++ global constructors and ELF
`.init_array` functions are another ordinary source of code before `main`.

This is not suspicious by itself. It is how languages work. It only becomes an
analysis trap when you assume `main` is the beginning and ignore everything the
loader and runtime ran first.

## Windows entry names

Windows adds one more layer of vocabulary. Console programs commonly use `main`
or `wmain`; GUI programs commonly use `WinMain` or `wWinMain`. The PE subsystem
and CRT startup variant decide which source-level entry shape is expected.

The machine-level pattern is still the same: the loader enters the image at
`AddressOfEntryPoint`, and startup code eventually calls the user's entry
function.

## Key points

- The executable entry point is a machine address, not necessarily `main`.
- Runtime startup prepares language and library state before calling the user's
  entry function.
- TLS callbacks, constructors, and init arrays can run before `main`.
- Startup frames in a debugger are usually normal runtime scaffolding, not hidden
  application logic.
