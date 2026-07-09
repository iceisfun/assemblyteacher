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

const NAV: Array<[string, string]> = [
  ["#/playground", "Playground"],
  ["#/lessons", "Lessons"],
  ["#/inspector", "Inspector"],
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
  renderPlayground(view);
}

window.addEventListener("hashchange", route);
if (!location.hash) location.hash = "#/playground";
route();
