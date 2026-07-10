// A global command palette: press ⌘K / Ctrl-K (or "/") anywhere to search
// registers, instructions and lessons at once. Entity hits (registers,
// instructions) resolve instantly from the bundled catalogs; lesson hits arrive
// from the server a moment later. Results are grouped, entity-first, and
// keyboard-navigable — ↑/↓ to move, ↵ to open, Esc to close.

import { searchEntities, searchLessonHits, type Hit } from "../core/omni-search.ts";

const GROUPS: Array<{ kind: Hit["kind"]; title: string; cap: number }> = [
  { kind: "register", title: "Registers", cap: 6 },
  { kind: "instruction", title: "Instructions", cap: 8 },
  { kind: "lesson", title: "Lessons", cap: 10 },
];

let overlay: HTMLElement | null = null;
let input: HTMLInputElement | null = null;
let list: HTMLElement | null = null;
let items: HTMLElement[] = [];
let sel = -1;
let queryToken = 0;
let debounce: number | undefined;
let lastFocus: Element | null = null;

function build(): void {
  if (overlay) return;
  overlay = document.createElement("div");
  overlay.className = "cmdk-overlay";
  overlay.hidden = true;
  overlay.innerHTML = `
    <div class="cmdk" role="dialog" aria-modal="true" aria-label="Search">
      <input class="cmdk-input" type="text" spellcheck="false" autocomplete="off"
             role="combobox" aria-expanded="true" aria-controls="cmdk-list"
             aria-label="Search registers, instructions and lessons"
             placeholder="Search registers, instructions, lessons…" />
      <div class="cmdk-results" id="cmdk-list" role="listbox"></div>
      <div class="cmdk-foot"><kbd>↑</kbd><kbd>↓</kbd> navigate&nbsp; <kbd>↵</kbd> open&nbsp; <kbd>esc</kbd> close</div>
    </div>`;
  input = overlay.querySelector<HTMLInputElement>(".cmdk-input")!;
  list = overlay.querySelector<HTMLElement>(".cmdk-results")!;

  // Backdrop click closes; clicks inside the dialog do not.
  overlay.addEventListener("mousedown", (e) => {
    if (e.target === overlay) close();
  });
  input.addEventListener("input", () => onQuery(input!.value));
  input.addEventListener("keydown", onKey);
  document.body.appendChild(overlay);
}

function onQuery(q: string): void {
  const query = q.trim();
  // Entities are in-memory: paint them at once. Lessons are fetched (debounced).
  const { registers, instructions } = query ? searchEntities(query) : { registers: [], instructions: [] };
  paint(registers, instructions, [], query.length > 0);

  window.clearTimeout(debounce);
  const token = ++queryToken;
  if (!query) return;
  debounce = window.setTimeout(() => {
    void searchLessonHits(query).then((lessons) => {
      if (token !== queryToken) return; // a newer query already ran
      const ents = searchEntities(query);
      paint(ents.registers, ents.instructions, lessons, false);
    });
  }, 140);
}

function paint(registers: Hit[], instructions: Hit[], lessons: Hit[], lessonsPending: boolean): void {
  if (!list) return;
  const byKind: Record<Hit["kind"], Hit[]> = { register: registers, instruction: instructions, lesson: lessons };
  list.innerHTML = "";
  items = [];

  for (const g of GROUPS) {
    const hits = byKind[g.kind].slice(0, g.cap);
    if (!hits.length) {
      if (g.kind === "lesson" && lessonsPending) {
        const head = groupHeader(g.title);
        const pending = document.createElement("div");
        pending.className = "cmdk-pending";
        pending.textContent = "searching…";
        list.append(head, pending);
      }
      continue;
    }
    list.appendChild(groupHeader(g.title));
    for (const hit of hits) list.appendChild(itemEl(hit));
  }

  if (!items.length && input && input.value.trim()) {
    const empty = document.createElement("div");
    empty.className = "cmdk-empty";
    empty.textContent = lessonsPending ? "searching…" : "No matches";
    list.appendChild(empty);
  }

  // Keep a valid selection: default to the first item (the strongest hit).
  sel = items.length ? Math.min(sel < 0 ? 0 : sel, items.length - 1) : -1;
  highlight();
}

function groupHeader(title: string): HTMLElement {
  const h = document.createElement("div");
  h.className = "cmdk-group";
  h.textContent = title;
  return h;
}

const KIND_TAG: Record<Hit["kind"], string> = { register: "REG", instruction: "INSN", lesson: "LESSON" };

function itemEl(hit: Hit): HTMLElement {
  const el = document.createElement("div");
  el.className = "cmdk-item";
  el.setAttribute("role", "option");
  el.dataset.href = hit.href;
  const idx = items.length;
  el.id = `cmdk-opt-${idx}`;
  el.innerHTML =
    `<span class="cmdk-kind cmdk-kind-${hit.kind}">${KIND_TAG[hit.kind]}</span>` +
    `<span class="cmdk-label"></span><span class="cmdk-sub"></span>`;
  el.querySelector(".cmdk-label")!.textContent = hit.label;
  el.querySelector(".cmdk-sub")!.textContent = hit.sub;
  el.addEventListener("mousemove", () => {
    if (sel !== idx) {
      sel = idx;
      highlight();
    }
  });
  el.addEventListener("click", () => go(hit.href));
  items.push(el);
  return el;
}

function highlight(): void {
  items.forEach((el, i) => el.classList.toggle("sel", i === sel));
  if (sel >= 0 && items[sel]) {
    items[sel]!.scrollIntoView({ block: "nearest" });
    input?.setAttribute("aria-activedescendant", items[sel]!.id);
  } else {
    input?.removeAttribute("aria-activedescendant");
  }
}

function onKey(e: KeyboardEvent): void {
  if (e.key === "Escape") {
    e.preventDefault();
    close();
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    if (items.length) {
      sel = (sel + 1) % items.length;
      highlight();
    }
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    if (items.length) {
      sel = (sel - 1 + items.length) % items.length;
      highlight();
    }
  } else if (e.key === "Enter") {
    e.preventDefault();
    if (sel >= 0 && items[sel]) go(items[sel]!.dataset.href!);
  }
}

function go(href: string): void {
  close();
  location.hash = href;
}

/** Whether focus is somewhere the "/" shortcut should stay literal. */
function isEditable(el: Element | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
}

export function openCommandPalette(): void {
  build();
  if (!overlay || overlay.hidden === false) return;
  lastFocus = document.activeElement;
  overlay.hidden = false;
  input!.value = "";
  paint([], [], [], false);
  sel = -1;
  input!.focus();
}

export function close(): void {
  if (!overlay || overlay.hidden) return;
  overlay.hidden = true;
  window.clearTimeout(debounce);
  queryToken++; // invalidate any in-flight lesson fetch
  if (lastFocus instanceof HTMLElement) lastFocus.focus();
}

/** Install the global open shortcuts. Call once at startup. */
export function installCommandPalette(): void {
  build();
  document.addEventListener("keydown", (e) => {
    const meta = e.metaKey || e.ctrlKey;
    if (meta && (e.key === "k" || e.key === "K")) {
      e.preventDefault();
      overlay && !overlay.hidden ? close() : openCommandPalette();
      return;
    }
    // "/" is a quick-open, but only when not typing into a field or the palette.
    if (e.key === "/" && !meta && !e.altKey && (!overlay || overlay.hidden) && !isEditable(document.activeElement)) {
      e.preventDefault();
      openCommandPalette();
    }
  });
}
