// Byte / string search over a buffer. Pure; unit-tested.

import { parseHex } from "./hex.ts";

/** Turn an ASCII string into its byte pattern. */
export function asciiNeedle(text: string): Uint8Array {
  const out = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i++) out[i] = text.charCodeAt(i) & 0xff;
  return out;
}

/**
 * Build a needle from a query. When `mode` is `hex`, the query is parsed as hex
 * bytes; when `ascii`, as raw characters. Returns null for an empty/invalid hex
 * query.
 */
export function buildNeedle(
  query: string,
  mode: "hex" | "ascii",
): Uint8Array | null {
  if (query.length === 0) return null;
  return mode === "hex" ? parseHex(query) : asciiNeedle(query);
}

/** Every start index at which `needle` occurs in `haystack`. */
export function findAll(haystack: Uint8Array, needle: Uint8Array): number[] {
  const hits: number[] = [];
  if (needle.length === 0 || needle.length > haystack.length) return hits;
  const first = needle[0]!;
  const last = haystack.length - needle.length;
  outer: for (let i = 0; i <= last; i++) {
    if (haystack[i] !== first) continue;
    for (let j = 1; j < needle.length; j++) {
      if (haystack[i + j] !== needle[j]) continue outer;
    }
    hits.push(i);
  }
  return hits;
}

/** The first match at or after `from`, wrapping to the start. -1 if none. */
export function findNext(
  haystack: Uint8Array,
  needle: Uint8Array,
  from: number,
): number {
  const all = findAll(haystack, needle);
  if (all.length === 0) return -1;
  for (const idx of all) if (idx >= from) return idx;
  return all[0]!;
}

/** The last match strictly before `from`, wrapping to the end. -1 if none. */
export function findPrev(
  haystack: Uint8Array,
  needle: Uint8Array,
  from: number,
): number {
  const all = findAll(haystack, needle);
  if (all.length === 0) return -1;
  for (let i = all.length - 1; i >= 0; i--) if (all[i]! < from) return all[i]!;
  return all[all.length - 1]!;
}
