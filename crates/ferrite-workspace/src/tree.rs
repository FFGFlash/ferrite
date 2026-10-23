//! Lazy directory listing for the file explorer.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::file::FileError;
use crate::filters::ExcludeFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
  File,
  Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
  pub name: String,
  pub path: PathBuf,
  pub kind: EntryKind,
  /// Matched by `.gitignore` (or `.ignore`) rules: shown dimmed, skipped by
  /// the file index and search.
  pub ignored: bool,
}

/// The entries of `dir` (a descendant of `root`), directories first, without
/// excluded entries. Symbolic links are listed as what they point to.
pub fn list_directory(
  root: &Path,
  dir: &Path,
  exclude: &ExcludeFilter,
) -> Result<Vec<DirEntry>, FileError> {
  let read = fs::read_dir(dir).map_err(|error| FileError::from_io(dir, error))?;
  let not_ignored = non_ignored_children(dir);

  let mut entries: Vec<DirEntry> = read
    .filter_map(Result::ok)
    .filter(|entry| !exclude.is_excluded(root, &entry.path()))
    .map(|entry| {
      let path = entry.path();
      // `metadata` follows symlinks; a broken link is listed as a file.
      let is_dir = fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir());
      DirEntry {
        name: entry.file_name().to_string_lossy().into_owned(),
        kind: if is_dir {
          EntryKind::Directory
        } else {
          EntryKind::File
        },
        ignored: !not_ignored.contains(&path),
        path,
      }
    })
    .collect();
  entries.sort_by(|a, b| {
    (b.kind == EntryKind::Directory)
      .cmp(&(a.kind == EntryKind::Directory))
      .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
      .then_with(|| a.name.cmp(&b.name))
  });
  Ok(entries)
}

/// Children of `dir` that ignore rules (`.gitignore` in git repositories,
/// `.ignore`, and the global git excludes) do not exclude.
pub(crate) fn non_ignored_children(dir: &Path) -> HashSet<PathBuf> {
  WalkBuilder::new(dir)
    .max_depth(Some(1))
    .hidden(false)
    .build()
    .filter_map(Result::ok)
    .filter(|entry| entry.depth() == 1)
    .map(ignore::DirEntry::into_path)
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lists_directories_first_and_hides_excluded_entries() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::create_dir(root.join("src")).expect("mkdir");
    fs::create_dir(root.join(".git")).expect("mkdir");
    fs::write(root.join("b.txt"), "").expect("write");
    fs::write(root.join("A.txt"), "").expect("write");

    let exclude = ExcludeFilter::new(&[".git"]).expect("valid");
    let names: Vec<_> = list_directory(root, root, &exclude)
      .expect("lists")
      .into_iter()
      .map(|entry| entry.name)
      .collect();
    assert_eq!(names, vec!["src", "A.txt", "b.txt"]);
  }

  #[test]
  fn marks_gitignored_entries() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::create_dir(root.join(".git")).expect("mkdir");
    fs::write(root.join(".gitignore"), "target/\n*.tmp\n").expect("write");
    fs::create_dir(root.join("target")).expect("mkdir");
    fs::write(root.join("scratch.tmp"), "").expect("write");
    fs::write(root.join("main.rs"), "").expect("write");

    let entries =
      list_directory(root, root, &ExcludeFilter::new(&[".git"]).expect("valid")).expect("lists");
    let ignored: Vec<_> = entries
      .iter()
      .filter(|entry| entry.ignored)
      .map(|entry| entry.name.as_str())
      .collect();
    assert_eq!(ignored, vec!["target", "scratch.tmp"]);
  }

  #[test]
  fn missing_directories_are_errors() {
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("gone");
    assert!(matches!(
      list_directory(dir.path(), &missing, &ExcludeFilter::default()),
      Err(FileError::NotFound { .. })
    ));
  }
}
