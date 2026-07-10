// Federated search across the three catalogs the app knows about: registers and
// instructions (bundled in the frontend, searched instantly in memory) and
// lessons (searched server-side over the curriculum). The command palette calls
// `searchEntities` for the instant entity hits and `searchLessonHits` for the
// async lesson hits, and renders them as grouped, entity-first results.
//
// Pure except for `searchLessonHits`, which is a single fetch.

import { lookupReg, searchRegs } from "./reginfo.ts";
import { lookupInsnEntry, searchInsns } from "./insninfo.ts";
import { search as searchLessons } from "../api.ts";

export type HitKind = "register" | "instruction" | "lesson";

export interface Hit {
  kind: HitKind;
  /** The primary label, e.g. "EAX", "MOV", or a lesson title. */
  label: string;
  /** A one-line secondary description. */
  sub: string;
  /** In-app hash link to open the result. */
  href: string;
}

// Run a name-search over the whole query and, for a multi-word query, over each
// word too, preserving order and dropping duplicates. This is what lets
// "eax foo bar" still surface EAX: the whole phrase matches nothing, but the
// "eax" token does. Single-word queries are unchanged.
function unionByWord(query: string, fn: (q: string) => string[]): string[] {
  const queries = [query, ...query.trim().split(/\s+/)];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const q of queries) {
    if (!q) continue;
    for (const name of fn(q)) {
      if (!seen.has(name)) {
        seen.add(name);
        out.push(name);
      }
    }
  }
  return out;
}

/** Registers and instructions, resolved synchronously from the bundled catalogs. */
export function searchEntities(query: string): { registers: Hit[]; instructions: Hit[] } {
  const registers: Hit[] = unionByWord(query, searchRegs).map((name) => {
    const info = lookupReg(name)!;
    return {
      kind: "register",
      label: name.toUpperCase(),
      sub: `${info.width}-bit · ${info.role.split(";")[0]!.trim()}`,
      href: `#/registers/${name}`,
    };
  });

  const instructions: Hit[] = unionByWord(query, searchInsns).map((mnemonic) => {
    const entry = lookupInsnEntry(mnemonic);
    return {
      kind: "instruction",
      label: mnemonic.toUpperCase(),
      sub: entry?.summary ?? "",
      href: `#/instructions/${mnemonic}`,
    };
  });

  return { registers, instructions };
}

/** Lesson hits from the server. Empty on any error — search must never throw. */
export async function searchLessonHits(query: string): Promise<Hit[]> {
  try {
    const hits = await searchLessons(query);
    return hits.map((h) => ({
      kind: "lesson",
      label: h.title,
      sub: h.snippet,
      href: `#/lessons/${encodeURIComponent(h.id)}`,
    }));
  } catch {
    return [];
  }
}
