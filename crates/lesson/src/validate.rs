//! Authoring-time validation.
//!
//! This is the mechanism that keeps documentation honest. Every exercise ships
//! with its own reference solution, and [`validate`] *runs* that solution
//! through the same grader a student's answer goes through. An exercise whose
//! stated answer does not actually pass is a broken exercise, and the test
//! suite fails on it.
//!
//! The same applies to `examples/`: every `.asm` file in every lesson must
//! assemble. A code block that has drifted out of sync with the assembler is
//! caught by `cargo test`, not by a confused student.

use crate::check::{check, from_hex};
use crate::error::Issue;
use crate::model::{Curriculum, ExerciseKind, Language, Lesson};
use asm_core::{assemble, Decoder};
use std::collections::HashSet;

/// Check every lesson. An empty result means the curriculum is well-formed.
pub fn validate(curriculum: &Curriculum) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut seen_ids: HashSet<&str> = HashSet::new();
    let all_ids: HashSet<&str> = curriculum.lessons().map(|l| l.id.as_str()).collect();

    for lesson in curriculum.lessons() {
        let issue = |message: String| Issue { lesson: lesson.id.clone(), exercise: None, message };

        if lesson.id.is_empty() {
            issues.push(issue("id is empty".into()));
        }
        if !seen_ids.insert(&lesson.id) {
            issues.push(issue(format!("duplicate lesson id `{}`", lesson.id)));
        }
        if lesson.title.trim().is_empty() {
            issues.push(issue("title is empty".into()));
        }
        if lesson.objectives.is_empty() {
            issues.push(issue("no learning objectives".into()));
        }
        if lesson.body.trim().is_empty() {
            issues.push(issue("body is empty".into()));
        }
        if !lesson.dir.join("README.md").exists() {
            issues.push(issue("no README.md — every lesson must stand alone".into()));
        }

        for prereq in &lesson.prerequisites {
            if !all_ids.contains(prereq.as_str()) {
                issues.push(issue(format!("prerequisite `{}` is not a lesson", prereq)));
            }
            if prereq == &lesson.id {
                issues.push(issue("lesson lists itself as a prerequisite".into()));
            }
        }

        validate_examples(lesson, &mut issues);
        validate_exercises(lesson, &mut issues);
    }

    issues
}

fn validate_examples(lesson: &Lesson, issues: &mut Vec<Issue>) {
    for example in &lesson.examples {
        if example.language != Language::Asm {
            continue;
        }
        if let Err(e) = assemble(&example.source) {
            issues.push(Issue {
                lesson: lesson.id.clone(),
                exercise: None,
                message: format!("examples/{} does not assemble: {}", example.name, e),
            });
        }
    }
}

fn validate_exercises(lesson: &Lesson, issues: &mut Vec<Issue>) {
    let mut seen: HashSet<&str> = HashSet::new();

    for exercise in &lesson.exercises {
        let issue = |message: String| Issue {
            lesson: lesson.id.clone(),
            exercise: Some(exercise.id.clone()),
            message,
        };

        if !seen.insert(&exercise.id) {
            issues.push(issue("duplicate exercise id".into()));
        }
        if exercise.prompt.trim().is_empty() {
            issues.push(issue("empty prompt".into()));
        }

        // Kind-specific well-formedness.
        match &exercise.kind {
            ExerciseKind::Quiz { choices, answer, .. } => {
                if choices.len() < 2 {
                    issues.push(issue("a quiz needs at least two choices".into()));
                }
                if *answer >= choices.len() {
                    issues.push(issue(format!(
                        "answer index {} is out of range for {} choices",
                        answer,
                        choices.len()
                    )));
                    continue; // check() below would be meaningless
                }
            }
            ExerciseKind::Assemble { expect_hex, solution, .. } => {
                if from_hex(expect_hex).is_none() {
                    issues.push(issue(format!("expect_hex `{}` is not valid hex", expect_hex)));
                    continue;
                }
                if solution.trim().is_empty() {
                    issues.push(issue("no reference solution".into()));
                    continue;
                }
            }
            ExerciseKind::Disassemble { hex, .. } => {
                let Some(bytes) = from_hex(hex) else {
                    issues.push(issue(format!("hex `{}` is not valid hex", hex)));
                    continue;
                };
                if bytes.is_empty() {
                    issues.push(issue("hex is empty".into()));
                    continue;
                }
                for insn in Decoder::new(&bytes, 0) {
                    if let Err(e) = insn {
                        issues.push(issue(format!("hex does not disassemble: {}", e)));
                        break;
                    }
                }
            }
            ExerciseKind::Emulate { solution, expect_registers, expect_stdout, .. } => {
                if solution.trim().is_empty() {
                    issues.push(issue("no reference solution".into()));
                    continue;
                }
                if expect_registers.is_empty() && expect_stdout.is_none() {
                    issues.push(issue("nothing is asserted about the result".into()));
                }
            }
        }

        // The load-bearing check: does the stated answer actually pass?
        let verdict = check(exercise, &exercise.kind.solution());
        if !verdict.correct {
            issues.push(issue(format!(
                "the reference solution does not pass its own exercise: {}",
                verdict.message
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use std::path::PathBuf;

    fn lesson_with(exercises: Vec<Exercise>) -> Lesson {
        Lesson {
            id: "t".into(),
            title: "T".into(),
            order: 1,
            part: 1,
            estimated_minutes: None,
            objectives: vec!["o".into()],
            prerequisites: vec![],
            body: "body".into(),
            examples: vec![],
            exercises,
            // A path that does not exist, so the README check fires; the tests
            // below filter to the exercise issues they care about.
            dir: PathBuf::from("/nonexistent"),
        }
    }

    fn exercise_issues(lesson: Lesson) -> Vec<String> {
        let c = Curriculum {
            parts: vec![Part {
                number: 1,
                title: "P".into(),
                slug: "p".into(),
                dir: PathBuf::new(),
                lessons: vec![lesson],
            }],
        };
        validate(&c).into_iter().filter(|i| i.exercise.is_some()).map(|i| i.message).collect()
    }

    #[test]
    fn a_wrong_reference_solution_is_caught() {
        // The solution assembles to 31 c0, but the exercise claims c3.
        let bad = Exercise {
            id: "e".into(),
            prompt: "p".into(),
            hints: vec![],
            kind: ExerciseKind::Assemble {
                expect_hex: "c3".into(),
                starter: String::new(),
                solution: "xor eax, eax".into(),
            },
        };
        let issues = exercise_issues(lesson_with(vec![bad]));
        assert!(
            issues.iter().any(|m| m.contains("does not pass its own exercise")),
            "{:?}",
            issues
        );
    }

    #[test]
    fn a_correct_reference_solution_passes() {
        let good = Exercise {
            id: "e".into(),
            prompt: "p".into(),
            hints: vec![],
            kind: ExerciseKind::Assemble {
                expect_hex: "31c0".into(),
                starter: String::new(),
                solution: "xor eax, eax".into(),
            },
        };
        assert!(exercise_issues(lesson_with(vec![good])).is_empty());
    }

    #[test]
    fn an_out_of_range_quiz_answer_is_caught() {
        let bad = Exercise {
            id: "e".into(),
            prompt: "p".into(),
            hints: vec![],
            kind: ExerciseKind::Quiz {
                choices: vec!["a".into(), "b".into()],
                answer: 5,
                explanation: String::new(),
            },
        };
        let issues = exercise_issues(lesson_with(vec![bad]));
        assert!(issues.iter().any(|m| m.contains("out of range")), "{:?}", issues);
    }

    #[test]
    fn undecodable_disassembly_bytes_are_caught() {
        let bad = Exercise {
            id: "e".into(),
            prompt: "p".into(),
            hints: vec![],
            kind: ExerciseKind::Disassemble {
                hex: "0606".into(), // invalid in 64-bit mode
                expect_text: "?".into(),
                solution: String::new(),
            },
        };
        let issues = exercise_issues(lesson_with(vec![bad]));
        assert!(issues.iter().any(|m| m.contains("does not disassemble")), "{:?}", issues);
    }

    #[test]
    fn duplicate_exercise_ids_are_caught() {
        let e = |id: &str| Exercise {
            id: id.into(),
            prompt: "p".into(),
            hints: vec![],
            kind: ExerciseKind::Quiz {
                choices: vec!["a".into(), "b".into()],
                answer: 0,
                explanation: String::new(),
            },
        };
        let issues = exercise_issues(lesson_with(vec![e("x"), e("x")]));
        assert!(issues.iter().any(|m| m.contains("duplicate")), "{:?}", issues);
    }
}
