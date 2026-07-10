# Stepping, Watchpoints and Inspection

The practical companion to "How Debuggers Work" — the primitives from that lesson
(the `0xCC` breakpoint, the Trap Flag, the debug registers) arranged into the
things you actually click on:

- **Step into vs step over** — step into is a single step that follows control
  into the callee; step over plants a temporary breakpoint on the return address
  and runs the call at full speed.
- **Inspecting and changing state** — reading and writing the stopped program's
  registers and memory via `ptrace`, and what a memory viewer / register panel is.
- **The backtrace** — walking the saved-rbp chain, and falling back to DWARF
  `.eh_frame` unwind tables when frame pointers are omitted (ties back to the
  stack lesson).
- **Conditional breakpoints** — break, test the condition, silently continue if
  false, and why that is slow on a hot line.

References the Playground memory viewer and register panel at an `int3` stop.

Four quiz exercises; no runnable examples (all assembly is in rendered prose).
