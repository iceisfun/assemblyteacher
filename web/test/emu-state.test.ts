import { test } from "node:test";
import assert from "node:assert/strict";
import { reconstruct } from "../src/core/emu-state.ts";
import type { RunResponse } from "../src/api.ts";

const flags = { zf: false, cf: false, sf: false, of: false, pf: false, af: false };

const run: RunResponse = {
  stop: { kind: "halted" },
  steps: 2,
  stdout: "",
  final: { registers: { rax: 3, rsp: 0x800000 }, rip: 8, flags },
  trace: [
    {
      ip: 0,
      text: "mov eax, 1",
      regWrites: [{ reg: "rax", before: 0, after: 1 }],
      memWrites: [{ addr: 0x7ffff8, bytes: "01" }],
      flagsAfter: flags,
    },
    {
      ip: 3,
      text: "add eax, 2",
      regWrites: [{ reg: "rax", before: 1, after: 3 }],
      memWrites: [],
      flagsAfter: { ...flags, pf: true },
    },
  ],
};

test("reconstruct recovers the pre-execution state", () => {
  const r = reconstruct(run);
  // snapshots: [initial, after step0, after step1]
  assert.equal(r.snapshots.length, 3);
  assert.equal(r.snapshots[0]!.registers["rax"], 0);
  assert.equal(r.snapshots[1]!.registers["rax"], 1);
  assert.equal(r.snapshots[2]!.registers["rax"], 3);
});

test("reconstruct tracks written registers per step", () => {
  const r = reconstruct(run);
  assert.deepEqual(r.snapshots[0]!.writtenRegs, []);
  assert.deepEqual(r.snapshots[1]!.writtenRegs, ["rax"]);
});

test("memoryAt replays writes cumulatively", () => {
  const r = reconstruct(run);
  const off = Number(0x7ffff8n - r.memBase);
  assert.ok(off >= 0 && off < r.memSize);
  // before any step, the byte is zero
  assert.equal(r.memoryAt(0)[off], 0);
  // after step 0 (snapshot index 1) it is written
  assert.equal(r.memoryAt(1)[off], 0x01);
});
