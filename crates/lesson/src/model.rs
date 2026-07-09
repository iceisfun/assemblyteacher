//! The lesson data model.
//!
//! A lesson is a directory, not a database row. Everything a lesson needs lives
//! inside it, so a lesson can be read, reviewed and moved on its own, and so
//! that `git log` on that directory is the lesson's history.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The whole curriculum: parts, each containing lessons, in order.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Curriculum {
    pub parts: Vec<Part>,
}

impl Curriculum {
    pub fn lessons(&self) -> impl Iterator<Item = &Lesson> {
        self.parts.iter().flat_map(|p| p.lessons.iter())
    }

    pub fn lesson(&self, id: &str) -> Option<&Lesson> {
        self.lessons().find(|l| l.id == id)
    }

    pub fn len(&self) -> usize {
        self.lessons().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One part of the curriculum, e.g. "Part III — Assembly Language".
#[derive(Clone, Debug, Serialize)]
pub struct Part {
    pub number: u32,
    pub title: String,
    pub slug: String,
    #[serde(skip)]
    pub dir: PathBuf,
    pub lessons: Vec<Lesson>,
}

/// `part.toml` at the root of a part directory.
#[derive(Debug, Deserialize)]
pub struct PartMeta {
    pub number: u32,
    pub title: String,
}

/// A single lesson.
#[derive(Clone, Debug, Serialize)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub order: u32,
    pub part: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_minutes: Option<u32>,
    pub objectives: Vec<String>,
    /// Lesson ids that should be read first.
    pub prerequisites: Vec<String>,
    /// The markdown body, with the front matter stripped.
    pub body: String,
    /// Runnable sources under `examples/`.
    pub examples: Vec<Example>,
    pub exercises: Vec<Exercise>,
    #[serde(skip)]
    pub dir: PathBuf,
}

impl Lesson {
    /// The lesson as the client may see it: exercises with their answers
    /// removed. The answer key never leaves the server.
    pub fn public(&self) -> PublicLesson<'_> {
        PublicLesson {
            id: &self.id,
            title: &self.title,
            order: self.order,
            part: self.part,
            estimated_minutes: self.estimated_minutes,
            objectives: &self.objectives,
            prerequisites: &self.prerequisites,
            body: &self.body,
            examples: &self.examples,
            exercises: self.exercises.iter().map(Exercise::public).collect(),
        }
    }

    pub fn exercise(&self, id: &str) -> Option<&Exercise> {
        self.exercises.iter().find(|e| e.id == id)
    }
}

#[derive(Debug, Serialize)]
pub struct PublicLesson<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub order: u32,
    pub part: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_minutes: Option<u32>,
    pub objectives: &'a [String],
    pub prerequisites: &'a [String],
    pub body: &'a str,
    pub examples: &'a [Example],
    pub exercises: Vec<PublicExercise<'a>>,
}

/// A runnable example shipped with a lesson.
#[derive(Clone, Debug, Serialize)]
pub struct Example {
    pub name: String,
    pub language: Language,
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Asm,
    C,
    Rust,
    Other,
}

impl Language {
    pub fn from_extension(ext: &str) -> Language {
        match ext {
            "asm" | "s" | "S" => Language::Asm,
            "c" | "h" => Language::C,
            "rs" => Language::Rust,
            _ => Language::Other,
        }
    }
}

/// An exercise, with its answer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Exercise {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub hints: Vec<String>,
    #[serde(flatten)]
    pub kind: ExerciseKind,
}

impl Exercise {
    pub fn public(&self) -> PublicExercise<'_> {
        PublicExercise {
            id: &self.id,
            prompt: &self.prompt,
            hints: &self.hints,
            kind: self.kind.public(),
        }
    }
}

/// The exercise as the client sees it: no `answer`, no `solution`.
#[derive(Debug, Serialize)]
pub struct PublicExercise<'a> {
    pub id: &'a str,
    pub prompt: &'a str,
    pub hints: &'a [String],
    #[serde(flatten)]
    pub kind: PublicKind<'a>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ExerciseKind {
    /// Multiple choice. `answer` is a 0-based index into `choices`.
    Quiz {
        choices: Vec<String>,
        answer: usize,
        #[serde(default)]
        explanation: String,
    },
    /// Write assembly that encodes to specific bytes.
    ///
    /// Graded by assembling the submission and comparing *bytes*, never text —
    /// so any encoding that produces the right machine code is accepted, and a
    /// student who finds a shorter one is not punished for it.
    Assemble {
        /// Expected machine code as lowercase hex.
        expect_hex: String,
        #[serde(default)]
        starter: String,
        solution: String,
    },
    /// Read machine code and say what it is.
    Disassemble {
        hex: String,
        /// Compared after normalising whitespace and case.
        expect_text: String,
        #[serde(default)]
        solution: String,
    },
    /// Write a program whose *execution* produces a given result.
    Emulate {
        #[serde(default)]
        starter: String,
        /// Register name -> required final value, e.g. `rax = 120`.
        #[serde(default)]
        expect_registers: BTreeMap<String, u64>,
        #[serde(default)]
        expect_stdout: Option<String>,
        #[serde(default = "default_max_steps")]
        max_steps: u64,
        solution: String,
    },
}

fn default_max_steps() -> u64 {
    100_000
}

impl ExerciseKind {
    pub fn name(&self) -> &'static str {
        match self {
            ExerciseKind::Quiz { .. } => "quiz",
            ExerciseKind::Assemble { .. } => "assemble",
            ExerciseKind::Disassemble { .. } => "disassemble",
            ExerciseKind::Emulate { .. } => "emulate",
        }
    }

    /// The reference answer, used by `validate` to prove the exercise is
    /// solvable and that its expected output is actually what the tools produce.
    pub fn solution(&self) -> String {
        match self {
            ExerciseKind::Quiz { answer, .. } => answer.to_string(),
            ExerciseKind::Assemble { solution, .. } => solution.clone(),
            ExerciseKind::Disassemble { expect_text, solution, .. } => {
                if solution.is_empty() {
                    expect_text.clone()
                } else {
                    solution.clone()
                }
            }
            ExerciseKind::Emulate { solution, .. } => solution.clone(),
        }
    }

    fn public(&self) -> PublicKind<'_> {
        match self {
            ExerciseKind::Quiz { choices, .. } => PublicKind::Quiz { choices },
            ExerciseKind::Assemble { starter, .. } => PublicKind::Assemble { starter },
            ExerciseKind::Disassemble { hex, .. } => PublicKind::Disassemble { hex },
            ExerciseKind::Emulate { starter, .. } => PublicKind::Emulate { starter },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PublicKind<'a> {
    Quiz { choices: &'a [String] },
    Assemble { starter: &'a str },
    Disassemble { hex: &'a str },
    Emulate { starter: &'a str },
}

/// The result of grading a submission.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Verdict {
    pub correct: bool,
    /// Shown to the student either way. On failure it should say *what* was
    /// wrong, not merely that something was.
    pub message: String,
    /// Progressive hints, revealed by the UI on repeated failure.
    pub hints: Vec<String>,
}

impl Verdict {
    pub fn correct(message: impl Into<String>) -> Verdict {
        Verdict { correct: true, message: message.into(), hints: Vec::new() }
    }

    pub fn wrong(message: impl Into<String>, hints: &[String]) -> Verdict {
        Verdict { correct: false, message: message.into(), hints: hints.to_vec() }
    }
}

/// The front matter of `lesson.md`.
#[derive(Debug, Deserialize)]
pub struct LessonMeta {
    pub id: String,
    pub title: String,
    pub order: u32,
    #[serde(default)]
    pub estimated_minutes: Option<u32>,
    #[serde(default)]
    pub objectives: Vec<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub exercises: Vec<Exercise>,
}
