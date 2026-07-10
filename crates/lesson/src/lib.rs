//! # lesson
//!
//! The lesson framework: load lessons from disk, serve them, grade submissions,
//! and — most importantly — prove at build time that every lesson is correct.
//!
//! ## Why a lesson is a directory
//!
//! Each lesson owns everything it needs: its prose, its runnable examples, its
//! exercises, its answers, its assets. Nothing about a lesson lives in a global
//! registry or a database. Adding one means adding a directory; reordering the
//! curriculum means editing one number. A lesson can be read on GitHub, in a
//! terminal, or in the browser, and it says the same thing in all three.
//!
//! ## Why the tests run the lessons
//!
//! Educational material rots. An assembler grows a short-form encoding, and
//! suddenly the bytes printed in chapter three are wrong; nobody notices,
//! because prose does not have a test suite.
//!
//! Here it does. [`validate`] takes every exercise's stated reference solution
//! and pushes it through the *same grader* a student's submission goes through.
//! It assembles every `examples/*.asm`. If chapter three claims `mov rax, 1` is
//! seven bytes and the assembler disagrees, `cargo test` fails.
//!
//! ```no_run
//! let curriculum = lesson::load("lessons")?;
//! let issues = lesson::validate(&curriculum);
//! assert!(issues.is_empty(), "{:#?}", issues);
//! # Ok::<(), lesson::LoadError>(())
//! ```

#![forbid(unsafe_code)]

pub mod check;
pub mod error;
pub mod load;
pub mod model;
pub mod search;
pub mod validate;

pub use check::{check, from_hex, to_hex};
pub use error::{Issue, LoadError};
pub use load::{load, split_front_matter};
pub use model::{
    Curriculum, Example, Exercise, ExerciseKind, Language, Lesson, Part, PublicExercise,
    PublicLesson, Verdict,
};
pub use search::{search, SearchHit};
pub use validate::validate;
