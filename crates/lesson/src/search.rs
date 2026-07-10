//! Full-text-ish search across the curriculum.
//!
//! The whole corpus is a few hundred KB and already resident in memory, so a
//! linear, field-weighted scan per query is far cheaper than maintaining an
//! index — and it means a term is findable the instant it is written into a
//! lesson, with no keyword metadata to keep in sync. A small concept map
//! bridges phrases that a reader might type but that a lesson does not spell out
//! verbatim, mirroring the frontend instruction search's alias table.

use crate::model::Curriculum;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub part: u32,
    /// The field the match was strongest in — for display and debugging.
    pub field: &'static str,
    /// The nearest heading and the matched line, cleaned of markdown noise.
    pub snippet: String,
    pub score: i32,
}

// Phrases a reader might search for that should surface a lesson even when the
// exact words are not in its prose. Key is lowercased; value is the lesson ids
// to boost. Terms that already appear verbatim in the corpus do not need an
// entry — the scan finds those on its own.
const CONCEPTS: &[(&str, &[&str])] = &[
    ("little endian", &["endianness"]),
    ("big endian", &["endianness"]),
    ("byte order", &["endianness"]),
    ("stack cookie", &["stack-canaries-and-cookies"]),
    ("stack smashing", &["stack-canaries-and-cookies"]),
    ("buffer overflow", &["the-stack", "stack-canaries-and-cookies"]),
    ("signature scan", &["aob-scanning"]),
    ("pattern scan", &["aob-scanning"]),
    ("wildcard", &["aob-scanning"]),
    ("code before main", &["tls-callbacks", "entry-point-to-main"]),
    ("position independent", &["elf-disk-to-memory", "rebasing-and-windows-aslr"]),
    ("api set", &["iat-and-imports"]),
];

/// Rank the curriculum against `query`, best first, at most `limit` hits.
pub fn search(curriculum: &Curriculum, query: &str, limit: usize) -> Vec<SearchHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = q.split_whitespace().collect();

    let mut hits: Vec<SearchHit> = Vec::new();
    for lesson in curriculum.lessons() {
        let title = lesson.title.to_lowercase();
        let id = lesson.id.to_lowercase();

        // Collect every field that matched, then keep the strongest.
        let mut cands: Vec<(i32, &'static str)> = Vec::new();

        if title == q {
            cands.push((100, "title"));
        } else if title.contains(&q) {
            cands.push((78, "title"));
        }
        if id != q && id.replace('-', " ").contains(&q) {
            cands.push((66, "id"));
        }
        if lesson.objectives.iter().any(|o| o.to_lowercase().contains(&q)) {
            cands.push((56, "objective"));
        }

        // Headings weigh more than plain body text.
        let (heading_hit, body_hit) = scan_body(&lesson.body, &q);
        if heading_hit {
            cands.push((48, "heading"));
        } else if body_hit {
            cands.push((32, "body"));
        }

        if lesson.exercises.iter().any(|e| e.prompt.to_lowercase().contains(&q)) {
            cands.push((28, "exercise"));
        }

        // Multi-word queries: every word present somewhere, even if not adjacent.
        if words.len() > 1 && cands.is_empty() {
            let blob = format!("{title} {id} {}", lesson.objectives.join(" ").to_lowercase());
            let blob = format!("{blob} {}", lesson.body.to_lowercase());
            if words.iter().all(|w| blob.contains(w)) {
                cands.push((24, "text"));
            }
        }

        // Concept/alias boost.
        for (phrase, ids) in CONCEPTS {
            if q.contains(phrase) && ids.contains(&id.as_str()) {
                cands.push((64, "concept"));
            }
        }

        if let Some((score, field)) = cands.into_iter().max_by_key(|(s, _)| *s) {
            hits.push(SearchHit {
                id: lesson.id.clone(),
                title: lesson.title.clone(),
                part: lesson.part,
                field,
                snippet: make_snippet(&lesson.body, &q),
                score,
            });
        }
    }

    // Highest score first; ties broken by title for a stable order.
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit);
    hits
}

/// Does the query appear in a heading line and/or a body line?
fn scan_body(body: &str, q: &str) -> (bool, bool) {
    let mut heading = false;
    let mut body_hit = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.to_lowercase().contains(q) {
            continue;
        }
        if trimmed.starts_with('#') {
            heading = true;
        } else {
            body_hit = true;
        }
    }
    (heading, body_hit)
}

/// A short context string: the nearest heading above the first matching line,
/// then the matching line itself, windowed around the match and de-marked.
fn make_snippet(body: &str, q: &str) -> String {
    let mut heading = String::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            heading = clean_markdown(rest.trim_start_matches('#').trim());
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.to_lowercase().contains(q) {
            let line = window_around(&clean_markdown(trimmed), q, 130);
            return if heading.is_empty() { line } else { format!("{heading} — {line}") };
        }
    }
    // No body line matched (the hit was in the title or an objective): fall back
    // to the first real sentence of the lesson.
    for line in body.lines() {
        let t = line.trim();
        if !t.is_empty() && !t.starts_with('#') {
            return window_around(&clean_markdown(t), "", 130);
        }
    }
    String::new()
}

/// Strip the markdown that would look like noise in a one-line result.
fn clean_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '`' | '*' | '_' | '>' => {}
            _ => out.push(ch),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Trim `line` to `max` characters, keeping the window centred on `q`.
fn window_around(line: &str, q: &str, max: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= max {
        return line.to_string();
    }
    let lc = line.to_lowercase();
    let pos = if q.is_empty() {
        0
    } else {
        lc.find(q).map(|b| lc[..b].chars().count()).unwrap_or(0)
    };
    let mut start = pos.saturating_sub(max / 3);
    let end = (start + max).min(chars.len());
    start = end.saturating_sub(max);
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    fn corpus() -> Curriculum {
        load("../../lessons").expect("load curriculum")
    }

    #[test]
    fn finds_a_literal_term_in_the_body() {
        let c = corpus();
        let hits = search(&c, "ModRM", 10);
        assert!(
            hits.iter().any(|h| h.id == "addressing-modes"),
            "ModRM should surface the addressing-modes lesson: {hits:#?}"
        );
    }

    #[test]
    fn a_title_word_outranks_a_passing_mention() {
        let c = corpus();
        let hits = search(&c, "endianness", 10);
        assert_eq!(hits.first().map(|h| h.id.as_str()), Some("endianness"));
    }

    #[test]
    fn a_concept_phrase_surfaces_a_lesson_without_the_literal_words() {
        let c = corpus();
        let hits = search(&c, "little endian", 10);
        assert!(hits.iter().any(|h| h.id == "endianness"));
    }

    #[test]
    fn an_empty_query_returns_nothing() {
        let c = corpus();
        assert!(search(&c, "   ", 10).is_empty());
    }

    #[test]
    fn a_hit_carries_a_nonempty_snippet_and_respects_the_limit() {
        let c = corpus();
        let hits = search(&c, "stack", 3);
        assert!(hits.len() <= 3);
        assert!(hits.iter().all(|h| !h.snippet.is_empty()));
    }
}
