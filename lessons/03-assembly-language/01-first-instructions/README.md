# Your First Instructions

`mov`, `add`/`sub`, `cmp`/`test`, and the conditional jumps — enough to write a
loop. The central idea is that `cmp` is a subtraction whose result is thrown
away, so that one subtraction can answer every comparison, and the *branch* is
what declares whether the operands were signed.

- `examples/sum_loop.asm` — sum 1..10; the reference loop.
- `examples/cmp_is_sub.asm` — `cmp`, `sub` and `test` side by side.
- `examples/infinite.asm` — `eb fe`, and why a jump is relative.
