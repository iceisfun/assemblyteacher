// The full register reference (#/registers, #/registers/<name>): the entire
// general-purpose register file on one linkable page. The left panel is the
// interactive card for the selected register; the right is every family, with
// the selected register's path from its 64-bit root highlighted.
//
// Selection lives in the URL, so any register is directly linkable and the
// hover card can jump here with "full register reference →".

import { buildRegCard } from "../components/help-cards.ts";
import {
  ancestorsOf,
  familyTree,
  largestOf,
  lookupReg,
  specialReg,
  type RegNode,
} from "../core/reginfo.ts";

const FAMILIES_R64 = [
  "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
  "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
];

function el(tag: string, className?: string, text?: string): HTMLElement {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function goTo(name: string): void {
  location.hash = `#/registers/${name}`;
}

/** Render a family's tree as nested clickable chips, highlighting the path to
 *  the selected register when this is the selected register's family. */
function familyBlock(r64: string, selected: string, onPath: Set<string>): HTMLElement {
  const isActiveFamily = largestOf(selected) === r64;
  const block = el("div", `reg-fam${isActiveFamily ? " active" : ""}`);

  const tree = el("div", "reg-fam-tree");
  const renderNode = (node: RegNode, depth: number): void => {
    const row = el("div", "reg-node-row");
    row.style.paddingLeft = `${depth * 14}px`;
    if (depth > 0) row.appendChild(el("span", "reg-node-branch", "└─"));
    let cls = "reg-node reg-fam-node";
    if (node.name === selected) cls += " selected";
    else if (isActiveFamily && onPath.has(node.name)) cls += " onpath";
    const chip = el("span", cls, node.name);
    chip.tabIndex = 0;
    chip.setAttribute("role", "button");
    chip.title = `${node.width}-bit · bits ${node.bitLo}–${node.bitHi}`;
    chip.addEventListener("click", () => goTo(node.name));
    chip.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        goTo(node.name);
      }
    });
    row.appendChild(chip);
    tree.appendChild(row);
    for (const child of node.children) renderNode(child, depth + 1);
  };
  const root = familyTree(r64);
  if (root) renderNode(root, 0);
  block.appendChild(tree);

  const info = lookupReg(r64);
  if (info) {
    block.appendChild(el("div", "reg-fam-role", info.role));
  }
  return block;
}

export function renderRegisters(view: HTMLElement, selectedRaw?: string): void {
  const selected =
    selectedRaw && (lookupReg(selectedRaw) || specialReg(selectedRaw))
      ? selectedRaw.toLowerCase()
      : "rax";
  view.innerHTML = "";

  const page = el("div", "reg-page");
  const header = el("div", "reg-page-header");
  header.appendChild(el("h1", undefined, "The x86-64 Register File"));
  header.appendChild(
    el(
      "p",
      "reg-page-intro",
      "Sixteen general-purpose registers, each addressable at four widths. Pick any " +
        "name to see the bits it owns, how it aliases the others, and what a write to it " +
        "does. Every register here is linkable.",
    ),
  );
  page.appendChild(header);

  const layout = el("div", "reg-page-layout");

  const detail = el("div", "reg-page-detail");
  detail.appendChild(buildRegCard(selected, { onNavigate: goTo }));
  layout.appendChild(detail);

  const grid = el("div", "reg-page-grid");
  const onPath = new Set<string>([selected, ...ancestorsOf(selected)]);
  for (const r64 of FAMILIES_R64) {
    grid.appendChild(familyBlock(r64, selected, onPath));
  }
  layout.appendChild(grid);

  page.appendChild(layout);
  view.appendChild(page);

  // Bring the selected register's family into view for a deep link.
  const active = grid.querySelector<HTMLElement>(".reg-fam.active");
  active?.scrollIntoView({ block: "nearest", behavior: "auto" });
}
