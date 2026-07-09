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
| II. CPU Architecture | Registers | the zero-extension rule, the `ah`/`spl` collision, the flags |
| III. Assembly Language | Your First Instructions | `cmp` is `sub` with the result discarded; building a loop |
| | Addressing Modes | `[base+index*scale+disp]`; why `rsp` can't be an index |
| IV. Stack and Heap | The Stack and Call Frames | `call`/`ret` = push/pop rip; the overflow that follows |

The proposed full curriculum — fifteen parts through advanced reverse
engineering — is in [`../docs/architecture.md`](../docs/architecture.md#curriculum).
It is a foundation to expand on, not an exhaustive list.
