# The Curriculum

Each lesson is a self-contained directory. Everything a lesson needs — prose,
runnable examples, exercises, reference answers, images — lives inside it, so a
lesson can be read on GitHub, in a terminal, or in the browser and says the same
thing in all three. Adding a lesson means adding a directory; reordering the
curriculum means editing one number.

**To write a lesson, read [`../SKILL.md`](../SKILL.md).** It is the step-by-step
guide: the directory layout, the `lesson.md` front matter, the four exercise
types, and the house style. This file is the short reference.

## Layout

```
lessons/
  NN-part-slug/
    part.toml                 number + title for the part
    NN-lesson-slug/
      lesson.md               +++ TOML front matter +++ then Markdown  (required)
      README.md               what this lesson is, for repo browsers    (required)
      examples/*.asm          assembled by the test suite
      solutions/              reference material, never served
      assets/                 images referenced by lesson.md
      tests/                  extra fixtures
```

Numeric prefixes make `ls` agree with the reading order; the actual order comes
from the `order` (lesson) and `number` (part) fields.

## The lessons are tested

`cargo test -p lesson` loads this whole tree, assembles every `examples/*.asm`,
and grades every exercise's reference `solution` with the same code that grades
a student. A lesson whose example does not assemble, or whose stated answer does
not pass, fails the build. This is what keeps the prose from drifting out of
sync with the assembler and emulator as they change.

The server also validates the curriculum at startup and refuses to serve a
broken one.

## Current curriculum

| Part | Lesson | Teaches |
|------|--------|---------|
| I. Computer Fundamentals | Binary and Hexadecimal | why hex (not octal) is universal; a byte's meaning comes from its reader |
| | Signed Integers | two's complement as the encoding the adder already implements |
| | Endianness | why the low byte lives at the low address, and where big-endian survives |
| | Bitwise Operations and Shifts | masks, bit tests, shifts, sign extension and zero extension |
| II. CPU Architecture | Registers | the zero-extension rule, the `ah`/`spl` collision, the flags |
| III. Assembly Language | Your First Instructions | `cmp` is `sub` with the result discarded; building a loop |
| | Addressing Modes | `[base+index*scale+disp]`; why `rsp` can't be an index |
| IV. Stack and Heap | The Stack and Call Frames | `call`/`ret` = push/pop rip; the overflow that follows |
| | Calling Conventions | the System V ABI, the red zone, alignment; why ABIs can't mix |
| | Heap Allocation | chunks, allocator metadata, reuse, and heap lifetime bugs |
| V. Memory | Virtual Memory and Pages | the MMU, page permissions, and the page fault as one mechanism |
| | The Process Address Space and ASLR | the process layout and what ASLR randomizes |
| VI. Processes | Processes and File Descriptors | process isolation, descriptor tables, fork-style copying and exec replacement |
| | Threads and Synchronization | shared address spaces, races, locks and atomics |
| VII. Executable Formats | ELF: From Disk to Memory | sections vs segments, RVA vs offset, what the loader does |
| | PE: From Disk to Memory | MZ, PE headers, RVAs, data directories, sections and loader work |
| | TLS Callbacks: Code Before main | PE TLS, callbacks before entry point, and the entry-point trap |
| | Entry Point to main | loader entry, CRT startup, constructors, user main, and exit |
| VIII. Dynamic Linking | The GOT and the PLT | how a call to `puts` finds `puts`; lazy binding; RELRO |
| | The Import Address Table | PE imports, ILT/IAT, API Sets, delay loading, and import signals |
| | Rebasing, Relocations, and Windows ASLR | ImageBase, base relocation blocks, and Windows ASLR behavior |
| | PE Exports and Forwarders | export tables, ordinals, forwarded exports, and GetProcAddress lookup |
| IX. Reverse Engineering | Reading Compiled Code | recognising prologues, loops, `if`s, structs, signedness |
| | Lab: Patch the Check | defeat a keycheck by flipping one branch |
| | AOB Scanning | signatures and wildcards; opcode vs operand bytes; branch reach; alignment |
| | Your Code vs the Compiler's | separating your logic from ABI, security and runtime scaffolding |
| X. Debugging | How Debuggers Work | `int3`, the trap flag, hardware breakpoints and watchpoints |
| | Stepping, Watchpoints and Inspection | step over vs into, backtraces, conditional breakpoints |
| XI. Memory Structures | Arrays, Strings and Structs | base-plus-index arrays, string representations, field offsets and padding |
| | Containers in Memory | vectors, strings, lists, trees and hash tables as concrete layouts |
| XII. Compiler Behavior | Optimization Patterns | inlining, register allocation, frame omission, strength reduction and tail calls |
| XIII. OS Interaction | Syscalls, Exceptions and Signals | user/kernel transitions, page faults, traps and Unix signal delivery |
| XIV. Security | Exploit Mitigations | NX, canaries, ASLR, RELRO, CFG, CET as a cost-raising chain |
| | Checking a Binary's Defenses | reading the mitigations off a binary with the Inspector |
| | Stack Canaries and Security Cookies | compiler stack guards, Linux canaries, Windows `/GS`, and failure triage |
| | Return-Oriented Programming (ROP) | gadgets, clobbering, unaligned gadgets, and which mitigation breaks which step |
| | Caller Validation and Trusted Control Flow | reading the return RIP, whitelists, and why it is a signal not a boundary |
| | Tail Calls and the Vanishing Frame | how TCO turns call into jmp, erases a frame, and defeats caller validation |
| | CET, Shadow Stacks, and Indirect Branch Tracking | hardware ret validation, why ROP breaks, tail-call compatibility, IBT/`endbr64` |
| XV. Advanced Topics | Capstone: Inspect, Trace, Patch | tie inspection, tracing, ASLR, patching and mitigations into one workflow |

The proposed full curriculum — fifteen parts through advanced reverse
engineering — is in [`../docs/architecture.md`](../docs/architecture.md#curriculum).
It is a foundation to expand on, not an exhaustive list.
