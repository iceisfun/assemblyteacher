// The Lessons pages: the curriculum index (#/lessons) and a single lesson
// (#/lessons/:id) rendering the markdown body plus its exercises. Exercise
// widgets: quiz (radio + check), assemble (embedded <code-editor> + check).

import {
  lessons as fetchLessons,
  lesson as fetchLesson,
  checkExercise,
  ApiError,
  type Exercise,
  type Lesson,
  type LessonIndex,
} from "../api.ts";
import { renderMarkdown } from "../core/markdown.ts";
import { CodeEditor } from "../components/code-editor.ts";

export async function renderLessonIndex(root: HTMLElement): Promise<void> {
  root.innerHTML = `<div class="lessons"><h1>Curriculum</h1><div class="lessons-body">loading…</div></div>`;
  const body = root.querySelector<HTMLElement>(".lessons-body")!;
  let index: LessonIndex;
  try {
    index = await fetchLessons();
  } catch (e) {
    body.innerHTML = "";
    body.appendChild(
      notice(
        e instanceof ApiError ? e.message : "lessons unavailable (offline?)",
        "warn",
      ),
    );
    return;
  }
  body.innerHTML = "";
  for (const part of index.parts) {
    const sec = document.createElement("section");
    sec.className = "part";
    sec.innerHTML = `<h2>Part ${part.number}. ${escapeHtml(part.title)}</h2>`;
    const grid = document.createElement("div");
    grid.className = "lesson-grid";
    for (const l of [...part.lessons].sort((a, b) => a.order - b.order)) {
      const card = document.createElement("a");
      card.className = "lesson-card";
      card.href = `#/lessons/${encodeURIComponent(l.id)}`;
      card.innerHTML =
        `<div class="lc-title">${escapeHtml(l.title)}</div>` +
        `<div class="lc-obj">${l.objectives.map(escapeHtml).slice(0, 3).join(" · ")}</div>` +
        `<div class="lc-meta">${l.exerciseCount} exercise${l.exerciseCount === 1 ? "" : "s"}</div>`;
      grid.appendChild(card);
    }
    sec.appendChild(grid);
    body.appendChild(sec);
  }
}

export async function renderLesson(root: HTMLElement, id: string): Promise<void> {
  root.innerHTML = `<div class="lesson"><a class="back" href="#/lessons">← all lessons</a><div class="lesson-body">loading…</div></div>`;
  const body = root.querySelector<HTMLElement>(".lesson-body")!;
  let lesson: Lesson;
  try {
    lesson = await fetchLesson(id);
  } catch (e) {
    body.innerHTML = "";
    body.appendChild(
      notice(e instanceof ApiError ? e.message : "lesson unavailable (offline?)", "warn"),
    );
    return;
  }
  body.innerHTML = "";

  const header = document.createElement("header");
  header.className = "lesson-head";
  header.innerHTML = `<h1>${escapeHtml(lesson.title)}</h1>`;
  if (lesson.objectives.length) {
    const ul = document.createElement("ul");
    ul.className = "objectives";
    for (const o of lesson.objectives) {
      const li = document.createElement("li");
      li.textContent = o;
      ul.appendChild(li);
    }
    header.appendChild(ul);
  }
  body.appendChild(header);

  const article = document.createElement("article");
  article.className = "prose";
  article.innerHTML = renderMarkdown(lesson.body);
  body.appendChild(article);

  if (lesson.exercises.length) {
    const exWrap = document.createElement("section");
    exWrap.className = "exercises";
    exWrap.innerHTML = "<h2>Exercises</h2>";
    lesson.exercises.forEach((ex, i) => {
      exWrap.appendChild(renderExercise(lesson.id, ex, i + 1));
    });
    body.appendChild(exWrap);
  }
}

function renderExercise(lessonId: string, ex: Exercise, n: number): HTMLElement {
  const card = document.createElement("div");
  card.className = "exercise";
  const prompt = document.createElement("div");
  prompt.className = "ex-prompt";
  prompt.innerHTML = `<span class="ex-num">${n}</span><span class="ex-kind">${ex.kind}</span> ${escapeHtml(ex.prompt)}`;
  card.appendChild(prompt);

  const verdict = document.createElement("div");
  verdict.className = "ex-verdict";
  verdict.hidden = true;

  const submit = async (answer: string): Promise<void> => {
    verdict.hidden = false;
    verdict.className = "ex-verdict";
    verdict.textContent = "checking…";
    try {
      const res = await checkExercise(lessonId, ex.id, { answer });
      verdict.classList.add(res.correct ? "ok" : "bad");
      const hints = res.hints.length
        ? "\nHints:\n• " + res.hints.join("\n• ")
        : "";
      verdict.textContent = (res.correct ? "✓ " : "✗ ") + res.message + hints;
    } catch (e) {
      verdict.classList.add("bad");
      verdict.textContent =
        e instanceof ApiError ? e.message : "check unavailable (offline?)";
    }
  };

  if (ex.kind === "quiz" && ex.choices) {
    // The check endpoint wants the choice INDEX as a decimal string ("0","1").
    const form = document.createElement("form");
    form.className = "ex-quiz";
    const name = `q-${ex.id}`;
    ex.choices.forEach((choice, i) => {
      const label = document.createElement("label");
      label.className = "ex-choice";
      label.innerHTML =
        `<input type="radio" name="${name}" value="${i}" /> ` +
        escapeHtml(choice);
      form.appendChild(label);
    });
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "btn";
    btn.textContent = "Check";
    btn.addEventListener("click", () => {
      const sel = form.querySelector<HTMLInputElement>("input:checked");
      if (!sel) {
        verdict.hidden = false;
        verdict.className = "ex-verdict";
        verdict.textContent = "pick an answer first";
        return;
      }
      void submit(sel.value);
    });
    form.appendChild(btn);
    card.appendChild(form);
  } else if (ex.kind === "disassemble") {
    // Show the raw bytes; the student types the instruction. We deliberately do
    // NOT explain them — that would give the answer away.
    const bytes = document.createElement("div");
    bytes.className = "ex-bytes";
    bytes.textContent = ex.hex ?? "";
    card.appendChild(bytes);
    const input = document.createElement("input");
    input.type = "text";
    input.className = "ex-text-answer";
    input.placeholder = "e.g. mov rax, qword [rsp+0x8]";
    input.setAttribute("aria-label", "Your disassembly");
    card.appendChild(input);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "btn";
    btn.textContent = "Check";
    btn.addEventListener("click", () => void submit(input.value));
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") void submit(input.value);
    });
    card.appendChild(btn);
  } else {
    // assemble / emulate: embedded editor + check
    const editor = new CodeEditor();
    editor.autoAssemble = false;
    if (ex.starter) editor.value = ex.starter;
    card.appendChild(editor);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "btn";
    btn.textContent = "Submit";
    btn.addEventListener("click", () => void submit(editor.value));
    card.appendChild(btn);
  }

  card.appendChild(verdict);
  return card;
}

function notice(text: string, kind: "warn" | "info"): HTMLElement {
  const el = document.createElement("div");
  el.className = `notice notice-${kind}`;
  el.textContent = text;
  return el;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
