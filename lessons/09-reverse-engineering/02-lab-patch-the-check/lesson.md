+++
id = "lab-patch-the-check"
title = "Lab: Patch the Check"
order = 2
estimated_minutes = 35
objectives = [
  "Read a license/keycheck routine and identify the deciding branch",
  "Defeat the check by patching a single conditional jump",
  "Explain why patching the branch is easier than recovering the key",
  "Recognise the same pattern in real 'if the check fails, deny' code",
]
prerequisites = ["reading-compiled-code"]

[[exercises]]
id = "q-which-branch"
kind = "quiz"
prompt = "A keycheck does `cmp edi, ecx` (entered key vs the secret) then `jne denied`. To make the program grant access for *any* key, what is the simplest patch?"
choices = [
  "Change the secret in ecx",
  "Invert or remove the branch, so control falls through to the 'granted' path regardless of the comparison",
  "Encrypt the comparison",
  "Delete the cmp instruction",
]
answer = 1
explanation = "You do not need the key. The branch is the only thing standing between you and the granted path, so flip it (jne → je) or NOP it out and execution falls through to 'granted' no matter what was entered. This is why a comparison result must never be the *only* thing protecting a secret."

[[exercises]]
id = "q-why-patch"
kind = "quiz"
prompt = "Why is patching the branch usually far easier than recovering the correct key?"
choices = [
  "The key is encrypted and the branch is not",
  "The branch is a single, obvious decision point; recovering the key may require reversing a whole comparison/derivation algorithm",
  "Patching requires no tools",
  "They are equally hard",
]
answer = 1
explanation = "The check funnels every code path through one conditional jump — find it and you win. Recovering the key means understanding how the input is transformed and compared, which can be arbitrarily complex. Attackers go for the branch; defenders respond by making the *result* of the check drive real work (like decrypting data with the key), so a patched branch leaves you with garbage."

[[exercises]]
id = "e-patch-it"
kind = "emulate"
prompt = """
Here is a keycheck. With the wrong key in edi (0), it jumps to `denied` and
halts with rax = 0:

    mov edi, 0          ; the entered key (wrong)
    mov eax, 0          ; assume denied
    mov ecx, 0x1337     ; the secret
    cmp edi, ecx
    jne denied          ; <-- the deciding branch
    mov eax, 1          ; granted
  denied:
    hlt

Patch it: change exactly the branch so the program grants access (halts with
rax = 1) *even though edi is still the wrong key*. Do not change edi or the
secret.
"""
starter = """
    mov edi, 0
    mov eax, 0
    mov ecx, 0x1337
    cmp edi, ecx
    jne denied          ; patch this line
    mov eax, 1
denied:
    hlt
"""
solution = """
    mov edi, 0
    mov eax, 0
    mov ecx, 0x1337
    cmp edi, ecx
    je denied
    mov eax, 1
denied:
    hlt
"""
expect_registers = { rax = 1 }
hints = [
  "The wrong key makes `jne` taken (edi != ecx), which skips the 'granted' line. You want the opposite.",
  "Invert the branch: `jne` becomes `je`. Now the taken/not-taken sense flips, the jump is NOT taken, and control falls through to `mov eax, 1`.",
]
+++

# Lab: Patch the Check

Software that guards a feature behind a password, a serial number, or a license
check almost always reduces, somewhere deep down, to a single decision: a
comparison, and a branch that goes one way on success and the other on failure.
Find that branch and you own the decision — without ever knowing the secret.

This lab is that idea in its purest form. It is also exactly why "compare a
secret and branch" is a broken way to protect anything.

## The target

Read this keycheck the way the previous lesson taught:

```asm
    mov edi, <entered key>   ; whatever the user typed
    mov eax, 0               ; default: denied
    mov ecx, 0x1337          ; the secret key, sitting right there in the code
    cmp edi, ecx             ; entered == secret ?
    jne denied               ; if not equal, jump to denied
    mov eax, 1               ; granted  (rax = 1)
  denied:
    hlt
```

Two things jump out. First, **the secret is in the binary** — `0x1337`, loaded
straight into `ecx`. A keycheck that compares against a constant hands you the
constant; you could just read it. But you do not even need to.

Second, **one branch decides everything.** `jne denied` is the only thing
between the entered key and the `mov eax, 1` that grants access. Everything
funnels through it.

## The patch

You want the `granted` line to run regardless of what was entered. The branch
`jne denied` is taken when the key is *wrong* (not equal), skipping `granted`.
So invert it:

```asm
    jne denied      →      je denied
```

Now the jump is taken when the key is *right*, and with a wrong key it is *not*
taken — so control falls straight through to `mov eax, 1`. The check now grants
access for every key except the correct one, which is more than enough to get in.

Other patches work just as well: replace `jne denied` with two `nop`s (fall
through always), or with `jmp granted`. On a real binary this is a one- or
two-byte edit to the executable — flip `0x75` (jne) to `0x74` (je), or overwrite
it with `0x90 0x90`. That is what "cracking" a naive check *is*: a couple of
bytes.

Do the exercise below in the Playground. Change the one line, run it, and watch
`rax` come back as 1 with the wrong key still in `edi`. Then open the register
view and single-step across the branch to see it *not* taken.

## Why this works, and how real software fights back

Patching beats key-recovery because the branch is a single obvious choke point,
while recovering the key might mean reversing a whole derivation algorithm. The
attacker's economy is always: find the smallest change that flips the outcome.

So the defence is never "compare and branch." It is to make the check's
*result* do real, irreversible work:

- **Derive a decryption key from the input** and decrypt the actual program data
  or assets with it. Patch the branch and you fall through to code that runs on
  garbage — the feature is not unlocked, it is broken.
- **Check in many places**, so no single patch suffices.
- **Verify a signature** you cannot forge, rather than compare against a constant
  you shipped.

The through-line: a decision that can be flipped by editing one branch protects
nothing. The security has to live in *what the correct answer unlocks*, not in an
`if`. That principle scales all the way up to how real DRM, secure boot, and
license servers are (and are not) built.

## Key points

- A password/serial/license check usually funnels down to one comparison and one
  branch.
- Patching that branch (invert it, NOP it, or jump past it) defeats the check
  without knowing the secret — often a one- or two-byte edit.
- It is easier than recovering the key because the branch is a single choke
  point; that asymmetry is why attackers target it.
- Robust software makes the check's result *do work* (decrypt, verify a
  signature) so a flipped branch yields garbage, not access.
