// Byte-buffer diffing for modified-byte highlighting. Pure; unit-tested.

/**
 * Indices whose value differs between `prev` and `next`. Indices that exist in
 * only one buffer (because the length changed) count as changed. Comparing over
 * `max(len)` keeps growth/shrink visible in the viewer.
 */
export function diffBytes(prev: Uint8Array, next: Uint8Array): Set<number> {
  const changed = new Set<number>();
  const n = Math.max(prev.length, next.length);
  for (let i = 0; i < n; i++) {
    if (prev[i] !== next[i]) changed.add(i);
  }
  return changed;
}

/** Whether any byte changed. Cheap early-out for the common no-op update. */
export function anyChanged(prev: Uint8Array, next: Uint8Array): boolean {
  if (prev.length !== next.length) return true;
  for (let i = 0; i < prev.length; i++) if (prev[i] !== next[i]) return true;
  return false;
}
