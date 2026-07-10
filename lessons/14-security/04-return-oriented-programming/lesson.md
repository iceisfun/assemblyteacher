+++
id = "return-oriented-programming"
title = "Return-Oriented Programming"
order = 4
estimated_minutes = 45
objectives = [
  "Explain what a gadget is and why a sequence ending in ret can be chained through the stack",
  "Account for a gadget's side effects — the registers it clobbers and the stack slots it consumes",
  "Explain why variable-length decoding makes the usable gadget set far larger than the code the compiler emitted",
  "Map each modern mitigation to the specific step of a reuse attack it raises the cost of — especially CET shadow stacks against ret-chaining",
]
prerequisites = ["exploit-mitigations", "the-stack", "aob-scanning"]

[[exercises]]
id = "q-what-gadget"
kind = "quiz"
prompt = "In return-oriented programming, what is a 'gadget', and why can gadgets be chained?"
choices = [
  "A shellcode payload injected onto the stack",
  "A short sequence of existing instructions ending in `ret`; because `ret` pops the next address off the stack and jumps to it, a stack full of gadget addresses runs each in turn",
  "A CPU instruction that disables NX",
  "A function in libc that spawns a shell",
]
answer = 1
explanation = "A gadget is a short run of instructions that already exists in executable memory and ends in `ret` (or another indirect branch). Since `ret` is `pop rip`, it takes its next destination from the stack. Lay out a list of gadget addresses on the stack and each gadget does its small step, then `ret`s straight into the next — a program written in the language of 'addresses of existing code.' No new code is executed, so NX is satisfied throughout."

[[exercises]]
id = "q-clobber"
kind = "quiz"
prompt = "An attacker wants a gadget that just loads `rdi`, but the cleanest one available is `pop rdi ; pop rbp ; ret`. Why does the extra `pop rbp` complicate the chain?"
choices = [
  "It does not — extra pops are harmless",
  "The gadget now consumes TWO stack slots and overwrites rbp, so the chain must supply a filler value for the second pop and can no longer rely on rbp — every side effect has to be accounted for or the chain derails",
  "It makes the gadget non-executable",
  "It triggers ASLR",
]
answer = 1
explanation = "A gadget does its one useful thing plus every side effect of the instructions around it. `pop rdi ; pop rbp ; ret` eats two stack entries, not one, and clobbers rbp. The chain has to place a throwaway value where the second `pop` will land, and treat rbp as destroyed. This bookkeeping — which registers each gadget clobbers and how many stack slots it consumes — is most of what makes chaining fiddly, and why short, side-effect-free gadgets are prized."

[[exercises]]
id = "q-unaligned"
kind = "quiz"
prompt = "Tools find far more gadgets in a binary than the compiler ever emitted as instructions. How, given the code is fixed?"
choices = [
  "They modify the code section at runtime",
  "x86 instructions are variable-length and unaligned, so decoding starting from a byte in the MIDDLE of an instruction yields a different, valid instruction stream — one the compiler never intended, often ending in a `c3` (ret) byte hidden inside a larger instruction",
  "They use undocumented CPU instructions",
  "The linker inserts spare gadgets",
]
answer = 1
explanation = "Because x86-64 instructions are 1–15 bytes with no alignment, the CPU will happily decode from any offset. Starting one byte into an instruction produces a completely different sequence — and `c3` (ret) bytes hide inside immediates, ModRM bytes and displacements all over the code. So the *effective* gadget set is much larger than the disassembly you see, which is exactly why gadgets are so hard to eliminate and why defenders turned to hardware (shadow stacks) rather than trying to scrub the code. This is the variable-length decoding you met in the AOB lesson, turned to a new purpose."

[[exercises]]
id = "q-mitigation-step"
kind = "quiz"
prompt = "Modern defenses each attack a different step of a reuse attack. Which one most directly breaks the ret-chaining mechanic itself?"
choices = [
  "ASLR — it randomizes where the gadgets are",
  "A CET shadow stack — the CPU keeps a protected second copy of each return address and faults on `ret` if the two disagree, so a return address planted on the stack no longer matches",
  "NX — it makes the stack non-executable",
  "A stack canary — it detects the overflow",
]
answer = 1
explanation = "Each mitigation raises the cost of a different step. ASLR hides *where* gadgets are (forcing an info leak first); NX is what forced reuse in the first place; a canary catches the overflow that *delivers* the chain. But the ret-chain mechanic itself — `ret` taking its target from the attacker-controlled stack — is what the CET **shadow stack** breaks directly: the CPU shadow-copies every return address and compares on `ret`, so a planted address mismatches and faults. Its sibling, IBT (`endbr64` landing pads), does the same for indirect *jumps/calls*, closing the jump-oriented variant."

[[exercises]]
id = "d-load-gadget"
kind = "disassemble"
prompt = "This single byte is the body of the most common register-loading gadget (its `ret` would follow). `5f`. What is the instruction?"
hex = "5f"
expect_text = "pop rdi"
hints = [
  "`58`–`5f` are the one-byte `pop r64` opcodes; `5f` selects rdi.",
  "Followed by a `ret` (`c3`), `5f c3` is the gadget `pop rdi ; ret` — load the first argument register, then chain onward.",
]
+++

# Return-Oriented Programming

The mitigations lesson told this story once already, and it is worth re-reading
its NX section before this one. The one-sentence recap: NX made injected
shellcode unrunnable, so attackers stopped injecting code and started **reusing**
the executable code already in the process — stitching together short snippets
that each end in `ret`. That is return-oriented programming. This lesson goes
under the hood of the technique: what a gadget really is, the discipline of
chaining one, why the supply of gadgets is far larger than it looks, and — the
part that matters most for reading a modern binary — exactly which defense breaks
which step.

Nothing here is a recipe for building an exploit. It is the mechanism, at the
level you need to recognise it in a disassembly and to understand why a hardened
binary is a hard target.

## The gadget, precisely

A **gadget** is a short run of instructions that (a) already exists in an
executable page and (b) ends in a `ret` — or another indirect branch. Recall
what `ret` does: it is `pop rip`. It takes the 8 bytes at the top of the stack,
loads them into the instruction pointer, and jumps. It trusts the stack
completely.

So if an attacker controls the stack — which a buffer overflow gives them — they
can lay out a list of addresses, each pointing at a gadget. The first `ret`
jumps to the first gadget; that gadget does its small step and ends in its own
`ret`, which pops the *next* address and jumps there; and so on down the list.
The stack has become a little program, and its instructions are "the addresses of
snippets of the program's own code."

Gadgets fall into a rough vocabulary:

```text
  pop rdi ; ret            load a register (here, the 1st argument register)
  mov [rax], rbx ; ret     write rbx to the address in rax   (memory store)
  add rsp, 0x18 ; ret      move the stack pointer             (skip / pivot)
  xor eax, eax ; ret       zero a register
  syscall ; ret            make a system call
```

With enough of these, the chain can load argument registers and call a function
or a syscall — the usual goal being to undo the very defenses in place (for
example, call `mprotect`/`VirtualProtect` to make a page executable again) or to
call something like `execve` directly. The details of assembling such a chain are
not our subject; the *shape* — a stack of code addresses driving borrowed
snippets — is.

## Clobbering: every side effect counts

The catch, and the reason chaining is fiddly, is that a gadget does its one
useful thing **plus** the side effects of the instructions bundled with it. A
clean `pop rdi ; ret` consumes exactly one stack slot and touches only rdi. But
the gadget you actually find might be `pop rdi ; pop rbp ; ret`: now it eats
**two** stack slots, so the chain must place a filler value where the second
`pop` lands, and it destroys rbp, so nothing later in the chain can depend on
rbp surviving.

Multiply that across a dozen gadgets and you see the discipline: for every gadget
you must track which registers it clobbers and how many stack entries it
consumes, or the chain desynchronises — the next `ret` pops the wrong slot and
the whole thing veers off. This is why attackers prize short, single-purpose
gadgets, and why gadget selection is the tedious heart of the technique. For a
reverse engineer, it is also a tell: a `ret` reached with a strangely arranged
stack, or a run of `pop`s consuming values that look like addresses, is the
texture of a chain.

## Why there are so many gadgets

You might think a program only offers as many gadgets as it has `ret`
instructions. It offers far more, and the reason is the same variable-length
encoding you met in the AOB lesson. x86-64 instructions are 1–15 bytes and
**unaligned** — the CPU will decode from any byte you point it at. Start decoding
one byte into an instruction and you get a *different*, equally valid instruction
stream that the compiler never emitted.

And `ret` is a single byte, `c3`, which occurs by accident all over the code —
inside immediates, ModRM bytes, displacements. Every stray `c3` is a potential
gadget terminator, and the few bytes before it are a potential gadget. So the
usable gadget set is much larger than the disassembly listing suggests, and
scrubbing a compiler's *intended* instructions does little. This is the key
reason defenders stopped trying to remove gadgets and moved the defense into
hardware — which is the next section.

The variants follow the same logic under different constraints: **ret2libc**
returns straight into a whole library function instead of small gadgets;
**JOP** (jump-oriented programming) chains `jmp`-terminated gadgets when `ret` is
defended; **COP** uses call-terminated ones. Different terminators, same idea:
borrow existing executable code.

## Which defense breaks which step

The mitigations lesson introduced these as a chain of obstacles. Seen through the
lens of a reuse attack, each one raises the cost of a *specific* step — and this
mapping is the useful thing to carry away:

| step of the attack | the defense that raises its cost |
|---|---|
| get the injected bytes to run as code | **NX / DEP** — the reason reuse exists at all |
| deliver the chain via a stack overflow | **stack canary** — mismatch detected before `ret` |
| deliver it via a Windows SEH overwrite | **SafeSEH / SEHOP** — validates the handler chain |
| know *where* the gadgets are | **ASLR / PIE** — randomizes bases; forces an info leak first |
| redirect an indirect **call** to a gadget | **CFG / CFI** — calls must target valid function entries |
| chain through **`ret`** at all | **CET shadow stack** — hardware second copy of the return address |
| chain through indirect **jumps** (JOP) | **CET IBT** — indirect branches must land on an `endbr64` |
| overwrite the GOT to redirect a call | **RELRO** — makes the table read-only after start-up |

Two of these are worth singling out because they attack ROP at its root rather
than around the edges. The **CET shadow stack** keeps a second, protected copy of
every return address that only `call` and `ret` maintain; on each `ret` the CPU
compares the two, and a return address an attacker planted on the ordinary stack
will not match the shadow copy — so it faults. That is the ret-chain mechanic
itself denied. **IBT** does the analogous thing for indirect jumps and calls:
they must land on an `endbr64` instruction (the landing pad you will see at the
top of functions), so a jump into the middle of one — the essence of a
gadget — is rejected. Between them, the borrowed-code trick that NX created is,
on hardware that enforces CET, largely closed.

## Key points

- A **gadget** is a short existing sequence ending in `ret`; because `ret` is
  `pop rip`, a stack of gadget addresses executes each in turn — reuse, not
  injection, so NX is satisfied.
- Every gadget carries **side effects**: the registers it clobbers and the stack
  slots it consumes must all be accounted for, or the chain desynchronises.
- Because x86 is **variable-length and unaligned**, decoding from mid-instruction
  exposes many more gadgets than the compiler emitted — which is why the defense
  moved into hardware.
- Each mitigation breaks a **different step**; the **CET shadow stack** breaks the
  `ret`-chain directly, and **IBT/`endbr64`** breaks the jump-oriented variant.
