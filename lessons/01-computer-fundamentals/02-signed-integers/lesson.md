+++
id = "signed-integers"
title = "Signed Integers and Two's Complement"
order = 2
estimated_minutes = 30
objectives = [
  "Explain two's complement as a consequence of wanting one adder, not two",
  "Negate a number by hand, and predict where negation fails",
  "Distinguish operations that care about signedness from operations that do not",
]
prerequisites = ["binary-and-hexadecimal"]

[[exercises]]
id = "q-ff-signed"
kind = "quiz"
prompt = "Read the byte `0xff` as a *signed* 8-bit integer. What is its value?"
choices = ["255", "-1", "-128", "-127"]
answer = 1
explanation = "All ones is -1: adding 1 wraps it to zero, and being the additive inverse of 1 is exactly what -1 means."

[[exercises]]
id = "q-shr-vs-sar"
kind = "quiz"
prompt = "You have a *signed* value in `rax` and want to divide it by 2. Which instruction is correct?"
choices = ["shr rax, 1", "sar rax, 1", "either one, they are the same", "shl rax, 1"]
answer = 1
explanation = "`sar` replicates the sign bit as it shifts, preserving negativity. `shr` shifts in zeroes, turning any negative number into a large positive one. This is why the CPU offers both."

[[exercises]]
id = "a-load-minus-one"
kind = "assemble"
prompt = "Load -1 into `al` using a single instruction."
starter = "mov al, "
solution = "mov al, -1"
expect_hex = "b0ff"
hints = ["-1 as an 8-bit two's complement value is `0xff`.", "`mov al, 0xff` and `mov al, -1` are the same instruction."]

[[exercises]]
id = "e-negate"
kind = "emulate"
prompt = "Put -5 into `rax`, then negate it in place so the program halts with `rax = 5`. End your program with `hlt`."
starter = """
mov rax, -5
; negate rax here
hlt
"""
solution = """
mov rax, -5
neg rax
hlt
"""
expect_registers = { rax = 5 }
hints = [
  "There is a single instruction that computes the two's-complement negation of its operand.",
  "It is called `neg`.",
]
+++

# Signed Integers and Two's Complement

A byte holds one of 256 patterns. If we want it to represent negative numbers,
we have to give up some of the positive ones and agree on which patterns mean
what. There are several ways to do that. Only one of them is any good, and the
reason it is good has nothing to do with elegance.

## The obvious idea, and why it fails

The obvious encoding is **sign-and-magnitude**: steal the top bit to mean
"negative", and read the rest as an ordinary magnitude.

```text
  0000 0101  =  +5
  1000 0101  =  -5
```

Readable. But now look at what the hardware has to do to add two numbers. It
must inspect both sign bits, compare magnitudes, decide whether this addition
is really a subtraction, subtract the smaller from the larger, and work out the
sign of the result. That is a lot of silicon to do something the machine does
billions of times a second.

There is also `1000 0000`: negative zero. A value that is equal to zero but is
not the same bit pattern as zero. Every comparison in the machine now has a
special case.

## The idea that works

Two's complement asks a different question. Not *"how do I write a minus
sign?"* but ***"what bit pattern, added to 5, gives zero?"***

Work in 8 bits, where arithmetic wraps at 256:

```text
    0000 0101      5
  + 1111 1011    251
  ───────────
   10000 0000    256, but the ninth bit falls off the end
    0000 0000      0
```

So `1111 1011` behaves exactly as −5 behaves, using the ordinary unsigned adder
and nothing else. We do not *define* it as −5 and then teach the adder about
signs; we notice that it already acts like −5, and adopt it.

That is the whole trick, and everything else follows:

- **One adder.** The circuit that computes `5 + 251` is the circuit that
  computes `5 + (−5)`. Addition, subtraction and comparison do not care whether
  you think the operands are signed. Read that again — it is why `add`, `sub`,
  `cmp`, `and`, `or` and `mov` have no signed and unsigned variants.
- **One zero.** `0000 0000` is the only pattern that is zero.
- **The top bit still tells you the sign**, not by fiat but as a consequence:
  every negative value has it set.

## Negating by hand

To negate: flip every bit, then add one.

```text
    5   =  0000 0101
  flip  =  1111 1010     (this is "one's complement", or NOT)
   +1   =  1111 1011     = -5
```

Flipping gives you a number that, added to the original, produces all ones —
which is −1. So `~x + x = −1`, and therefore `−x = ~x + 1`. The `neg`
instruction does exactly this, and it is exactly `sub 0, x`.

Check it by negating twice: `1111 1011` flips to `0000 0100`, plus one is
`0000 0101`, which is 5. Negation is its own inverse, as it should be.

## The asymmetry, and the bug it causes

Eight bits give 256 patterns. One is zero. That leaves 255 to split between the
positives and the negatives — an odd number. Something has to give.

```text
  0111 1111  =  +127     the largest positive
  1000 0000  =  -128     the most negative
  1111 1111  =    -1
```

The range is −128..=+127. **There is no +128, so `-(-128)` cannot be
represented.**

Negate `1000 0000`: flip to `0111 1111`, add one, and you get `1000 0000`
again. `neg` of the most negative number returns the most negative number, and
sets the overflow flag to tell you it happened.

This is not a curiosity. It is a real, exploited class of bug:

```c
int abs(int x) { return x < 0 ? -x : x; }
abs(INT_MIN);   /* still negative — and now your bounds check passes */
```

In Rust, `i32::MIN.abs()` panics in debug builds and wraps in release, and
`checked_abs()` exists precisely because of this square. In C it is undefined
behaviour. The hardware, at least, is honest: it raises the overflow flag and
carries on.

## Where signedness *does* live

If `add` and `sub` do not care about signedness, who does?

**The comparisons.** `cmp a, b` is just `sub` with the result thrown away: it
sets flags and nothing else. What you do *next* is where you declare your
interpretation:

| you mean            | after `cmp a, b` use | it tests   |
|---------------------|----------------------|------------|
| unsigned `a < b`    | `jb`  (below)        | `CF = 1`   |
| signed `a < b`      | `jl`  (less)         | `SF ≠ OF`  |
| unsigned `a > b`    | `ja`  (above)        | `CF=0, ZF=0` |
| signed `a > b`      | `jg`  (greater)      | `ZF=0, SF=OF` |

Compare `1` with `0xffffffffffffffff`. Unsigned, that second value is huge, so
`jb` (1 < huge) is taken. Signed, it is −1, so `jl` (1 < −1) is *not* taken.
Same `cmp`. Same flags. Different branch, different answer.

Reversing this — reading a disassembly, seeing `jl`, and concluding the variable
was signed — is a core reverse-engineering technique. The compiler tells you the
types it erased, if you know where to look.

**The widening moves.** Copying a small value into a bigger register needs a
decision: fill the new high bits with zeroes (`movzx`, for unsigned) or with
copies of the sign bit (`movsx`, for signed).

**The right shifts.** `shr` shifts in zeroes; `sar` replicates the sign bit.
For a negative number they give wildly different answers, which is exercise two.

**Multiply and divide.** `mul`/`div` are unsigned, `imul`/`idiv` are signed.
Here the hardware genuinely needs two circuits.

## Key points

- Two's complement is not a convention imposed on the adder; it is the encoding
  the adder already implements. Negative *x* is the pattern that adds to *x* to
  give zero.
- Negate by flipping the bits and adding one. `neg` is `0 - x`.
- The range is asymmetric: `-(most negative)` overflows back to itself.
- Addition, subtraction, and bitwise ops are sign-agnostic. Comparisons,
  widening moves, right shifts, and multiply/divide are not — and in a
  disassembly, those are where the original types show through.
