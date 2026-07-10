+++
id = "got-and-plt"
title = "The GOT and the PLT"
order = 1
estimated_minutes = 35
objectives = [
  "Explain why a call to a shared-library function cannot be resolved at compile time",
  "Trace a call through the PLT and the GOT to the real function",
  "Describe lazy binding and what the GOT holds before and after the first call",
  "Say why the GOT is an attacker's target and how full RELRO answers it",
]
prerequisites = ["elf-disk-to-memory", "the-stack"]

[[exercises]]
id = "q-why-indirect"
kind = "quiz"
prompt = "Your program calls `puts`, which lives in libc. Why can the compiler not just emit `call <address of puts>`?"
choices = [
  "puts is too large to inline",
  "libc is loaded at a different, ASLR-randomised address every run, so its address is unknown until load time",
  "puts is written in a different language",
  "The call would be too slow",
]
answer = 1
explanation = "The address of puts is not known when your program is compiled, or even when it is linked — libc is a separate file, mapped at a random address by ASLR at load time. The call must go through a level of indirection that the loader fills in."

[[exercises]]
id = "q-got-holds"
kind = "quiz"
prompt = "The Global Offset Table (GOT) is a table of what?"
choices = [
  "Function source code",
  "Runtime addresses of imported symbols, filled in by the loader / dynamic linker",
  "The program's own function addresses only",
  "Relocation records",
]
answer = 1
explanation = "The GOT is an array of pointers, one per imported symbol. It starts out pointing at the resolver; after resolution each slot holds the real runtime address of its symbol. Code reaches an import by loading its address *from* the GOT rather than hard-coding it."

[[exercises]]
id = "q-lazy"
kind = "quiz"
prompt = "With lazy binding (the default), when is `puts`'s real address written into its GOT slot?"
choices = [
  "At compile time",
  "At program load, before main runs",
  "On the first call to puts, by the resolver the PLT jumps to",
  "Never; it is resolved on every call",
]
answer = 2
explanation = "Lazy binding defers resolution until first use. Before the first call, the GOT slot points back into the PLT at the resolver stub. The first call runs the resolver, which finds puts and *overwrites* the GOT slot, so every later call goes straight through. It trades a one-time cost per symbol for not resolving symbols that are never called."

[[exercises]]
id = "d-plt-jump"
kind = "disassemble"
prompt = "The heart of a PLT stub is this indirect jump through the GOT: `ff 25 e2 2f 00 00`. What is it?"
hex = "ff25e22f0000"
expect_text = "jmp qword [rip+0x2fe2]"
hints = [
  "`ff /4` is an indirect `jmp`; the ModRM `25` selects the RIP-relative memory form.",
  "It jumps to the *address stored at* `[rip+0x2fe2]` — that memory is the symbol's GOT slot.",
]
+++

# The GOT and the PLT

When your program calls `printf`, it is calling into libc — a separate file that
your program does not contain. The address of `printf` is not known when your
code is compiled (libc is a different project), not known when your program is
linked (libc is still a separate file), and not even fixed at load time (ASLR
maps libc somewhere random). Yet the `call` in your code must go *somewhere*.

The answer is a level of indirection filled in at runtime, built from two
tables: the **PLT** and the **GOT**. Understanding them explains a large fraction
of what you see disassembling any real dynamically-linked binary.

## The problem, precisely

A direct `call printf` would need `printf`'s address baked into the instruction.
That address is unknowable at build time and different every run. So the
compiler cannot emit it. What it *can* emit is "call a fixed local stub that
knows how to find printf" — and that stub is the PLT entry.

## Two tables

- The **GOT** (Global Offset Table) is an array of pointers in the writable data
  segment. It has one slot per imported symbol. The loader (or the lazy
  resolver) writes each symbol's real runtime address into its slot.
- The **PLT** (Procedure Linkage Table) is an array of tiny code stubs in the
  executable segment, one per imported symbol. A stub's whole job is to jump to
  wherever the GOT slot points.

Your code never calls `printf` directly. It calls `printf@plt` — a fixed address
inside your own image — and that stub does:

```asm
printf@plt:
    jmp qword [rip + printf_got_offset]   ; jump to whatever the GOT slot holds
    ...
```

That `jmp qword [rip+...]` — the instruction in the exercise — is an *indirect*
jump: it does not jump to the operand, it jumps to the address *stored at* the
operand. The operand addresses the GOT slot; the slot holds `printf`. One level
of indirection, and the only thing that changed per run is the pointer in the
data segment, not a single byte of code.

## Lazy binding: paying only for what you call

A large program imports hundreds of symbols and calls only some of them.
Resolving all of them at startup would be wasted work, so the default is **lazy
binding**: resolve each symbol the first time it is actually called.

The trick is that the GOT slot starts out pointing *back into the PLT*, at a
resolver stub:

```text
   before the first call to printf:
     printf@plt:  jmp [GOT_printf]   ─┐
     GOT_printf:  ──────────────────┘  (points back to the resolver in the PLT)
                                        the resolver finds printf, then
                                        OVERWRITES GOT_printf with its real address

   after the first call:
     printf@plt:  jmp [GOT_printf]   ──►  the real printf in libc
     GOT_printf:  &printf
```

So the first call is slow — it runs the dynamic linker's symbol lookup — and
every call after it is a single indirect jump straight to libc. This is why the
first use of a library function in a fresh process is measurably slower than the
rest, and why profilers show a one-time cost at a call site's first hit.

You can turn this off. Linking with "bind now" (full RELRO, next section)
resolves every symbol at load, trading startup time for predictability.

## Why the GOT is a target

The GOT is *writable* — it must be, so the loader can fill it in. And it holds
*code pointers* that `jmp`/`call` instructions dereference. To an attacker with a
memory-write primitive, that is irresistible: overwrite the GOT slot for `printf`
with the address of `system`, and the program's next `printf("...")` calls
`system("...")`. No code was modified — NX is untouched — yet control flow is
hijacked.

The defence is **RELRO** (Relocation Read-Only). *Partial* RELRO puts the parts
of the GOT resolved at startup into a read-only page. *Full* RELRO resolves
everything at load (no lazy binding) and then makes the *entire* GOT read-only,
so there is nothing left to overwrite. The cost is slower startup; the benefit is
that the most popular control-flow-hijack target in the binary becomes immutable.
The **Inspector** reports a binary's RELRO level for exactly this reason.

## Windows says the same thing differently

Windows uses the **Import Address Table** (IAT), which is the GOT by another
name: an array of pointers the loader fills with the addresses of functions
imported from DLLs. `LoadLibrary` maps a DLL, `GetProcAddress` looks up a
function within it — the explicit, programmable version of what the dynamic
linker does implicitly. **Delay-loaded** imports are Windows' lazy binding: the
DLL is not even mapped until the first call to one of its functions. The
Inspector lists both an ELF's relocations and a PE's imports, including
delay-loads.

## Key points

- A shared-library function's address is unknown until load time, so calls to it
  go through indirection the loader fills in.
- The **PLT** is per-symbol code stubs; the **GOT** is per-symbol pointers. Code
  calls the PLT stub, which jumps to whatever the GOT slot holds.
- **Lazy binding** resolves a symbol on its first call and caches the result in
  its GOT slot; the first call is slow, the rest are one indirect jump.
- The GOT is writable and holds code pointers, making it a hijack target;
  **full RELRO** answers by resolving everything up front and freezing the GOT.
