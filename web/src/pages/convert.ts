// The byte / value converter (#/convert). Paste what a hex dump shows —
// `aa bb cc dd` — and get the value you actually need (`0xddccbbaa`, little-
// endian), or go the other way from a `0x…` value to its byte layout. Handles
// arbitrary widths up to 64-bit, in both byte orders, with the full binary
// decomposition from the number card.

import { buildNumberCard } from "../components/help-cards.ts";
import {
  bytesOfValue,
  formatBytes,
  parseHexBytes,
  signedOf,
  valueOfBytes,
  type Endian,
} from "../core/convert.ts";
import { parseNumberLiteral } from "../core/numinfo.ts";

type Mode = "bytes" | "number";

function el(tag: string, className?: string, text?: string): HTMLElement {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function copyable(text: string, value: string): HTMLElement {
  const s = el("span", "cv-copy", text);
  s.tabIndex = 0;
  s.title = "click to copy";
  const copy = () => {
    void navigator.clipboard?.writeText(value);
    s.classList.add("copied");
    setTimeout(() => s.classList.remove("copied"), 600);
  };
  s.addEventListener("click", copy);
  s.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      copy();
    }
  });
  return s;
}

export function renderConvert(view: HTMLElement): void {
  view.innerHTML = "";
  const page = el("div", "cv-page");

  const header = el("div", "cv-header");
  header.appendChild(el("h1", undefined, "Byte / Value Converter"));
  header.appendChild(
    el(
      "p",
      "cv-intro",
      "A hex dump shows bytes in memory order — paste them to get the value, or " +
        "paste a value to get its bytes. Little-endian is the order x86 stores " +
        "integers in, so `aa bb cc dd` in a dump is the value 0xddccbbaa.",
    ),
  );
  page.appendChild(header);

  // ---- controls ----
  const controls = el("div", "cv-controls");
  const input = document.createElement("input");
  input.className = "cv-input";
  input.type = "text";
  input.placeholder = "aa bb cc dd   ·   0xffffff00   ·   00 00 00 10";
  input.setAttribute("aria-label", "hex bytes or a value");
  input.spellcheck = false;
  controls.appendChild(input);

  let mode: Mode = "bytes";
  let endian: Endian = "le";

  const modeGroup = radioGroup("read input as", [
    ["bytes", "hex bytes"],
    ["number", "a number"],
  ], mode, (v) => {
    mode = v as Mode;
    update();
  });
  const endianGroup = radioGroup("byte order", [
    ["le", "little-endian (x86 / dump)"],
    ["be", "big-endian (network)"],
  ], endian, (v) => {
    endian = v as Endian;
    update();
  });
  controls.append(modeGroup, endianGroup);
  page.appendChild(controls);

  const out = el("div", "cv-out");
  page.appendChild(out);
  view.appendChild(page);

  function update(): void {
    out.innerHTML = "";
    const raw = input.value.trim();
    if (!raw) {
      out.appendChild(el("div", "cv-hint", "Enter hex bytes (like a dump) or a value."));
      return;
    }

    // Resolve to a value + byte length, honouring the mode.
    let value: bigint;
    let byteLen: number;
    if (mode === "number") {
      const info = parseNumberLiteral(raw.startsWith("0x") || /^\d/.test(raw) || raw.startsWith("0b") ? raw : `0x${raw}`);
      if (!info) {
        out.appendChild(el("div", "cv-error", "That is not a number. Try 0x…, a decimal, or switch to hex bytes."));
        return;
      }
      value = info.value;
      byteLen = info.width / 8;
    } else {
      const bytes = parseHexBytes(raw);
      if (!bytes) {
        out.appendChild(el("div", "cv-error", "That is not a run of hex bytes. Use pairs like `aa bb cc dd`, or switch to number mode."));
        return;
      }
      if (bytes.length > 8) {
        out.appendChild(el("div", "cv-error", "Up to 8 bytes (64 bits)."));
        return;
      }
      value = valueOfBytes(bytes, endian);
      byteLen = bytes.length;
    }

    const bits = byteLen * 8;
    const hex = "0x" + value.toString(16).padStart(byteLen * 2, "0");

    // Headline: the value, big and copyable — the thing you came to retype.
    const headline = el("div", "cv-headline");
    headline.appendChild(el("span", "cv-headline-label", `${bits}-bit value`));
    headline.appendChild(copyable(hex, hex));
    out.appendChild(headline);

    // Readings.
    const readings = el("div", "cv-readings");
    const row = (label: string, text: string, copyVal?: string) => {
      const r = el("div", "cv-reading");
      r.appendChild(el("span", "cv-label", label));
      r.appendChild(copyVal !== undefined ? copyable(text, copyVal) : el("span", "cv-value", text));
      readings.appendChild(r);
    };
    const dec = value.toString(10);
    const sig = signedOf(value, bits).toString(10);
    row("unsigned", dec, dec);
    row("signed", sig, sig);
    out.appendChild(readings);

    // Byte layouts in both orders — the crux of the tool.
    const layouts = el("div", "cv-layouts");
    layouts.appendChild(el("div", "cv-section-title", "bytes in memory"));
    const leBytes = formatBytes(bytesOfValue(value, byteLen, "le"));
    const beBytes = formatBytes(bytesOfValue(value, byteLen, "be"));
    const layoutRow = (label: string, bytesText: string, active: boolean) => {
      const r = el("div", `cv-layout${active ? " active" : ""}`);
      r.appendChild(el("span", "cv-label", label));
      r.appendChild(copyable(bytesText, bytesText));
      layouts.appendChild(r);
    };
    layoutRow("little-endian", leBytes, endian === "le");
    layoutRow("big-endian", beBytes, endian === "be");
    out.appendChild(layouts);

    // Full binary decomposition, reusing the number card.
    const decomp = el("div", "cv-decomp");
    const card = buildNumberCard(hex);
    card.classList.add("help-card-inline");
    decomp.appendChild(card);
    out.appendChild(decomp);
  }

  input.addEventListener("input", update);
  input.value = "aa bb cc dd";
  update();
  input.focus();
}

let radioSeq = 0;

function radioGroup(
  label: string,
  options: Array<[string, string]>,
  selected: string,
  onChange: (value: string) => void,
): HTMLElement {
  const group = el("div", "cv-radio-group");
  group.appendChild(el("span", "cv-label", label));
  const name = `cv-${label.replace(/\s+/g, "-")}-${radioSeq++}`;
  for (const [value, text] of options) {
    const wrap = el("label", "cv-radio");
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = name;
    radio.value = value;
    radio.checked = value === selected;
    radio.addEventListener("change", () => radio.checked && onChange(value));
    wrap.append(radio, document.createTextNode(text));
    group.appendChild(wrap);
  }
  return group;
}
