# TLS Callbacks: Code Before main

Introduces thread-local storage and the PE TLS directory, then makes the point
that matters for reverse engineering: TLS callbacks run *before* the entry point
(and on every thread create/exit). That timing is why packers and anti-debugging
code hide in TLS callbacks — an entry-point breakpoint arrives too late — and the
lesson shows the reason-code check (`cmp edx, 1`, DLL_PROCESS_ATTACH) a reverser
learns to recognise. It closes by connecting the idea to ELF's `.init_array`, so
"code before main" reads as a cross-format concept rather than a Windows quirk.

Prerequisite: **PE: From Disk to Memory** (the TLS directory is data directory 9).
