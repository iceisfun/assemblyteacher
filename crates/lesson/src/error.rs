//! Errors from loading and validating lessons.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("{0} is not a directory")]
    NotADirectory(PathBuf),

    #[error("reading {0}: {1}")]
    Io(PathBuf, String),

    #[error("parsing {0}: {1}")]
    Toml(PathBuf, String),

    #[error("lesson.md must begin with `+++` front matter")]
    MissingFrontMatter,

    #[error("{0} must begin with `+++` front matter")]
    MissingFrontMatterIn(PathBuf),

    #[error("front matter is never terminated by a closing `+++`")]
    UnterminatedFrontMatter,
}

/// A problem found by [`crate::validate`]. These are *authoring* errors: they
/// mean a lesson is broken, and the test suite fails on any of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub lesson: String,
    pub exercise: Option<String>,
    pub message: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.exercise {
            Some(e) => write!(f, "lesson `{}`, exercise `{}`: {}", self.lesson, e, self.message),
            None => write!(f, "lesson `{}`: {}", self.lesson, self.message),
        }
    }
}
