+++
id = "pe-exports-and-forwarders"
title = "PE Exports and Forwarders"
order = 4
estimated_minutes = 40
objectives = [
  "Explain how a PE DLL publishes functions through its export directory",
  "Distinguish export names, ordinals, RVAs and the ordinal base",
  "Describe forwarded exports and why an exported name may resolve into another DLL",
  "Connect GetProcAddress to the same export-table lookup the loader uses for imports",
]
prerequisites = ["iat-and-imports", "rebasing-and-windows-aslr"]

[[exercises]]
id = "q-export-directory"
kind = "quiz"
prompt = "What does the PE export directory primarily describe?"
choices = [
  "The functions and data symbols this image makes available to other images, indexed by name and/or ordinal",
  "The DLLs this image imports from",
  "The stack size for each thread",
  "The executable's command-line arguments",
]
answer = 0
explanation = "Imports say what this image needs. Exports say what this image offers. The loader and `GetProcAddress` use the export directory to turn a requested name or ordinal into an address."

[[exercises]]
id = "q-ordinal-base"
kind = "quiz"
prompt = "A DLL's export directory has ordinal base 1. Name `ReadThing` points at ordinal index 4 in the export address table. What public ordinal is usually displayed?"
choices = ["4", "5", "1", "0"]
answer = 1
explanation = "The export address table is indexed from zero, but public ordinals are biased by the directory's ordinal base. Public ordinal = base + index = 1 + 4 = 5."

[[exercises]]
id = "q-forwarder"
kind = "quiz"
prompt = "An export entry for `KERNEL32.Sleep` contains the string `KERNELBASE.Sleep` instead of an RVA pointing into code. What is that?"
choices = [
  "A corrupt export",
  "A forwarded export: resolving it continues in another DLL under the named export",
  "A TLS callback",
  "A bound import timestamp",
]
answer = 1
explanation = "A forwarded export is an alias. The exporting DLL says, in effect, 'this function is implemented over there.' Modern Windows uses this heavily to preserve old DLL names while implementation moves."

[[exercises]]
id = "e-export-va"
kind = "emulate"
prompt = "A DLL loaded at base 0x180000000 exports a function at RVA 0x1234. Compute the runtime virtual address in rax and halt with rax = 0x180001234."
starter = """
    mov rax, 0x180000000
    ; add the export RVA
    hlt
"""
solution = """
    mov rax, 0x180000000
    add rax, 0x1234
    hlt
"""
expect_registers = { rax = 6442455604 }
hints = ["PE exports store RVAs. Runtime VA = actual image base + RVA."]
+++

# PE Exports and Forwarders

The IAT lesson looked from the caller's side: an executable imports
`MessageBoxA`, and the loader fills an IAT slot with the address. This lesson
looks from the DLL's side. How did the loader know where `MessageBoxA` lived?

It read the DLL's **export directory**.

## Imports ask; exports answer

A PE import descriptor says: "from `USER32.dll`, I need `MessageBoxA`." The
loader maps `USER32.dll`, reads its export directory, and looks for that name.
If it finds it, the export entry gives an RVA. The loader adds the DLL's actual
base address and writes the resulting VA into the caller's IAT.

`GetProcAddress` is the programmable version of the same lookup. Given a module
handle and a name or ordinal, it searches the export directory and returns the
resolved address.

## The export tables

The export directory is a small index over three related tables:

```text
export name pointer table  -> strings such as "CreateFileW"
export ordinal table       -> indexes into the address table
export address table       -> RVAs, or forwarder strings
```

Names are for humans and source compatibility. Ordinals are numbers. Some DLLs
export by both; some exports are ordinal-only. Ordinal-only exports are harder to
read during analysis because the useful name is absent from the binary interface.

The directory also carries an **ordinal base**. The export address table is
zero-indexed, but public ordinals are usually displayed as `base + index`.

## Forwarded exports

Sometimes an export address entry does not point at code. It points at a string
inside the export directory, such as:

```text
KERNELBASE.Sleep
NTDLL.RtlAllocateHeap
```

That is a **forwarded export**. The DLL is publishing a stable name, but the
implementation lives in another DLL. Resolution continues there.

Forwarding is ordinary on Windows. It lets Microsoft preserve old import names
while moving implementation into a different component. It also explains why a
tool may show an import from `kernel32.dll` but a debugger lands in
`kernelbase.dll` or `ntdll.dll`.

## Analysis value

Exports are a triage surface. A DLL that exports one normal plugin entry point
looks different from a DLL that exports dozens of oddly named ordinals. Neither
fact proves intent. It tells you what questions to ask next: who imports this,
which exports are actually called, and whether a forwarder moved the final target
somewhere else.

## Key points

- Imports name what a PE needs; exports name what a PE offers.
- Export lookup turns a name or ordinal into an RVA, then into a runtime VA by
  adding the DLL's actual load base.
- Public ordinals are biased by the export directory's ordinal base.
- Forwarded exports are normal aliases to exports in another DLL.
- `GetProcAddress` is export-table lookup exposed as an API.
