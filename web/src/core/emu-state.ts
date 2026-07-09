// Reconstruct per-step machine state from an /api/emu/run response so an
// execution can be scrubbed forwards and backwards.
//
// All machine words are hex strings on the wire; we parse them to bigint here
// and never touch a JS number. The response gives the final register file, the
// final memory (regions), and per step the register/memory writes (each with a
// `before` and `after`) plus the flags before and after. We undo the register
// writes from `final` to recover the initial registers, and undo the memory
// writes from the final regions to recover the initial memory, then replay
// forward to snapshot every step.

import type { Flags, Region, RunResponse } from "../api.ts";
import { parseWord } from "./word.ts";
import { parseHex } from "./hex.ts";

export interface StepSnapshot {
  /** ip of the instruction executed to reach this snapshot (initial: first ip). */
  execIp: bigint;
  /** Current rip — the next instruction to execute. */
  ripNow: bigint;
  text: string;
  registers: Record<string, bigint>;
  writtenRegs: string[];
  flags: Flags;
}

export interface WindowRegion {
  start: number; // byte offset into the memory window
  end: number; // exclusive
  perms: string;
  name: string;
}

export interface ReconstructedRun {
  /** snapshots[0] is the pre-execution state; snapshots[k+1] is after step k. */
  snapshots: StepSnapshot[];
  memBase: bigint;
  memSize: number;
  /** Full memory image of the window at each snapshot index. */
  memoryAt(index: number): Uint8Array;
  /** Regions overlapping the window, as offsets, for colouring. */
  windowRegions: WindowRegion[];
  traceTruncated: boolean;
  stop: RunResponse["stop"];
  stdout: string;
  stderr: string;
}

const ZERO_FLAGS: Flags = {
  cf: false, pf: false, af: false, zf: false, sf: false, of: false, df: false,
};

const MAX_WINDOW = 8192;

interface ParsedRegion {
  base: bigint;
  bytes: Uint8Array;
  perms: string;
  name: string;
}

export function reconstruct(res: RunResponse): ReconstructedRun {
  const trace = res.trace;

  // ---- registers: recover the initial file by undoing every write ----
  const finalRegs: Record<string, bigint> = {};
  for (const [name, w] of Object.entries(res.final.registers)) {
    finalRegs[name] = parseWord(w);
  }
  const initialRegs: Record<string, bigint> = { ...finalRegs };
  for (let t = trace.length - 1; t >= 0; t--) {
    const w = trace[t]!.regWrites;
    for (let j = w.length - 1; j >= 0; j--) {
      initialRegs[w[j]!.reg] = parseWord(w[j]!.before);
    }
  }

  const finalRip = parseWord(res.final.rip);
  const snapshots: StepSnapshot[] = [];
  snapshots.push({
    execIp: trace[0] ? parseWord(trace[0].ip) : finalRip,
    ripNow: trace[0] ? parseWord(trace[0].ip) : finalRip,
    text: "(initial state)",
    registers: { ...initialRegs },
    writtenRegs: [],
    flags: trace[0]?.flagsBefore ?? ZERO_FLAGS,
  });

  let regs: Record<string, bigint> = { ...initialRegs };
  for (let t = 0; t < trace.length; t++) {
    const e = trace[t]!;
    regs = { ...regs };
    for (const w of e.regWrites) regs[w.reg] = parseWord(w.after);
    snapshots.push({
      execIp: parseWord(e.ip),
      ripNow: trace[t + 1] ? parseWord(trace[t + 1]!.ip) : finalRip,
      text: e.text,
      registers: regs,
      writtenRegs: e.regWrites.map((w) => w.reg),
      flags: e.flagsAfter,
    });
  }

  // ---- memory window ----
  const regions = parseRegions(res.regions);
  const rspFinal = finalRegs["rsp"];
  const rspInit = initialRegs["rsp"];

  let min: bigint | null = null;
  let max: bigint | null = null;
  const extend = (a: bigint, b: bigint) => {
    if (min === null || a < min) min = a;
    if (max === null || b > max) max = b;
  };
  // include the stack neighbourhood
  for (const rsp of [rspInit, rspFinal]) {
    if (rsp !== undefined) extend(rsp - 64n, rsp + 64n);
  }
  // include every written range
  for (const e of trace) {
    for (const m of e.memWrites) {
      const a = parseWord(m.addr);
      const len = BigInt(parseHex(m.after)?.length ?? 0);
      extend(a, a + len);
    }
  }
  // fall back to the first region if nothing else pinned the window
  if (min === null && regions.length > 0) {
    const r = regions[0]!;
    extend(r.base, r.base + BigInt(r.bytes.length));
  }

  let memBase = 0n;
  let memSize = 0;
  if (min !== null && max !== null) {
    memBase = (min as bigint) & ~0xfn;
    let size = Number((max as bigint) - memBase);
    if (size < 0) size = 0;
    if (size > MAX_WINDOW) size = MAX_WINDOW;
    memSize = size;
  }

  // seed the final-memory window from the regions (ground truth)
  const finalWindow = new Uint8Array(memSize);
  for (const r of regions) {
    copyOverlap(finalWindow, memBase, r.base, r.bytes);
  }
  // undo every memory write, latest first, to recover the initial window
  const initialWindow = finalWindow.slice();
  for (let t = trace.length - 1; t >= 0; t--) {
    const writes = trace[t]!.memWrites;
    for (let j = writes.length - 1; j >= 0; j--) {
      const m = writes[j]!;
      const before = parseHex(m.before);
      if (before) copyOverlap(initialWindow, memBase, parseWord(m.addr), before);
    }
  }

  const memoryAt = (index: number): Uint8Array => {
    const buf = initialWindow.slice();
    const upTo = Math.min(index, trace.length);
    for (let t = 0; t < upTo; t++) {
      for (const m of trace[t]!.memWrites) {
        const after = parseHex(m.after);
        if (after) copyOverlap(buf, memBase, parseWord(m.addr), after);
      }
    }
    return buf;
  };

  const windowRegions: WindowRegion[] = [];
  for (const r of regions) {
    const start = Number(r.base - memBase);
    const end = start + r.bytes.length;
    const clampedStart = Math.max(0, start);
    const clampedEnd = Math.min(memSize, end);
    if (clampedStart < clampedEnd) {
      windowRegions.push({
        start: clampedStart,
        end: clampedEnd,
        perms: r.perms,
        name: r.name,
      });
    }
  }

  return {
    snapshots,
    memBase,
    memSize,
    memoryAt,
    windowRegions,
    traceTruncated: res.traceTruncated,
    stop: res.stop,
    stdout: res.stdout,
    stderr: res.stderr,
  };
}

function parseRegions(regions: Region[]): ParsedRegion[] {
  const out: ParsedRegion[] = [];
  for (const r of regions) {
    const bytes = parseHex(r.hex);
    if (!bytes) continue;
    out.push({ base: parseWord(r.base), bytes, perms: r.perms, name: r.name });
  }
  return out;
}

/** Copy `src` (living at virtual address `srcBase`) into the window buffer. */
function copyOverlap(
  win: Uint8Array,
  winBase: bigint,
  srcBase: bigint,
  src: Uint8Array,
): void {
  const offset = Number(srcBase - winBase);
  for (let k = 0; k < src.length; k++) {
    const idx = offset + k;
    if (idx >= 0 && idx < win.length) win[idx] = src[k]!;
  }
}
