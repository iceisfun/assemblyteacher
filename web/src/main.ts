// Entry point: a tiny hash-router wiring the three pages together, plus the top
// navigation. Importing the component modules registers the custom elements.

import "./style.css";
import "./components/memory-viewer.ts";
import "./components/register-view.ts";
import "./components/stack-view.ts";
import "./components/insn-explain.ts";
import "./components/code-editor.ts";

import { renderPlayground } from "./pages/playground.ts";
import { renderLessonIndex, renderLesson } from "./pages/lessons.ts";
import { renderInspector } from "./pages/inspector.ts";
import { renderRegisters } from "./pages/registers.ts";
import { renderInstructions } from "./pages/instructions.ts";
import { renderConvert } from "./pages/convert.ts";

const NAV: Array<[string, string]> = [
  ["#/playground", "Playground"],
  ["#/lessons", "Lessons"],
  ["#/inspector", "Inspector"],
  ["#/registers", "Registers"],
  ["#/instructions", "Instructions"],
  ["#/convert", "Convert"],
];

function mountShell(): { view: HTMLElement; nav: HTMLElement } {
  const app = document.getElementById("app")!;
  app.innerHTML = `
    <header class="topbar">
      <div class="brand">Assembly<span>Teacher</span></div>
      <nav class="nav"></nav>
    </header>
    <main class="view"></main>
  `;
  const nav = app.querySelector<HTMLElement>(".nav")!;
  for (const [href, label] of NAV) {
    const a = document.createElement("a");
    a.href = href;
    a.textContent = label;
    a.dataset.route = href;
    nav.appendChild(a);
  }
  return { view: app.querySelector<HTMLElement>(".view")!, nav };
}

const shell = mountShell();

function highlightNav(route: string): void {
  for (const a of shell.nav.querySelectorAll<HTMLAnchorElement>("a")) {
    a.classList.toggle("active", route.startsWith(a.dataset.route ?? ""));
  }
}

function route(): void {
  const hash = location.hash || "#/playground";
  highlightNav(hash);
  const view = shell.view;

  const lessonMatch = /^#\/lessons\/(.+)$/.exec(hash);
  if (lessonMatch) {
    void renderLesson(view, decodeURIComponent(lessonMatch[1]!));
    return;
  }
  if (hash.startsWith("#/lessons")) {
    void renderLessonIndex(view);
    return;
  }
  if (hash.startsWith("#/inspector")) {
    renderInspector(view);
    return;
  }
  const regMatch = /^#\/registers\/(.+)$/.exec(hash);
  if (regMatch) {
    renderRegisters(view, decodeURIComponent(regMatch[1]!));
    return;
  }
  if (hash.startsWith("#/registers")) {
    renderRegisters(view);
    return;
  }
  const insnMatch = /^#\/instructions\/(.+)$/.exec(hash);
  if (insnMatch) {
    renderInstructions(view, decodeURIComponent(insnMatch[1]!));
    return;
  }
  if (hash.startsWith("#/instructions")) {
    renderInstructions(view);
    return;
  }
  if (hash.startsWith("#/convert")) {
    renderConvert(view);
    return;
  }
  renderPlayground(view);
}

window.addEventListener("hashchange", route);
if (!location.hash) location.hash = "#/playground";
route();
