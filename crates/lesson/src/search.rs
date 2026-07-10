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

        // Score each field. A whole-word match is strong and scales with
        // frequency; a mid-word substring is a weak fallback, and only for
        // queries of 4+ characters — at three characters `rop` inside `drop` is
        // noise, but at five `egist` inside `register` is a fair partial.
        let mut cands: Vec<(i32, &'static str)> = Vec::new();

        if title == q {
            cands.push((100, "title"));
        } else if let Some(s) = phrase_score(&title, &q, 82, 60, 3) {
            cands.push((s, "title"));
        }

        let id_spaced = id.replace('-', " ");
        if id != q {
            if let Some(s) = phrase_score(&id_spaced, &q, 66, 60, 3) {
                cands.push((s, "id"));
            }
        }

        let objectives = lesson.objectives.join("\n").to_lowercase();
        if let Some(s) = phrase_score(&objectives, &q, 58, 22, 4) {
            cands.push((s, "objective"));
        }

        // Headings weigh more than body text; more mentions rank higher.
        let mut head_hits = 0usize;
        let mut body_hits = 0usize;
        for line in lesson.body.lines() {
            let trimmed = line.trim();
            let n = word_hits(&trimmed.to_lowercase(), &q);
            if n == 0 {
                continue;
            }
            if trimmed.starts_with('#') {
                head_hits += n;
            } else {
                body_hits += n;
            }
        }
        if head_hits > 0 {
            cands.push((50 + freq_bonus(head_hits), "heading"));
        }
        if body_hits > 0 {
            cands.push((38 + freq_bonus(body_hits), "body"));
        }
        // Substring fallback in the body, only when nothing matched as a word.
        if head_hits == 0 && body_hits == 0 && q.len() >= 4 && lesson.body.to_lowercase().contains(&q) {
            cands.push((16, "body"));
        }

        let prompts = lesson
            .exercises
            .iter()
            .map(|e| e.prompt.to_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(s) = phrase_score(&prompts, &q, 28, 12, 4) {
            cands.push((s, "exercise"));
        }

        // Concept/alias boost.
        for (phrase, ids) in CONCEPTS {
            if q.contains(phrase) && ids.contains(&id.as_str()) {
                cands.push((64, "concept"));
            }
        }

        let mut best = cands.into_iter().max_by_key(|(s, _)| *s);

        // Multi-word queries are OR, not AND: a lesson that matches *some* of the
        // words still surfaces (ranked by how many), so "eax foo bar" still finds
        // eax's lessons rather than returning nothing.
        let mut focus: Option<String> = None;
        if words.len() > 1 {
            let hay = format!("{title}\n{objectives}\n{}", lesson.body.to_lowercase());
            let covered: Vec<&&str> = words.iter().filter(|w| word_hits(&hay, w) > 0).collect();
            if let Some(first) = covered.first() {
                focus = Some((**first).to_string());
                // Reward covering more distinct words, and mentioning them more
                // often, so an eax-heavy lesson outranks a one-line mention.
                let total: usize = covered.iter().map(|w| word_hits(&hay, w)).sum();
                let cov = covered.len() as i32 * 6 + freq_bonus(total);
                best = Some(match best {
                    Some((s, f)) => (s + cov, f),
                    None => (cov, "text"),
                });
            }
        }

        if let Some((score, field)) = best {
            let term = focus.as_deref().filter(|_| field == "text").unwrap_or(&q);
            hits.push(SearchHit {
                id: lesson.id.clone(),
                title: lesson.title.clone(),
                part: lesson.part,
                field,
                snippet: make_snippet(&lesson.body, term),
                score,
            });
        }
    }

    // Highest score first; ties broken by title for a stable order.
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit);
    hits
}

/// Count how many times `q` starts a word in `text` (a lowercased haystack).
/// "Starts a word" = the preceding byte is not alphanumeric, which lets `(ROP)`
/// and `big-endian` match while `appropriate` and `Europe` do not. This is what
/// keeps a short query from drowning in mid-word coincidences.
fn word_hits(text: &str, q: &str) -> usize {
    if q.is_empty() {
        return 0;
    }
    let bytes = text.as_bytes();
    let mut count = 0;
    let mut from = 0;
    while let Some(rel) = text[from..].find(q) {
        let at = from + rel;
        if at == 0 || !bytes[at - 1].is_ascii_alphanumeric() {
            count += 1;
        }
        from = at + q.len();
    }
    count
}

/// A diminishing bonus for repeated mentions: 1 hit adds nothing, many hits add
/// up to +14, so a lesson that is *about* a term beats one that name-drops it.
fn freq_bonus(hits: usize) -> i32 {
    (hits.min(8) as i32 - 1).max(0) * 2
}

/// Score `q` against one field: a whole-word match earns `strong` (plus a
/// frequency bonus); a mid-word substring earns the weaker `weak`, but only once
/// the query is at least `min_sub` characters, below which substrings are noise.
fn phrase_score(text: &str, q: &str, strong: i32, weak: i32, min_sub: usize) -> Option<i32> {
    let w = word_hits(text, q);
    if w > 0 {
        Some(strong + freq_bonus(w))
    } else if q.len() >= min_sub && text.contains(q) {
        Some(weak)
    } else {
        None
    }
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
        if word_hits(&trimmed.to_lowercase(), q) > 0 {
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
    fn a_short_query_matches_whole_words_not_mid_word_coincidences() {
        // "rop" must find the lesson that says ROP 13 times, and must NOT match
        // "drop"/"appropriate"/"Europe" in unrelated lessons.
        let c = corpus();
        let hits = search(&c, "ROP", 8);
        let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert!(ids.contains(&"exploit-mitigations"), "ROP should find exploit-mitigations: {ids:?}");
        assert!(
            !ids.contains(&"calling-conventions"),
            "the only `rop` in calling-conventions is `drop`; it must not match: {ids:?}"
        );
    }

    #[test]
    fn frequent_mentions_outrank_a_single_name_drop() {
        let c = corpus();
        let hits = search(&c, "gadget", 8);
        // The ROP and mitigations lessons are about gadgets; a passing mention
        // elsewhere must not outrank them.
        let top = hits.first().map(|h| h.id.as_str());
        assert!(matches!(top, Some("return-oriented-programming" | "exploit-mitigations")), "{top:?}");
    }

    #[test]
    fn a_longer_query_matches_a_mid_word_substring_but_ranks_it_below_real_hits() {
        let c = corpus();
        // "egist" appears only inside "register" — a weak partial, but better
        // than returning nothing.
        let hits = search(&c, "egist", 8);
        assert!(hits.iter().any(|h| h.id == "registers"), "egist should find registers: {hits:#?}");
    }

    #[test]
    fn a_multi_word_query_is_or_not_and() {
        let c = corpus();
        // "eax foo bar" — only "eax" matches anything, but the query must still
        // surface eax's lessons rather than returning nothing.
        let hits = search(&c, "eax foo bar", 25);
        assert!(!hits.is_empty(), "partial multi-word query should still return hits");
        assert!(hits.iter().any(|h| h.id == "registers"));
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
