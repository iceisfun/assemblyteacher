//! The curriculum is part of the test suite.
//!
//! Every lesson in `lessons/` is loaded, every `examples/*.asm` is assembled,
//! and every exercise's stated reference solution is graded by the same code
//! that grades a student. If a lesson claims `mov rax, 1` assembles to seven
//! particular bytes, and the assembler ever disagrees, this test fails.
//!
//! This is the mechanism that keeps the prose honest as the tools evolve.

use std::path::PathBuf;

fn lessons_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/lesson.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lessons")
}

#[test]
fn the_whole_curriculum_is_valid() {
    let dir = lessons_dir();
    assert!(dir.is_dir(), "lessons/ not found at {}", dir.display());

    let curriculum = lesson::load(&dir).expect("the curriculum loads");
    assert!(!curriculum.is_empty(), "no lessons were found");

    let issues = lesson::validate(&curriculum);
    if !issues.is_empty() {
        let report = issues.iter().map(|i| format!("  - {}", i)).collect::<Vec<_>>().join("\n");
        panic!("{} problem(s) in the curriculum:\n{}", issues.len(), report);
    }

    eprintln!("validated {} lessons across {} parts", curriculum.len(), curriculum.parts.len());
}

/// Lessons are ordered, and the order has to be a total order within a part —
/// two lessons with `order = 1` would render in an arbitrary sequence.
#[test]
fn lesson_order_is_unambiguous() {
    let curriculum = lesson::load(lessons_dir()).expect("loads");
    for part in &curriculum.parts {
        let mut orders: Vec<u32> = part.lessons.iter().map(|l| l.order).collect();
        let before = orders.len();
        orders.sort_unstable();
        orders.dedup();
        assert_eq!(
            orders.len(),
            before,
            "part {} ({}) has two lessons with the same `order`",
            part.number,
            part.title
        );
    }
}

/// A prerequisite must appear *earlier* in the curriculum than the lesson that
/// requires it. A cycle, or a forward reference, means the reading order is
/// impossible.
#[test]
fn prerequisites_always_point_backwards() {
    let curriculum = lesson::load(lessons_dir()).expect("loads");

    let mut position = std::collections::HashMap::new();
    for (i, l) in curriculum.lessons().enumerate() {
        position.insert(l.id.as_str(), i);
    }

    for lesson in curriculum.lessons() {
        let here = position[lesson.id.as_str()];
        for prereq in &lesson.prerequisites {
            let there = position
                .get(prereq.as_str())
                .unwrap_or_else(|| panic!("`{}` requires unknown lesson `{}`", lesson.id, prereq));
            assert!(
                *there < here,
                "`{}` requires `{}`, which comes later in the curriculum",
                lesson.id,
                prereq
            );
        }
    }
}

/// Every lesson must be reachable by a reader who starts at the beginning: the
/// first lesson of the first part has no prerequisites.
#[test]
fn the_curriculum_has_an_entry_point() {
    let curriculum = lesson::load(lessons_dir()).expect("loads");
    let first = curriculum.lessons().next().expect("at least one lesson");
    assert!(
        first.prerequisites.is_empty(),
        "the first lesson `{}` cannot have prerequisites",
        first.id
    );
}
