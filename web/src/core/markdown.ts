// A tiny, safe Markdown-to-HTML renderer. It escapes all HTML first, so raw
// tags in the source are shown literally rather than injected — the lesson body
// comes from the server, but defence-in-depth is cheap. Supports the subset a
// lesson needs: headings, bold/italic, inline and fenced code, links, unordered
// and ordered lists, blockquotes, GFM tables and paragraphs. No external
// dependency.

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// Inline spans applied to already-escaped text.
function inline(text: string): string {
  return text
    .replace(/`([^`]+)`/g, (_m, c: string) => `<code>${c}</code>`)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>")
    .replace(
      /\[([^\]]+)\]\(([^)\s]+)\)/g,
      (_m, label: string, href: string) => {
        // Only allow http(s), mailto and in-app hash links.
        const safe = /^(https?:|mailto:|#)/i.test(href) ? href : "#";
        return `<a href="${safe}" rel="noopener noreferrer">${label}</a>`;
      },
    );
}

// A GFM table separator row: pipes and dashes only, with optional `:` for
// alignment, e.g. `|---|:--:|---:|`. This is what distinguishes a table header
// from an ordinary paragraph line that merely happens to contain a pipe.
function isTableSeparator(s: string): boolean {
  const t = s.trim();
  if (!t.includes("-")) return false;
  return /^\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)+\|?$/.test(t);
}

// Split `| a | b | c |` into `["a", "b", "c"]`. The source has already been
// HTML-escaped, so any `|` here is a real cell delimiter; lesson tables do not
// use escaped pipes.
function splitTableRow(s: string): string[] {
  let t = s.trim();
  if (t.startsWith("|")) t = t.slice(1);
  if (t.endsWith("|")) t = t.slice(0, -1);
  return t.split("|").map((c) => c.trim());
}

function cellAlign(spec: string): string {
  const t = spec.trim();
  const left = t.startsWith(":");
  const right = t.endsWith(":");
  if (left && right) return ' style="text-align:center"';
  if (right) return ' style="text-align:right"';
  if (left) return ' style="text-align:left"';
  return "";
}

export function renderMarkdown(src: string): string {
  const lines = escapeHtml(src.replace(/\r\n?/g, "\n")).split("\n");
  const out: string[] = [];
  let i = 0;
  let listType: "ul" | "ol" | null = null;

  const closeList = () => {
    if (listType) {
      out.push(`</${listType}>`);
      listType = null;
    }
  };

  while (i < lines.length) {
    const line = lines[i]!;

    // fenced code block
    if (/^```/.test(line)) {
      closeList();
      i++;
      const buf: string[] = [];
      while (i < lines.length && !/^```/.test(lines[i]!)) {
        buf.push(lines[i]!);
        i++;
      }
      i++; // skip closing fence
      out.push(`<pre class="md-code"><code>${buf.join("\n")}</code></pre>`);
      continue;
    }

    // GFM table: a header row, then a separator row, then body rows. The
    // separator on the second line is what tells a table apart from a
    // paragraph that merely contains a pipe.
    if (
      line.includes("|") &&
      i + 1 < lines.length &&
      isTableSeparator(lines[i + 1]!)
    ) {
      closeList();
      const headers = splitTableRow(line);
      const aligns = splitTableRow(lines[i + 1]!).map(cellAlign);
      i += 2;

      const head = headers
        .map((c, j) => `<th${aligns[j] ?? ""}>${inline(c)}</th>`)
        .join("");
      const body: string[] = [];
      while (
        i < lines.length &&
        lines[i]!.trim() !== "" &&
        lines[i]!.includes("|")
      ) {
        const cells = splitTableRow(lines[i]!);
        // Pad or truncate to the header width, as GFM does.
        const tds: string[] = [];
        for (let j = 0; j < headers.length; j++) {
          tds.push(`<td${aligns[j] ?? ""}>${inline(cells[j] ?? "")}</td>`);
        }
        body.push(`<tr>${tds.join("")}</tr>`);
        i++;
      }

      out.push(
        `<table class="md-table"><thead><tr>${head}</tr></thead>` +
          `<tbody>${body.join("")}</tbody></table>`,
      );
      continue;
    }

    // heading
    const h = /^(#{1,6})\s+(.*)$/.exec(line);
    if (h) {
      closeList();
      const level = h[1]!.length;
      out.push(`<h${level}>${inline(h[2]!)}</h${level}>`);
      i++;
      continue;
    }

    // blockquote
    if (/^>\s?/.test(line)) {
      closeList();
      out.push(`<blockquote>${inline(line.replace(/^>\s?/, ""))}</blockquote>`);
      i++;
      continue;
    }

    // unordered list
    const ul = /^[-*+]\s+(.*)$/.exec(line);
    if (ul) {
      if (listType !== "ul") {
        closeList();
        out.push("<ul>");
        listType = "ul";
      }
      out.push(`<li>${inline(ul[1]!)}</li>`);
      i++;
      continue;
    }

    // ordered list
    const ol = /^\d+[.)]\s+(.*)$/.exec(line);
    if (ol) {
      if (listType !== "ol") {
        closeList();
        out.push("<ol>");
        listType = "ol";
      }
      out.push(`<li>${inline(ol[1]!)}</li>`);
      i++;
      continue;
    }

    // blank line
    if (line.trim() === "") {
      closeList();
      i++;
      continue;
    }

    // paragraph (accumulate consecutive non-blank lines)
    closeList();
    const para: string[] = [line];
    i++;
    while (
      i < lines.length &&
      lines[i]!.trim() !== "" &&
      !/^(#{1,6}\s|```|>|[-*+]\s|\d+[.)]\s)/.test(lines[i]!)
    ) {
      para.push(lines[i]!);
      i++;
    }
    out.push(`<p>${inline(para.join(" "))}</p>`);
  }

  closeList();
  return out.join("\n");
}
