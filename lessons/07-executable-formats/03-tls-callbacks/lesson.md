+++
id = "tls-callbacks"
title = "TLS Callbacks: Code Before main"
order = 3
estimated_minutes = 30
objectives = [
  "Explain what thread-local storage is and how the PE TLS directory declares per-thread data",
  "State the surprising fact that TLS callbacks run *before* the entry point, and on every thread create and exit",
  "Explain why packers and anti-debugging code hide in TLS callbacks, and how that defeats an entry-point breakpoint",
  "Read the reason code a callback receives and identify the DLL_PROCESS_ATTACH check",
]
prerequisites = ["pe-disk-to-memory"]

[[exercises]]
id = "q-what-tls"
kind = "quiz"
prompt = "What problem does thread-local storage (TLS) solve?"
choices = [
  "It encrypts a thread's stack",
  "It gives each thread its own private copy of a variable, so `errno`-style globals do not collide when many threads run the same code",
  "It stores the thread's return address",
  "It is where the loader keeps the import table",
]
answer = 1
explanation = "A plain global is shared by every thread, which is a disaster for something like `errno` or a per-thread buffer. TLS gives each thread its own instance of the variable, reached through a per-thread base register (gs on Windows x64). The PE TLS directory declares the template for that per-thread data so the loader can allocate and initialise a copy for each new thread."

[[exercises]]
id = "q-before-main"
kind = "quiz"
prompt = "The PE TLS directory holds AddressOfCallBacks — a null-terminated array of function pointers. When does the loader call them, relative to the program's entry point?"
choices = [
  "After main returns, during cleanup",
  "Before the entry point runs (for process attach), and again on every thread creation and exit",
  "Only if the program has no imports",
  "Never; they are optional metadata",
]
answer = 1
explanation = "TLS callbacks run *early*: the process-attach call happens before the executable's entry point, and each callback also fires on every thread create and thread exit, with a reason code saying which. That 'before the entry point' timing is the whole reason they matter to a reverse engineer."

[[exercises]]
id = "q-why-packers"
kind = "quiz"
prompt = "A reverse engineer sets a breakpoint on the entry point of a packed sample and runs it — but the anti-debugging check has already fired. Why is a TLS callback the classic hiding place for it?"
choices = [
  "TLS callbacks cannot be disassembled",
  "A TLS callback runs before the entry point, so any breakpoint placed at the entry point is reached too late — the callback's code has already executed",
  "TLS callbacks run with kernel privileges",
  "The debugger is forbidden from reading the TLS directory",
]
answer = 1
explanation = "Because the process-attach callback runs before the entry point, an analyst who breaks only at the entry point arrives after the callback has already done its work — detected the debugger, decrypted a stage, or bailed out. It is a favourite of packers and anti-analysis code for exactly that reason. The countermeasure is to break on the TLS callback itself (debuggers offer an option for this), or to read the TLS directory before running anything."

[[exercises]]
id = "d-reason-check"
kind = "disassemble"
prompt = "A TLS callback is called as `f(DllHandle, Reason, Reserved)` — on Windows x64 that puts Reason in `edx`. This tests it against DLL_PROCESS_ATTACH (== 1): `83 fa 01`. What is the instruction?"
hex = "83fa01"
expect_text = "cmp edx, 0x1"
hints = [
  "`83 /7` is `cmp` of a 32-bit register against a sign-extended 8-bit immediate; ModRM `fa` selects `edx`.",
  "A callback that only wants to act once, at process startup, compares the reason to 1 (DLL_PROCESS_ATTACH) and returns otherwise.",
]
+++

# TLS Callbacks: Code Before main

Every lesson so far has treated the **entry point** as where a program begins —
the loader finishes its work and jumps to `e_entry`, and execution starts. That
is very nearly true, and the exception is worth a whole lesson, because it is
both a genuinely useful language feature and one of the first tricks a reverse
engineer gets fooled by. Some code runs *before* the entry point. On Windows, the
usual vehicle is a **TLS callback**.

To understand the callback you first need the storage it is named after.

## Thread-local storage

A global variable is shared by every thread in a process. Usually that is what
you want. Sometimes it is a catastrophe: `errno`, a per-thread scratch buffer, a
"current transaction" pointer — if two threads run the same function and stomp on
one shared global, they corrupt each other.

**Thread-local storage** gives each thread its *own* copy of such a variable.
Declared `__declspec(thread)` on Windows (or `thread_local` in modern C/C++),
the variable lives not at one fixed address but at a per-thread offset from a
base the CPU keeps per thread — the `gs` segment base on Windows x64. Reading the
variable means "load from `gs:[offset]`", and because every thread has its own
`gs` base, every thread sees its own instance.

The PE describes this with **data directory 9, the TLS directory**. It holds the
template for the per-thread block — the initial bytes to copy, how large the
zero-filled area is — so that when a new thread starts, the loader can carve out
a fresh copy and initialise it. This is the ordinary, honest purpose of the TLS
directory, and most programs that use `thread_local` never think about it.

## The callback list

The TLS directory carries one more field, and this is the interesting one:
**AddressOfCallBacks**, a pointer to a null-terminated array of function
pointers.

```text
  TLS directory
    ├─ raw data (the per-thread template)
    ├─ index
    └─ AddressOfCallBacks ──► [ &cb0, &cb1, …, NULL ]
```

The loader calls each function in that array at defined moments in a thread's
life. A callback has the same signature as a DLL's entry point:

```c
VOID NTAPI TlsCallback(PVOID DllHandle, DWORD Reason, PVOID Reserved);
```

and `Reason` tells it which moment this is:

```text
  DLL_PROCESS_ATTACH = 1   the process is starting  ← runs BEFORE the entry point
  DLL_THREAD_ATTACH  = 2   a new thread was created
  DLL_THREAD_DETACH  = 3   a thread is exiting
  DLL_PROCESS_DETACH = 0   the process is ending
```

The intended use is bookkeeping: initialise or tear down per-thread state as
threads come and go, which the plain TLS template cannot do because it only
copies bytes — it cannot *run code* on thread creation. Callbacks fill that gap.

## Why this is the first trap in reversing

Read the timing again: the `DLL_PROCESS_ATTACH` callback runs **before the
program's entry point**. That single fact makes TLS callbacks a favourite hiding
place for code that does not want to be watched.

The standard first move against an unknown executable is to open it in a debugger
and set a breakpoint on the entry point — "let it get loaded, then stop before it
does anything." A TLS callback slips underneath that. By the time the entry-point
breakpoint is hit, the process-attach callback has already run: it may have
checked for a debugger and altered its behaviour, decrypted the next stage,
resolved imports by hand, or simply exited. The analyst sees a program that
behaves differently under the debugger, or that has already changed itself, and
the reason is not at the entry point at all.

A callback that only wants to act once, at startup, begins by checking the
reason code — the `cmp edx, 1` in the exercise below is that check, testing
`Reason` against `DLL_PROCESS_ATTACH`. Recognising it tells you a callback means
to fire on process start and ignore the thread events.

The countermeasures follow directly from the mechanism. Good debuggers offer a
"break on TLS callbacks" option that stops before *anything* user-controlled
runs. Static tools list the callback addresses straight from the TLS directory,
so you can read the code before executing a single instruction. The rule a
reverser internalises is simply: **the entry point is not necessarily the
beginning** — check the TLS directory first.

## The same idea elsewhere

"Code before main" is not unique to Windows. ELF has `.init_array` (and the older
`.init`) — a list of functions the loader runs before `main`, which is how C++
runs the constructors of global objects and how `__attribute__((constructor))`
works. C++ static initialisers land there. The mechanisms differ, but the lesson
is shared across both formats: a program's first instruction is rarely the entry
point, and anything that runs before it is both a legitimate initialisation hook
and a place for code that would rather you did not look.

## Key points

- **Thread-local storage** gives each thread a private copy of a variable,
  reached through the per-thread `gs` base; the PE TLS directory holds the
  template the loader copies for each new thread.
- The TLS directory's **AddressOfCallBacks** is a null-terminated list of
  functions the loader runs on process start, thread create, and thread exit —
  the `Reason` argument says which.
- The process-attach callback runs **before the entry point**, which is why
  packers and anti-debugging code hide there and why an entry-point breakpoint
  arrives too late.
- The same "code before `main`" exists on ELF as `.init_array`; on both formats,
  the entry point is not necessarily where execution begins.
