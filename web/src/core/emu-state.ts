// Reconstruct per-step machine state from an /api/emu/run response so an
// execution can be scrubbed forwards and backwards. The response gives the
// final register file plus, per step, the register/memory writes and the flags
// after the step. We undo all register writes from the final state to recover
// the initial state, then replay forward to snapshot every step.

import type { Flags, RunResponse, Registers } from "../api.ts";
import { parseHex } from "./hex.ts";

export interface StepSnapshot {
  /** ip of the instruction executed to reach this snapshot. */
  ip: bigint;
  text: string;
  registers: Registers;
  writtenRegs: string[];
  flags: Flags;
}

const ZERO_FLAGS: Flags = {
  zf: false, cf: false, sf: false, of: false, pf: false, af: false,
};

export interface ReconstructedRun {
  /** snapshots[0] is the pre-execution state; snapshots[k+1] is after step k. */
  snapshots: StepSnapshot[];
  /** Memory base chosen to cover the stack window and every write. */
  memBase: bigint;
  memSize: number;
  /** Full memory image at each snapshot index (parallel to snapshots). */
  memoryAt(index: number): Uint8Array;
  stop: RunResponse["stop"];
  stdout: string;
}

const MAX_MEM_WINDOW = 8192;

export function reconstruct(res: RunResponse, initialRsp = 0x800000n): ReconstructedRun {
  const trace = res.trace;

  // ---- registers: recover the initial file by undoing every write ----
  const initialRegs: Registers = { ...res.final.registers };
  for (let t = trace.length - 1; t >= 0; t--) {
    const w = trace[t]!.regWrites;
    for (let j = w.length - 1; j >= 0; j--) {
      initialRegs[w[j]!.reg] = w[j]!.before;
    }
  }
  if (initialRegs["rsp"] === undefined) initialRegs["rsp"] = Number(initialRsp);

  const snapshots: StepSnapshot[] = [];
  snapshots.push({
    ip: BigInt(trace[0]?.ip ?? res.final.rip),
    text: "(initial state)",
    registers: { ...initialRegs },
    writtenRegs: [],
    flags: ZERO_FLAGS,
  });

  let regs: Registers = { ...initialRegs };
  for (let t = 0; t < trace.length; t++) {
    const e = trace[t]!;
    regs = { ...regs };
    for (const w of e.regWrites) regs[w.reg] = w.after;
    snapshots.push({
      ip: BigInt(e.ip),
      text: e.text,
      registers: regs,
      writtenRegs: e.regWrites.map((w) => w.reg),
      flags: e.flagsAfter,
    });
  }

  // ---- memory window ----
  let minAddr = initialRsp - 64n;
  let maxAddr = initialRsp + 16n;
  const rspFinal = BigInt(Math.trunc(res.final.registers["rsp"] ?? Number(initialRsp)));
  if (rspFinal - 64n < minAddr) minAddr = rspFinal - 64n;
  for (const e of trace) {
    for (const m of e.memWrites) {
      const a = BigInt(m.addr);
      const len = BigInt((parseHex(m.bytes)?.length ?? 0));
      if (a < minAddr) minAddr = a;
      if (a + len > maxAddr) maxAddr = a + len;
    }
  }
  minAddr = minAddr & ~0xfn;
  let size = Number(maxAddr - minAddr);
  if (size < 0) size = 0;
  if (size > MAX_MEM_WINDOW) size = MAX_MEM_WINDOW;
  const memBase = minAddr;
  const memSize = size;

  const memoryAt = (index: number): Uint8Array => {
    const buf = new Uint8Array(memSize);
    // apply writes for steps 0..index-1 (snapshot index i is after step i-1)
    const upTo = Math.min(index, trace.length);
    for (let t = 0; t < upTo; t++) {
      for (const m of trace[t]!.memWrites) {
        const bytes = parseHex(m.bytes);
        if (!bytes) continue;
        const off = Number(BigInt(m.addr) - memBase);
        for (let k = 0; k < bytes.length; k++) {
          const idx = off + k;
          if (idx >= 0 && idx < memSize) buf[idx] = bytes[k]!;
        }
      }
    }
    return buf;
  };

  return {
    snapshots,
    memBase,
    memSize,
    memoryAt,
    stop: res.stop,
    stdout: res.stdout,
  };
}
