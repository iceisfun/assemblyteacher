+++
id = "cet-shadow-stacks"
title = "CET, Shadow Stacks, and Indirect Branch Tracking"
order = 7
estimated_minutes = 40
objectives = [
  "Explain how a shadow stack protects return addresses by keeping a second hardware-tracked copy",
  "Trace why a corrupted normal return address faults when `ret` compares it with the shadow-stack entry",
  "Explain why ordinary tail calls do not violate shadow-stack rules even though they remove a frame from the backtrace",
  "Distinguish return protection from indirect branch tracking, and read `endbr64` as an indirect-branch landing pad",
]
prerequisites = ["exploit-mitigations", "return-oriented-programming", "tail-calls"]

[[exercises]]
id = "q-what-shadow-stack"
kind = "quiz"
prompt = "What is the key idea behind a CET shadow stack?"
choices = [
  "Encrypt every instruction pointer before it is used",
  "Keep a protected second copy of return addresses, so `ret` can compare the normal stack return address against the hardware-tracked one",
  "Move all local variables off the stack",
  "Require every function to use rbp as a frame pointer",
]
answer = 1
explanation = "A shadow stack is a second return-address stack managed by the CPU. A normal `call` still pushes a return address on the ordinary stack, but CET also records the expected return on the shadow stack. On `ret`, the CPU checks that the ordinary return address matches the shadow entry. If they differ, the processor raises a control-protection fault instead of jumping to the attacker-controlled address."

[[exercises]]
id = "q-rop-break"
kind = "quiz"
prompt = "Why does a shadow stack directly break classic ROP chains built from gadgets ending in `ret`?"
choices = [
  "Because gadgets are no longer executable when CET is enabled",
  "Because the chain's planted stack addresses do not match the protected shadow-stack returns, so the first mismatching `ret` faults",
  "Because `pop` instructions become illegal",
  "Because ASLR is automatically perfect when CET is enabled",
]
answer = 1
explanation = "A classic ROP chain works by placing a list of gadget addresses on the normal stack and letting each `ret` pop the next one. With a shadow stack, `ret` no longer trusts that normal stack slot by itself. The planted gadget address must match the protected shadow-stack entry for that call depth; it usually will not, so the CPU faults at the return edge."

[[exercises]]
id = "q-tail-call"
kind = "quiz"
prompt = "Why does an ordinary compiler tail call (`jmp target` in tail position) not by itself violate a shadow stack?"
choices = [
  "Because shadow stacks ignore optimized code",
  "Because a tail call is a `jmp`, not a `call`: it pushes no new return address, so the tail-called function eventually returns against the original caller's existing shadow-stack entry",
  "Because `jmp` copies the normal stack into the shadow stack",
  "Because tail calls are disabled whenever CET is enabled",
]
answer = 1
explanation = "The shadow stack tracks call/return nesting. A tail call deliberately does not add another call level: the caller has no work left, tears down its frame if it had one, and jumps. Since no `call` happened, there is no extra return address to expect. The tail-called function's `ret` returns to the original caller, matching the shadow entry created by the original `call`. The backtrace is flatter, but the return discipline is still consistent."

[[exercises]]
id = "q-ibt"
kind = "quiz"
prompt = "CET also includes indirect branch tracking (IBT). What does IBT check?"
choices = [
  "That every indirect `call` or `jmp` lands on a valid landing pad, normally an `endbr64` instruction",
  "That every `ret` target is inside the current function",
  "That the stack pointer is 16-byte aligned after every instruction",
  "That all direct branches go through the PLT",
]
answer = 0
explanation = "Shadow stacks protect return edges. IBT protects indirect call/jump edges. When IBT is enabled, an indirect branch must land at a location the compiler or loader marked as a legal target, represented on x86-64 by `endbr64` (`f3 0f 1e fa`). Direct branches are not the point of IBT, and `ret` is handled by the shadow-stack half of CET."

[[exercises]]
id = "e-shadow-compare"
kind = "emulate"
prompt = "Simulate the shadow-stack check. `victim` is entered by `call`, then its ordinary return address at `[rsp]` is corrupted to `bad_target`. Keep the expected return address (`after`) in `rbx` as the protected shadow copy, compare `[rsp]` against it, and halt with `rax = 1` if the mismatch is detected before `ret`."
starter = """
    call victim
after:
    hlt
bad_target:
    mov rax, 99
    hlt
victim:
    lea rbx, [rip+after]       ; protected shadow copy of the expected return
    lea rcx, [rip+bad_target]
    mov [rsp], rcx             ; attacker corrupted the normal return slot
    ; compare [rsp] with rbx; on mismatch, report detection with rax = 1
    ret
"""
solution = """
    call victim
after:
    hlt
bad_target:
    mov rax, 99
    hlt
victim:
    lea rbx, [rip+after]
    lea rcx, [rip+bad_target]
    mov [rsp], rcx
    mov rax, [rsp]
    cmp rax, rbx
    jne detected
    xor eax, eax
    ret
detected:
    mov rax, 1
    hlt
"""
expect_registers = { rax = 1 }
hints = [
  "The normal return slot is `[rsp]`; `mov [rsp], rcx` makes it point at `bad_target` instead of `after`.",
  "Treat `rbx` as the protected shadow-stack copy. Load `[rsp]` into `rax`, compare it with `rbx`, and branch on `jne`.",
  "On mismatch, do not execute `ret`; halt with `rax = 1` to model the fault a real shadow stack would raise.",
]
+++

# CET, Shadow Stacks, and Indirect Branch Tracking

The last few security lessons circled the same weak point from different angles:
the return address. A stack overflow overwrites it. ROP turns it into a list of
gadget destinations. Caller validation tries to learn something from it. Tail
calls show why it is not even a perfect record of the source-level call graph.

Intel **Control-flow Enforcement Technology (CET)** is hardware aimed at that
whole family of problems. It does not make memory corruption disappear. Instead,
it asks a narrower question at the exact moment control flow is about to move:

> Is this edge one the program could have reached through the normal call and
> branch machinery?

CET has two halves on x86-64:

- **Shadow stacks** protect `ret` edges.
- **Indirect branch tracking (IBT)** protects indirect `call` and `jmp` edges.

They solve different problems. Keeping that separation clear is the lesson.

## The old problem: `ret` trusts writable memory

Recall the mechanical definition: `ret` is essentially `pop rip`. It reads the
8 bytes at `[rsp]`, advances `rsp`, and jumps to the value it found. That is
perfectly reasonable when the stack is honest. It is disastrous when an overflow
or a ROP chain controls the stack.

```text
normal return:

    call victim              pushes return address "after"
after:
    ...

inside victim:
    rsp -> after             ret pops after into rip
```

ROP abuses exactly that trust:

```text
attacker-controlled stack:

    rsp -> gadget_1
           value_for_pop
           gadget_2
           gadget_3

each ret pops the next gadget address and keeps the chain running
```

Stack canaries may catch the overflow that delivered this stack layout. ASLR may
hide where the gadgets are. NX ensures the stack itself is not executable. But if
the attacker reaches a `ret` with a prepared stack, the old hardware rule is still
simple: pop the address and go.

Shadow stacks change that rule.

## Shadow stacks: a second copy for returns

With CET shadow stacks enabled, a `call` records the return address twice:

```text
ordinary stack:       shadow stack:

    rsp -> after          ssp -> after
```

The ordinary stack still exists because functions need their normal ABI layout:
arguments, spills, saved registers, local variables, and the return slot that old
code expects. The shadow stack is different. It is a hardware-tracked return stack
that ordinary stores cannot casually rewrite. User code does not get to repair it
with `mov [shadow], ...` the way it can overwrite `[rsp]`.

On `ret`, the CPU compares the two copies:

```text
    normal_return = [rsp]
    shadow_return = [ssp]

    if normal_return != shadow_return:
        raise a control-protection fault
    else:
        pop both and jump
```

So a stack overwrite that changes the normal return address is no longer enough.
The attacker has to make the protected shadow entry agree too. In the usual ROP
shape, they cannot, so the first mismatching `ret` faults before it reaches the
gadget.

The exercise simulates that check in ordinary assembly: `rbx` stands in for the
protected shadow copy, `[rsp]` is corrupted, and the code detects the mismatch
before executing `ret`.

## What shadow stacks do not promise

A shadow stack is not a general memory-safety system. It protects a specific
control-flow edge: returns. That makes it powerful, but narrow.

It does not stop every bug around a return address:

- A canary still matters because it can catch a linear stack overflow earlier,
  before the function reaches its epilogue.
- ASLR still matters because code addresses and valid call targets should not be
  predictable.
- NX still matters because injected bytes should not become executable code.
- Bounds checks and object lifetime checks still matter because data-only
  corruption may never touch a return edge.

It also does not say whether the source-level call graph was pretty. The hardware
cares about the dynamic call/return discipline, not whether a debugger backtrace
matches what a human expected.

That is why the tail-call lesson matters here.

## Tail calls are not a shadow-stack violation

A tail call looks strange in a backtrace because the intermediate function
vanishes:

```asm
wrapper:
    ...                     ; no work remains after worker
    add rsp, 0x20           ; tear wrapper's frame down, if it built one
    jmp worker              ; tail call: no return address is pushed
```

But this is not an attack on the shadow stack. The key fact is that the tail call
is a `jmp`, not a `call`. It does not create a new return address on the ordinary
stack, and therefore it should not create a new entry on the shadow stack either.

The sequence is balanced:

```text
caller calls wrapper:

    ordinary stack gets return-to-caller
    shadow stack gets return-to-caller

wrapper tail-jumps to worker:

    no new ordinary return address
    no new shadow return address

worker returns:

    ordinary top == shadow top == return-to-caller
```

So tail calls still flatten backtraces and still defeat naive caller validation,
but they do not inherently break CET. The hardware sees one call and one return.
That is exactly the shape it expects.

## IBT: the other edge

ROP is return-oriented: it drives control flow with `ret`. Attackers can also use
indirect branches: overwrite a function pointer, virtual-method table entry, jump
table slot, or callback so an indirect `call rax` or `jmp [mem]` lands somewhere
useful. That is a different edge from `ret`, so CET uses a different mechanism.

**Indirect branch tracking (IBT)** says that an indirect `call` or `jmp` may only
land on an approved landing pad. On x86-64, that landing pad is the instruction
`endbr64`, encoded as:

```text
f3 0f 1e fa
```

Compilers place `endbr64` at places that are legitimate indirect targets: for
example, function entries whose addresses may be taken. When IBT is enabled, an
indirect branch to some random byte in the middle of a function raises a
control-protection fault instead of beginning a gadget stream there.

That distinction matters:

```text
edge type                     CET mechanism
---------------------------   -----------------------------
ret                           shadow stack comparison
indirect call / indirect jmp   IBT landing pad (`endbr64`)
direct call / direct jmp       not the edge IBT is checking
```

When IBT is not enabled on a CET-capable system, `endbr64` behaves like a harmless
landing marker. With IBT enabled, it becomes a gate: indirect branches are allowed
to enter there, not at arbitrary nearby bytes.

## How to read a hardened binary

In a disassembly, CET leaves recognizable texture.

Function entries in an IBT-enabled binary often start like this:

```asm
some_function:
    endbr64
    push rbp
    mov rbp, rsp
    ...
```

That does not mean every call to the function is indirect. It means this address
is a legal landing pad if an indirect branch does target it. Direct calls do not
need the landing-pad check; they name their destination in the instruction stream.

Shadow stacks are less visible in normal function bodies. You do not usually see
extra instructions around every `call` and `ret`, because the CPU is doing the
tracking. You infer it from binary properties, loader notes, platform policy, or
from the fact that a corrupted return now dies with a control-protection fault
where older hardware would have jumped.

## The mitigation stack, updated

Each mitigation attacks a different step. None makes the others obsolete:

| mitigation | raises the cost of... |
|---|---|
| NX / DEP | executing injected data as code |
| ASLR + PIE | knowing where code and gadgets live |
| stack canary | reaching the saved return address through a linear overflow |
| shadow stack | making `ret` consume a forged return address |
| IBT / CFG / CFI | sending indirect calls or jumps to arbitrary code bytes |

The useful mental model is not "CET makes exploitation impossible." It is more
specific: CET makes two common control-flow edges stop trusting attacker-writable
state by itself. `ret` must agree with the shadow stack. Indirect branches must
land on approved targets. Bugs remain; the path from bug to code execution gets
narrower.

## Key points

- A **shadow stack** is a protected second return-address stack. `call` records
  the expected return; `ret` compares the ordinary stack slot with the shadow
  entry and faults on mismatch.
- Shadow stacks directly break classic **ROP** because a stack full of planted
  gadget addresses no longer matches the protected call/return history.
- An ordinary **tail call** is not a shadow-stack violation: it is a `jmp`, not a
  `call`, so it adds no extra return entry and the eventual `ret` matches the
  original caller's shadow entry.
- **IBT** protects indirect `call` and `jmp` edges by requiring legal landing
  pads such as `endbr64`; it is separate from return protection.
- CET is defense in depth, not memory safety. Canaries, ASLR, NX, validation, and
  ordinary invariant checks still matter.
