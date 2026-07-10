+++
id = "rebasing-and-windows-aslr"
title = "Rebasing, Relocations, and Windows ASLR"
order = 3
estimated_minutes = 35
objectives = [
  "Explain what a base relocation is and why an image that loads away from ImageBase needs a whole table of them",
  "Contrast the two answers to 'where do I load?': position-independent code that needs no fixups, and a relocation table that patches absolute addresses",
  "Describe what Windows ASLR randomizes and the key difference from Linux: a DLL's base is picked once per boot, shared across processes",
  "Read a .reloc entry and say which absolute addresses in an image the loader will patch",
]
prerequisites = ["address-space-layout", "pe-disk-to-memory"]

[[exercises]]
id = "q-why-reloc"
kind = "quiz"
prompt = "A DLL prefers ImageBase 0x180000000 but the loader must place it at 0x7FF9_1230_0000 because that address was taken. An instruction inside it holds the absolute address of a global that was computed for the preferred base. What has to happen?"
choices = [
  "Nothing — the CPU adjusts addresses automatically",
  "The loader adds the difference (actual base − preferred base) to every stored absolute address the image flagged in its base relocation table",
  "The DLL fails to load",
  "The global is copied to the preferred address",
]
answer = 1
explanation = "Every absolute address baked into the image was correct only at the preferred base. Moving the image by a delta of (actual − preferred) makes each one wrong by exactly that delta. The base relocation table lists where every such address sits; the loader walks it and adds the delta to each. This is 'rebasing', and it is the price of using absolute addresses instead of relative ones."

[[exercises]]
id = "q-pic-vs-reloc"
kind = "quiz"
prompt = "A Linux PIE and a classic Windows DLL both load at a random address, but they get there differently. What is the core difference?"
choices = [
  "The PIE is encrypted and the DLL is not",
  "PIC references code and data *relative to rip*, so nothing needs patching; the DLL uses absolute addresses and ships a relocation table the loader must apply",
  "The DLL cannot be randomized at all",
  "The PIE stores its addresses in the GOT and the DLL does not use a GOT",
]
answer = 1
explanation = "Two strategies for the same goal. Position-independent code avoids the problem: every reference is a distance from rip, correct at any base, so there is nothing to fix up. The relocation approach embeds absolute addresses and pays at load time — the loader walks the .reloc table and patches each one by the load delta. RIP-relative code trades a tiny per-reference cost for zero load-time work; relocations do the reverse. Modern PEs lean heavily on RIP-relative too, but the relocation table is still how the remaining absolutes are fixed."

[[exercises]]
id = "q-per-boot"
kind = "quiz"
prompt = "The sharpest difference between Windows and Linux ASLR: when is a given DLL's random base chosen?"
choices = [
  "Windows re-randomizes it on every process launch, like Linux does per exec",
  "Windows picks a DLL's base once per boot and reuses it for every process that loads that DLL; Linux randomizes per exec",
  "Windows never randomizes DLLs, only the main executable",
  "Both systems pick a fresh base on every function call",
]
answer = 1
explanation = "Windows randomizes a DLL's image base *once per boot*, then loads that DLL at the same address in every process for the rest of the boot. This lets the physical pages of a shared DLL be mapped into many processes at one address — a real memory saving. The cost is that the base is shared: an attacker who leaks kernel32's address in *any* process (or reads it from their own) knows it everywhere until the next reboot. Linux re-rolls the mmap base on each exec, so a leak is confined to that one process."

[[exercises]]
id = "q-reloc-block"
kind = "quiz"
prompt = "The .reloc section is a list of blocks, each starting with a page RVA and a size, followed by 2-byte entries. Why group fixups into per-page blocks with 12-bit offsets instead of storing a full 4-byte RVA per fixup?"
choices = [
  "To encrypt the relocation targets",
  "Compression: fixups cluster on the same page, so one 4-byte page RVA is amortised over many 2-byte entries (a 4-bit type + a 12-bit offset within that page)",
  "Because the CPU can only relocate one page at a time",
  "To keep the entries aligned to 8 bytes",
]
answer = 1
explanation = "Relocations cluster — a page of code has many absolute addresses, the next page has its own. Storing a full RVA per fixup would waste space, so the table factors out the shared high bits: each block names one 4KB page once, then lists 2-byte entries of (4-bit type, 12-bit offset-within-page). Twelve bits addresses the whole 4KB page, and the type says how wide the fixup is (usually DIR64: add the full 64-bit delta). It is a space optimisation over the obvious flat list."
+++

# Rebasing, Relocations, and Windows ASLR

The ASLR lesson in Part V explained *why* you would want to load an image at a
random address: if an attacker cannot predict where your code and data live, a
hardcoded exploit hits an unmapped page and crashes instead of winning. That
lesson was told on Linux, where a position-independent executable slides to a
random base for free. This lesson asks the mechanical question that raises: **if
the image can land anywhere, what happens to the absolute addresses baked inside
it?** Windows answers that with a structure the ELF world mostly designed away —
the base relocation table — and with an ASLR model that differs from Linux's in
one consequential way.

Load a DLL into the **Inspector** and it lists both the preferred `ImageBase`
and the relocations described here.

## Two ways to survive a random base

An image that loads at an unpredictable address has two options for any
address it needs to name.

The first you already know: **position-independent code**. Name everything as a
distance from `rip`. `lea rax, [rip+0x2fe0]` is correct whether the image loaded
at `0x400000` or `0x7FF9_0000_0000`, because the distance to a neighbour does not
change when the whole image slides. Nothing to patch. This is what a Linux PIE
does, and what modern PEs do for most of their references too.

The second is the older answer, and the one PE was built around: **use absolute
addresses, and ship a list of where they are so they can be fixed after the
move.** That list is the base relocation table.

## What a relocation actually fixes

Suppose the linker, assuming the preferred `ImageBase` of `0x180000000`, stored
the absolute address of a global as `0x180003000` inside some instruction. Now
the loader has to put the image at `0x7FF9_1230_0000` instead. Every absolute
address in the image is now wrong by the same amount:

```text
  delta = actual_base − preferred_base
        = 0x7FF9_1230_0000 − 0x1_8000_0000
  fixed_address = stored_address + delta
```

The loader cannot guess which of the image's bytes are addresses and which are
ordinary data — `0x180003000` and the integer `6442465280` are the same bits. So
the linker records, in the `.reloc` section, the location of *every* stored
absolute address. The loader walks that list and adds `delta` to each. That is
**rebasing**: not moving anything, but correcting the addresses the move
invalidated.

RIP-relative code needs none of this, which is the whole appeal — no table, no
load-time work, and nothing for the loader to get wrong. So why does PE still
carry relocations? Because some constructs genuinely need an absolute address —
certain global pointers, jump tables, and data structures the ABI defines with
absolute fields. Modern PEs are mostly RIP-relative with a *small* `.reloc`; the
table never went away, it shrank.

## How .reloc is laid out

The table is built for the fact that relocations **cluster**: a page of code has
several absolute addresses, the next page has its own. So rather than a flat list
of 4-byte RVAs, it is a list of **blocks**, one per 4 KB page:

```text
  block:  Page RVA (4 bytes) │ Block size (4 bytes) │ entries…
  entry:  ┌─4 bits─┬────12 bits────┐
          │  type  │ offset in page │
          └────────┴───────────────┘
```

Each block names one page once; each 2-byte entry then gives a 4-bit *type* and
a 12-bit offset into that page (12 bits is exactly a 4 KB page). On x64 the type
is almost always `IMAGE_REL_BASED_DIR64`: "add the full 64-bit delta to the
8-byte value here." Factoring the shared page RVA out of every entry is a plain
space optimisation, and reading one block tells you precisely which addresses on
that page the loader will touch.

## Windows ASLR, and the once-per-boot twist

Opting an image into ASLR is a link-time flag, `/DYNAMICBASE` (paired with
`/HIGHENTROPYVA` for the full 64-bit entropy). With it set, the loader picks a
random base and rebases via `.reloc` if the image has to move. So far this
mirrors Linux. The difference is *when the die is rolled*, and it matters:

- **Linux** randomizes the mmap base **per exec** — every launch of every
  process gets fresh library addresses. A leak in one process tells you nothing
  about the next.
- **Windows** picks a DLL's base **once per boot**, then loads that DLL at the
  **same address in every process** until the machine reboots.

Windows made that choice for a concrete payoff: if `kernel32.dll` sits at one
address system-wide, its read-only code pages exist in physical memory *once* and
map into every process at that shared address — a genuine memory saving across
hundreds of processes. The cost is a weaker secret. Leak the address of a common
DLL in *any* process — or simply read it in a process you already control — and
you know where it is in *every* process, no per-target leak required, until the
next boot re-rolls it. Same mitigation, different threat model, because of one
decision about when to randomize.

The main executable and heap still get per-launch treatment, and modern Windows
adds high-entropy (64-bit) randomization so the base is not brute-forceable — but
the shared-DLL-per-boot property is the thing to remember, because it changes how
an exploit reasons about what an information leak buys.

## Key points

- An absolute address baked into an image is only correct at the preferred
  `ImageBase`. Loading elsewhere shifts every one by `delta = actual −
  preferred`.
- **Position-independent code** avoids fixups entirely (everything is
  rip-relative); a **relocation table** is the alternative — ship the location of
  every absolute address and let the loader patch each by the delta. Modern PEs
  use both, leaning on rip-relative with a small `.reloc`.
- `.reloc` is organised into per-page blocks of 2-byte (type, 12-bit offset)
  entries — a space optimisation exploiting that fixups cluster by page.
- **Windows ASLR** randomizes a DLL's base **once per boot**, shared across all
  processes (saving memory but sharing the secret), where **Linux** randomizes
  **per exec**. That difference decides what a single address leak is worth.
