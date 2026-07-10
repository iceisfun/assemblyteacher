+++
id = "compiler-scaffolding"
title = "Your Code vs the Compiler's"
order = 4
estimated_minutes = 40
objectives = [
  "Separate the instructions you wrote from the ABI, security and runtime scaffolding the toolchain added",
  "Recognise the security instrumentation on sight: the endbr64 landing pad, the fs:[0x28] stack canary, and CFG guard checks",
  "Recognise runtime plumbing: PLT/IAT thunks, CRT startup frames, and stack-probe calls",
  "Read a toolchain's fingerprint and know why the same source looks different from GCC, Clang and MSVC",
]
prerequisites = ["reading-compiled-code", "calling-conventions"]

[[exercises]]
id = "q-which-is-mine"
kind = "quiz"
prompt = "You disassemble a two-line function and see, in order: `endbr64`, `push rbp`, `mov rbp, rsp`, `mov rax, fs:[0x28]`, `add edi, esi`, … Which single instruction is your actual logic?"
choices = [
  "endbr64",
  "add edi, esi — the rest is scaffolding the toolchain inserted: a CET landing pad, the frame setup, and a stack-canary load",
  "mov rax, fs:[0x28]",
  "push rbp",
]
answer = 1
explanation = "Only `add edi, esi` computes anything the programmer asked for. `endbr64` is a CET landing pad, `push rbp`/`mov rbp, rsp` is the frame prologue, and `mov rax, fs:[0x28]` loads the stack-canary reference. Reading compiled code means recognising that scaffolding at a glance and mentally subtracting it, so the handful of instructions that are *your* code stand out."

[[exercises]]
id = "q-canary-tell"
kind = "quiz"
prompt = "In a Linux x86-64 function you see `mov rax, qword fs:[0x28]` in the prologue and, near the end, a compare against that saved value followed by a call to `__stack_chk_fail`. What is this?"
choices = [
  "Thread-local storage for a user variable",
  "A stack canary: the guard value is read from the thread block at fs:[0x28], stored in the frame, and checked before return — inserted by the compiler's stack protector, not written by the programmer",
  "A system call",
  "Position-independent addressing",
]
answer = 1
explanation = "`fs:[0x28]` is where glibc keeps the per-thread stack-guard value, and loading it into the prologue plus checking it before `ret` (branching to `__stack_chk_fail` on mismatch) is the signature of `-fstack-protector`. It is pure instrumentation: none of it is the function's logic, and recognising the `fs:[0x28]` tell lets you skip straight over it. MSVC does the same job with a `__security_cookie` and `__security_check_cookie`."

[[exercises]]
id = "q-plt-thunk"
kind = "quiz"
prompt = "Your source calls `strlen` once, but the disassembly shows a `call` to a tiny stub that immediately does `jmp qword [rip+…]`, and only *that* eventually reaches libc. Why the indirection?"
choices = [
  "The compiler duplicated strlen",
  "It is a PLT/IAT thunk: the call goes through the linker's indirection table (filled at load time) rather than directly to strlen, because the library's address is not known until runtime",
  "strlen was inlined",
  "It is a stack canary",
]
answer = 1
explanation = "A call into a shared library goes through a stub — the PLT on ELF, an IAT thunk on PE — that jumps through a pointer the loader fills in. It is runtime plumbing, not your logic: when tracing, step over the thunk and label it with the real function name. This is the GOT/PLT and IAT machinery from Part VIII, seen from the caller's disassembly."

[[exercises]]
id = "d-endbr64"
kind = "disassemble"
prompt = "This four-byte instruction sits at the top of nearly every function in a CET-enabled binary and is not your code. `f3 0f 1e fa`. What is it?"
hex = "f30f1efa"
expect_text = "endbr64"
hints = [
  "It is the CET Indirect Branch Tracking landing pad — the only byte pattern an indirect `call`/`jmp` is allowed to land on.",
  "As a reader you skip it; it does nothing to the program's data. Its job is to make jump-oriented gadgets fail.",
]
+++

# Your Code vs the Compiler's

Write `int add(int a, int b) { return a + b; }`, compile it, and disassemble the
result. You asked for one instruction — an add. You may get a dozen. The extra
ones are not a mistake and they are not your logic; they are **scaffolding** the
toolchain wraps around every function: the ABI's bookkeeping, the security
options' instrumentation, and the runtime's plumbing. Reading real disassembly is
mostly the skill of recognising that scaffolding on sight and subtracting it, so
the few instructions that are actually *your* program stand out.

The previous lesson taught the shapes of your logic — prologues, loops, `if`s,
struct accesses. This one teaches the shapes of everything *around* it.

## Three kinds of scaffolding

Everything the compiler adds falls into three buckets.

**ABI bookkeeping** keeps the calling convention's promises. The frame prologue
and epilogue (`push rbp ; mov rbp, rsp` … `leave ; ret`), a `sub rsp, N` that
rounds the stack to a 16-byte boundary or reserves Windows shadow space, and
`push rbx ; push r12 …` saving the callee-saved registers the function wants to
use. None of it computes anything; it is the cost of being callable.

**Security instrumentation** is inserted by mitigation flags, and it is the part
that most often makes simple code look alien:

```asm
    endbr64                       ; f3 0f 1e fa   CET landing pad (see below)
    push rbp
    mov  rbp, rsp
    mov  rax, qword fs:[0x28]      ; load the stack-canary reference value
    mov  qword [rbp-8], rax        ; stash it just below the saved frame pointer
    ...                            ; ── your code, at last ──
    mov  rax, qword [rbp-8]
    sub  rax, qword fs:[0x28]      ; canary unchanged?
    jne  .__stack_chk_fail         ; if not, abort — do not return
```

Three things there are all instrumentation. `endbr64` is a **CET landing pad**:
with Indirect Branch Tracking on, an indirect `call` or `jmp` is only allowed to
land on an `endbr64`, so it sits at the top of every indirectly-reachable
function — its whole purpose is to make the jump-oriented gadgets from the ROP
lesson fail. The `fs:[0x28]` load and the check before `ret` are the **stack
canary**. On Windows the same jobs appear as a `__security_cookie` and a
`__guard_check_icall` thunk guarding indirect calls for **CFG**. You did not
write any of it.

**Runtime plumbing** connects your function to the world. A call into a shared
library goes through a **PLT/IAT thunk** — a stub that jumps through a
loader-filled pointer — rather than straight to the target (the Part VIII
machinery, seen from the caller). Around `main` sit the **CRT startup frames**
(`__libc_start_main`, `mainCRTStartup`) from the entry-point lesson. And a
function with a large frame may open with a `__chkstk` / `___chkstk_ms`
**stack-probe** call that touches each guard page so the stack grows safely.

## The method: subtract, then read

Put the two lessons together into one habit. When you land in a function:

1. **Skip the entry scaffolding.** An `endbr64`, the `push rbp ; mov rbp, rsp`
   prologue, a stack-alignment `sub rsp`, callee-saved pushes, and a
   `fs:[0x28]` canary load — none of it is logic. Jump past it.
2. **Note the exit scaffolding.** The canary check, the `pop` restores, `leave`,
   `ret`. Ignore it too.
3. **Read what is left** with the previous lesson's field guide — that residue
   is your loops, `if`s and struct accesses.
4. **Label the plumbing.** Replace each PLT/IAT thunk with the library function
   it reaches, and step over CRT frames rather than into them.

A function that looked like twenty-five baffling instructions collapses to "load
two fields, compare, branch" once the scaffolding is subtracted. The complexity
was never in what you wrote; it was in what the toolchain guaranteed around it.

## Reading the toolchain's fingerprint

The same source produces recognisably different scaffolding per compiler, and
spotting which one you are looking at sets your expectations:

- **GCC** and **Clang** on Linux: `fs:[0x28]` canaries, `endbr64` pads with CET,
  `xor eax, eax` to zero, and heavy RIP-relative addressing. Clang tends toward
  denser branchless sequences (`cmov`, `setcc`); GCC is often more literal.
- **MSVC** on Windows: `__security_cookie` canaries, CFG `__guard_check_icall`
  thunks, `sub rsp, 0x28`+ shadow space at call sites, `__chkstk` for big frames,
  and the `rcx, rdx, r8, r9` argument order instead of `rdi, rsi, …`.

You are not memorising trivia; you are learning that a `sub rsp, 0x28` before a
call is *expected* Windows shadow space, not a mystery, and that `fs:[0x28]`
means "Linux, stack protector on" — context that tells you what is boilerplate
before you have read a single line of the real logic.

## Key points

- Most instructions in a function are **scaffolding**, not logic: ABI
  bookkeeping, security instrumentation, and runtime plumbing.
- Learn the instrumentation on sight: **`endbr64`** (CET landing pad),
  **`fs:[0x28]`** / `__security_cookie` (stack canary), CFG guard thunks.
- **PLT/IAT thunks**, **CRT startup frames** and **`__chkstk`** probes are
  runtime plumbing — label and step over them.
- To read a function, **subtract the scaffolding first**; the residue is the
  code you actually wrote, in the shapes of the previous lesson.
- Compiler **fingerprints** (GCC/Clang `fs:[0x28]` vs MSVC `__security_cookie`
  and shadow space) tell you what to expect before you start.
