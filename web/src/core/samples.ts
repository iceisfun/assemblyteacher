// Built-in sample data so every page renders something even when the API is
// unreachable. The components must never blank out on a fetch failure.

import type { Region, Annotation } from "../components/memory-viewer.ts";

export const SAMPLE_SOURCE = `; Sum 1..5 and exit with the total.
        mov   ecx, 5
        xor   eax, eax
loop:
        add   eax, ecx
        dec   ecx
        jnz   loop
        mov   edi, eax
        mov   eax, 60      ; exit
        syscall
`;

/** A small, structured buffer that shows off regions, annotations and a pointer. */
export function sampleBuffer(): {
  bytes: Uint8Array;
  base: bigint;
  regions: Region[];
  annotations: Annotation[];
} {
  const base = 0x601000n;
  const bytes = new Uint8Array(256);
  // fill with a readable gradient
  for (let i = 0; i < bytes.length; i++) bytes[i] = i & 0xff;
  // a C string at offset 0x10
  const msg = "hello, assembly!\0";
  for (let i = 0; i < msg.length; i++) bytes[0x10 + i] = msg.charCodeAt(i);
  // a struct at 0x40: { u32 id; u32 flags; u64 next; }
  bytes.set([0x2a, 0x00, 0x00, 0x00], 0x40); // id = 42
  bytes.set([0x01, 0x00, 0x00, 0x00], 0x44); // flags = 1
  // next -> points at 0x601080 (offset 0x80), little-endian u64
  const ptr = 0x601080n;
  for (let i = 0; i < 8; i++) {
    bytes[0x48 + i] = Number((ptr >> BigInt(i * 8)) & 0xffn);
  }
  const regions: Region[] = [
    { start: 0x10, end: 0x21, color: "rgba(94, 212, 166, 0.30)", label: "string" },
    { start: 0x40, end: 0x50, color: "rgba(124, 196, 255, 0.30)", label: "struct node" },
    { start: 0x80, end: 0x90, color: "rgba(255, 143, 143, 0.30)", label: "next node" },
  ];
  const annotations: Annotation[] = [
    { addr: base + 0x40n, size: 4, label: "id", type: "u32" },
    { addr: base + 0x44n, size: 4, label: "flags", type: "u32" },
    { addr: base + 0x48n, size: 8, label: "next", type: "ptr" },
  ];
  return { bytes, base, regions, annotations };
}
