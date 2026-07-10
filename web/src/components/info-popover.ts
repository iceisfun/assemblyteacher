// The shared popover controller. One popover element for the whole page; token
// spans are handled by event delegation so a lesson with hundreds of tokens
// costs nothing per token.
//
// Interaction model, consistent with the rest of the app:
//   - desktop hover  → a lightweight preview that follows the pointer and
//                      disappears on mouse-out
//   - click / tap    → a pinned card that stays until dismissed (Esc, a click
//                      outside, or the close button). This is the mobile path,
//                      which has no hover.
//   - keyboard       → tokens are focusable; Enter/Space pins, Esc closes.
//
// It also hydrates the always-open inline embeds emitted by the `:::number` /
// `:::instruction` block directives.

import { buildInsnCard, buildNumberCard, buildRegCard } from "./help-cards.ts";

function embedCard(kind: string | undefined, arg: string): HTMLElement {
  if (kind === "instruction") return buildInsnCard(arg.split(/\s+/)[0] ?? "", arg);
  if (kind === "register") return buildRegCard(arg.split(/\s+/)[0] ?? "");
  return buildNumberCard(arg);
}

let popover: HTMLElement | null = null;
let pinned = false;
let hideTimer: number | undefined;
/** The token the popover is currently anchored to, for re-positioning. */
let anchor: HTMLElement | null = null;

function ensurePopover(): HTMLElement {
  if (popover) return popover;
  const p = document.createElement("div");
  p.className = "help-popover";
  p.setAttribute("role", "dialog");
  p.hidden = true;
  // Keep the popover open while the pointer is over it (so its controls work).
  p.addEventListener("mouseenter", () => window.clearTimeout(hideTimer));
  p.addEventListener("mouseleave", () => {
    if (!pinned) scheduleHide();
  });
  // A card whose contents changed size (the number width toggle) asks to be
  // re-placed so it does not drift off-screen.
  p.addEventListener("help-resize", () => {
    if (anchor) position(p, anchor);
  });
  document.body.appendChild(p);
  popover = p;
  return p;
}

function cardFor(token: HTMLElement): HTMLElement | null {
  const kind = token.dataset.help;
  if (kind === "num" && token.dataset.lit) {
    return buildNumberCard(token.dataset.lit);
  }
  if (kind === "insn" && token.dataset.insn) {
    return buildInsnCard(token.dataset.insn, token.dataset.context);
  }
  if (kind === "reg" && token.dataset.reg) {
    return buildRegCard(token.dataset.reg);
  }
  return null;
}

function show(token: HTMLElement, pin: boolean): void {
  const card = cardFor(token);
  if (!card) return;
  const p = ensurePopover();
  window.clearTimeout(hideTimer);
  pinned = pin;
  anchor = token;

  p.innerHTML = "";
  if (pin) {
    const close = document.createElement("button");
    close.className = "help-popover-close";
    close.setAttribute("aria-label", "close");
    close.textContent = "×";
    close.addEventListener("click", hide);
    p.appendChild(close);
  }
  p.appendChild(card);
  p.classList.toggle("pinned", pin);
  p.hidden = false;

  position(p, token);
}

function position(p: HTMLElement, token: HTMLElement): void {
  // Measure, then place below the token, flipping above if it would overflow,
  // and clamped horizontally into the viewport.
  p.style.left = "0px";
  p.style.top = "0px";
  const t = token.getBoundingClientRect();
  const pr = p.getBoundingClientRect();
  const margin = 8;
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;

  let left = t.left + window.scrollX;
  left = Math.min(left, window.scrollX + vw - pr.width - margin);
  left = Math.max(left, window.scrollX + margin);

  let top = t.bottom + window.scrollY + 6;
  if (t.bottom + pr.height + margin > vh) {
    const above = t.top + window.scrollY - pr.height - 6;
    if (above > window.scrollY + margin) top = above;
  }
  p.style.left = `${Math.round(left)}px`;
  p.style.top = `${Math.round(top)}px`;
}

function scheduleHide(): void {
  window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(hide, 120);
}

function hide(): void {
  if (!popover) return;
  popover.hidden = true;
  popover.classList.remove("pinned");
  pinned = false;
}

function closestToken(target: EventTarget | null): HTMLElement | null {
  return target instanceof Element
    ? target.closest<HTMLElement>(".tok[data-help]")
    : null;
}

/**
 * Wire token interactions within `root`, and hydrate any inline help embeds.
 * Safe to call repeatedly (e.g. each time a lesson is rendered) — the global
 * document-level listeners are installed only once.
 */
export function installTokenHelpers(root: ParentNode): void {
  hydrateEmbeds(root);
  installGlobal();
}

let globalInstalled = false;
function installGlobal(): void {
  if (globalInstalled) return;
  globalInstalled = true;

  document.addEventListener("mouseover", (e) => {
    const tok = closestToken(e.target);
    if (tok && !pinned) show(tok, false);
  });
  document.addEventListener("mouseout", (e) => {
    if (closestToken(e.target) && !pinned) scheduleHide();
  });
  document.addEventListener("click", (e) => {
    const tok = closestToken(e.target);
    if (tok) {
      e.preventDefault();
      // Tapping the already-pinned token closes it.
      if (pinned && popover && !popover.hidden) hide();
      else show(tok, true);
      return;
    }
    // A click outside the popover dismisses a pinned card.
    if (pinned && popover && !popover.contains(e.target as Node)) hide();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") return hide();
    const tok = closestToken(document.activeElement);
    if (tok && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      show(tok, true);
    }
  });
  window.addEventListener("scroll", () => {
    if (!pinned) hide();
  });
}

function hydrateEmbeds(root: ParentNode): void {
  for (const host of root.querySelectorAll<HTMLElement>(".help-embed")) {
    if (host.dataset.hydrated) continue;
    host.dataset.hydrated = "1";
    const card = embedCard(host.dataset.embed, host.dataset.arg ?? "");
    card.classList.add("help-card-inline");
    host.appendChild(card);
  }
}
