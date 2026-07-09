//! Loading lessons from disk.
//!
//! Layout:
//!
//! ```text
//! lessons/
//!   03-assembly-language/
//!     part.toml                  number = 3, title = "Assembly Language"
//!     01-first-instructions/
//!       lesson.md                TOML front matter between +++ fences, then markdown
//!       README.md                what this lesson is, for someone browsing the repo
//!       examples/hello.asm       assembled and run by the test suite
//!       solutions/              reference answers, never served
//!       assets/                 images referenced by lesson.md
//!       tests/                  extra fixtures
//! ```
//!
//! Ordering comes from the `order` field, not from the directory name; the
//! numeric prefixes exist so that `ls` agrees with the curriculum.

use crate::error::LoadError;
use crate::model::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Split `+++\n<toml>\n+++\n<body>` into its two halves.
///
/// Returning an error rather than treating a missing fence as "no front matter"
/// is deliberate: a lesson without metadata is a mistake, not a lesson.
pub fn split_front_matter(text: &str) -> Result<(&str, &str), LoadError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text
        .strip_prefix("+++\n")
        .or_else(|| text.strip_prefix("+++\r\n"))
        .ok_or(LoadError::MissingFrontMatter)?;
    let end = rest.find("\n+++").ok_or(LoadError::UnterminatedFrontMatter)?;
    let meta = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    Ok((meta, body))
}

/// Load the whole curriculum from a directory of parts.
pub fn load(root: impl AsRef<Path>) -> Result<Curriculum, LoadError> {
    let root = root.as_ref();
    if !root.is_dir() {
        return Err(LoadError::NotADirectory(root.to_path_buf()));
    }

    let mut parts = Vec::new();
    for entry in sorted_dirs(root)? {
        let part_toml = entry.join("part.toml");
        if !part_toml.exists() {
            // A directory without part.toml is not a part. Ignore it rather
            // than failing, so that `lessons/README.md` and scratch dirs are OK.
            continue;
        }
        parts.push(load_part(&entry)?);
    }
    parts.sort_by_key(|p| p.number);
    Ok(Curriculum { parts })
}

fn load_part(dir: &Path) -> Result<Part, LoadError> {
    let text = fs::read_to_string(dir.join("part.toml"))
        .map_err(|e| LoadError::Io(dir.join("part.toml"), e.to_string()))?;
    let meta: PartMeta =
        toml::from_str(&text).map_err(|e| LoadError::Toml(dir.join("part.toml"), e.to_string()))?;

    let mut lessons = Vec::new();
    for entry in sorted_dirs(dir)? {
        if entry.join("lesson.md").exists() {
            lessons.push(load_lesson(&entry, meta.number)?);
        }
    }
    lessons.sort_by_key(|l| l.order);

    Ok(Part {
        number: meta.number,
        title: meta.title,
        slug: dir.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        dir: dir.to_path_buf(),
        lessons,
    })
}

fn load_lesson(dir: &Path, part: u32) -> Result<Lesson, LoadError> {
    let path = dir.join("lesson.md");
    let text = fs::read_to_string(&path).map_err(|e| LoadError::Io(path.clone(), e.to_string()))?;
    let (front, body) = split_front_matter(&text).map_err(|e| match e {
        LoadError::MissingFrontMatter => LoadError::MissingFrontMatterIn(path.clone()),
        other => other,
    })?;

    let meta: LessonMeta =
        toml::from_str(front).map_err(|e| LoadError::Toml(path.clone(), e.to_string()))?;

    Ok(Lesson {
        id: meta.id,
        title: meta.title,
        order: meta.order,
        part,
        estimated_minutes: meta.estimated_minutes,
        objectives: meta.objectives,
        prerequisites: meta.prerequisites,
        body: body.to_string(),
        examples: load_examples(&dir.join("examples"))?,
        exercises: meta.exercises,
        dir: dir.to_path_buf(),
    })
}

fn load_examples(dir: &Path) -> Result<Vec<Example>, LoadError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| LoadError::Io(dir.to_path_buf(), e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    entries.sort();

    for path in entries {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let source =
            fs::read_to_string(&path).map_err(|e| LoadError::Io(path.clone(), e.to_string()))?;
        out.push(Example {
            name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            language: Language::from_extension(ext),
            source,
        });
    }
    Ok(out)
}

fn sorted_dirs(root: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .map_err(|e| LoadError::Io(root.to_path_buf(), e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_splits_at_the_fences() {
        let (front, body) = split_front_matter("+++\nid = \"x\"\n+++\n# Title\n").unwrap();
        assert_eq!(front, "id = \"x\"");
        assert_eq!(body, "# Title\n");
    }

    #[test]
    fn a_body_containing_plus_fences_is_not_truncated_early() {
        let text = "+++\nid = \"x\"\n+++\ntext\n\n```\n+++\n```\n";
        let (_, body) = split_front_matter(text).unwrap();
        assert!(body.contains("```"));
    }

    #[test]
    fn missing_front_matter_is_an_error_not_a_default() {
        assert!(matches!(
            split_front_matter("# Just markdown"),
            Err(LoadError::MissingFrontMatter)
        ));
    }

    #[test]
    fn unterminated_front_matter_is_an_error() {
        assert!(matches!(
            split_front_matter("+++\nid = \"x\"\n"),
            Err(LoadError::UnterminatedFrontMatter)
        ));
    }
}
