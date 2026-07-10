+++
id = "stack-canaries-and-cookies"
title = "Stack Canaries and Security Cookies"
order = 3
estimated_minutes = 40
objectives = [
  "Explain why a compiler places a guard value between local buffers and saved control data",
  "Recognise the prologue and epilogue pattern of a stack canary or Windows /GS security cookie",
  "Distinguish Linux canary terminology from Windows security-cookie terminology",
  "Interpret a canary failure as evidence of stack corruption, not as a complete root-cause analysis",
]
prerequisites = ["exploit-mitigations", "checking-mitigations", "calling-conventions"]

[[exercises]]
id = "q-purpose"
kind = "quiz"
prompt = "What is the purpose of a stack canary or Windows /GS security cookie?"
choices = [
  "To encrypt the whole stack",
  "To place a hard-to-predict guard near vulnerable stack data and check it before returning",
  "To make every stack page executable",
  "To replace ASLR",
]
answer = 1
explanation = "The guard is a tripwire. A simple linear overflow that reaches saved control data should corrupt the guard first, so the epilogue detects the damage before `ret` uses the corrupted return address."

[[exercises]]
id = "q-linux-windows"
kind = "quiz"
prompt = "Which statement best compares Linux stack canaries and Windows /GS security cookies?"
choices = [
  "They are unrelated ideas",
  "They are the same mitigation pattern with different ABI/runtime details and different failure handlers",
  "Windows cookies only protect heap allocations",
  "Linux canaries are stored in the PE load-config directory",
]
answer = 1
explanation = "Both are compiler-inserted stack guards. The exact source of the guard, where metadata lives, and which helper handles failure differ by platform and toolchain."

[[exercises]]
id = "q-failure"
kind = "quiz"
prompt = "A program aborts in `__stack_chk_fail` or a Windows security-check failure path. What can you conclude immediately?"
choices = [
  "The program is definitely malicious",
  "A protected stack frame's guard did not match at function exit; investigate earlier writes for stack corruption",
  "ASLR is disabled",
  "The return address was successfully used by an attacker",
]
answer = 1
explanation = "The failure tells you a guard mismatch was detected. It is a strong corruption signal, but root cause still requires tracing where the overwrite happened."

[[exercises]]
id = "e-cookie-check"
kind = "emulate"
prompt = "Model a cookie check: rax holds the saved cookie and rbx holds the expected cookie. If they match, halt with rax = 1; if not, halt with rax = 0. Use matching values."
starter = """
    mov rax, 0x1234
    mov rbx, 0x1234
    ; compare saved and expected cookies
    hlt
"""
solution = """
    mov rax, 0x1234
    mov rbx, 0x1234
    cmp rax, rbx
    jne fail
    mov rax, 1
    hlt
fail:
    xor eax, eax
    hlt
"""
expect_registers = { rax = 1 }
hints = [
  "The real epilogue branches to a failure helper on mismatch.",
  "For the matching case, overwrite rax with 1 before halting.",
]
+++

# Stack Canaries and Security Cookies

The exploit-mitigations lesson introduced canaries as one layer in the chain.
This lesson zooms in on what the compiler actually adds and how an analyst reads
the result.

The Linux word is usually **stack canary**. The Windows compiler switch is
usually **/GS**, and the value is called a **security cookie**. The names differ;
the mitigation pattern is the same.

## The problem it detects

A classic stack overflow writes past a local buffer toward saved control data:
saved frame pointer, saved registers, and eventually the return address. If the
function returned normally after that overwrite, `ret` would use attacker-chosen
or corrupted bytes as a destination.

A canary places a guard value between the risky local area and the saved control
data. Before returning, the function checks whether the guard still matches the
expected process/thread value. If not, it aborts through a failure path instead
of returning through a damaged frame.

## What the compiler emits

The pattern has two halves:

```text
prologue:
  read the process/thread guard value
  store a copy in this stack frame

epilogue:
  reload the frame's copy
  compare it with the expected guard
  on mismatch, call the runtime failure handler
  on match, restore the frame and return
```

On Linux x86-64 you will often see a canary loaded from thread-local storage and
a failure path to `__stack_chk_fail`. On Windows you will see `/GS` security
cookie setup and a security-check failure path. Windows also records mitigation
and guard-related information in PE structures such as the load-config directory,
which analysis tools can report.

## What it does not prove

A canary failure is a detection, not a full explanation. It tells you a protected
frame's guard was changed before function exit. It does not identify the source
write by itself, and it does not mean the attacker successfully gained control.
In fact, the point is to stop before the corrupted return path is used.

It also does not protect every bug. Non-linear writes, object-level corruption,
heap corruption, data-only attacks, and information leaks live outside this
specific tripwire. Mitigations are layers, not absolutes.

## Analysis value

For reverse engineering and incident response, canary/cookie code answers two
questions:

- Was this function compiled with stack-protection instrumentation?
- Did the crash path indicate a guard mismatch rather than an ordinary fault?

Those are defensive observations. They help you triage a crash, understand a
compiler's output, and decide where to inspect earlier memory writes.

## Key points

- Stack canaries and Windows `/GS` security cookies are compiler-inserted guard
  checks for protected stack frames.
- The guard sits between vulnerable locals and saved control data, then gets
  checked before returning.
- A mismatch means stack corruption was detected before the function returned.
- The failure path is evidence to investigate, not a complete root cause by
  itself.
- Canaries are one mitigation layer; they do not replace ASLR, NX, CFG, CET, or
  memory-safe code.
