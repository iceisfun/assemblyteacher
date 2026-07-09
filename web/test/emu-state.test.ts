import { test } from "node:test";
import assert from "node:assert/strict";
import { reconstruct } from "../src/core/emu-state.ts";
import type { RunResponse } from "../src/api.ts";

const flags = { cf: false, pf: false, af: false, zf: false, sf: false, of: false, df: false };

// A stack region at 0x7fff00 (64 bytes of zeros) so the memory window has
// ground-truth bytes to seed from and undo writes against.
const stackHex = "00".repeat(64);

const run: RunResponse = {
  stop: { kind: "halted" },
  steps: 2,
  stdout: "",
  stderr: "",
  base: "0x1000",
  final: { registers: { rax: "0x3", rsp: "0x7fff20" }, rip: "0x8", flags },
  traceTruncated: false,
  regions: [
    { base: "0x7fff00", name: "stack", perms: "rw-", hex: stackHex, truncated: false },
  ],
  trace: [
    {
      ip: "0x0",
      text: "mov eax, 1",
      hex: "b801000000",
      regWrites: [{ reg: "rax", before: "0x0", after: "0x1" }],
      memWrites: [{ addr: "0x7fff10", before: "00", after: "01" }],
      memReads: [],
      flagsBefore: flags,
      flagsAfter: flags,
    },
    {
      ip: "0x5",
      text: "add eax, 2",
      hex: "83c002",
      regWrites: [{ reg: "rax", before: "0x1", after: "0x3" }],
      memWrites: [],
      memReads: [],
      flagsBefore: flags,
      flagsAfter: { ...flags, pf: true },
    },
  ],
};

test("reconstruct recovers the pre-execution registers as bigint", () => {
  const r = reconstruct(run);
  assert.equal(r.snapshots.length, 3); // initial + 2 steps
  assert.equal(r.snapshots[0]!.registers["rax"], 0n);
  assert.equal(r.snapshots[1]!.registers["rax"], 1n);
  assert.equal(r.snapshots[2]!.registers["rax"], 3n);
});

test("reconstruct tracks written registers and current rip", () => {
  const r = reconstruct(run);
  assert.deepEqual(r.snapshots[0]!.writtenRegs, []);
  assert.deepEqual(r.snapshots[1]!.writtenRegs, ["rax"]);
  // after step 0, rip is the next instruction's ip
  assert.equal(r.snapshots[1]!.ripNow, 0x5n);
  // after the last step, rip is final.rip
  assert.equal(r.snapshots[2]!.ripNow, 0x8n);
});

test("memoryAt replays writes cumulatively and undoes them for the initial", () => {
  const r = reconstruct(run);
  const off = Number(0x7fff10n - r.memBase);
  assert.ok(off >= 0 && off < r.memSize, "written addr falls in the window");
  // before any step the byte is its reconstructed initial value (0)
  assert.equal(r.memoryAt(0)[off], 0x00);
  // after step 0 (snapshot index 1) it holds the written value
  assert.equal(r.memoryAt(1)[off], 0x01);
});

test("windowRegions expose the mapped stack region as offsets", () => {
  const r = reconstruct(run);
  assert.ok(r.windowRegions.some((w) => w.name === "stack" && w.perms === "rw-"));
});
