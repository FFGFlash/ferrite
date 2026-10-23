//! The file index behind Go to File: relative paths of the workspace's
//! non-ignored files, built in the background and kept up to date
//! incrementally from file-change notifications.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::filters::{ExcludeFilter, to_slash};
use crate::tree::non_ignored_children;

/// Most files indexed. Larger workspaces are indexed partially.
pub const MAX_INDEXED_FILES: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatch {
  /// Path relative to the workspace root, with `/` separators.
  pub relative: String,
  /// UTF-16 indices of the matched characters in `relative`, for highlighting.
  pub indices: Vec<u32>,
}

#[derive(Debug, Default)]
pub struct FileIndex {
  root: PathBuf,
  files: BTreeSet<String>,
  truncated: bool,
}

/// Walks `root` for non-ignored, non-excluded files. Returns early (with what
/// it has) when `cancel` is set.
pub fn scan_files(root: &Path, exclude: &ExcludeFilter, cancel: &AtomicBool) -> FileIndex {
  let mut index = FileIndex {
    root: root.to_path_buf(),
    ..FileIndex::default()
  };
  index.add_tree(root, exclude, cancel);
  index
}

impl FileIndex {
  /// An index of the given relative paths (`/`-separated) without touching
  /// the disk, for tests and benchmarks.
  pub fn from_paths(root: &Path, paths: impl IntoIterator<Item = String>) -> Self {
    let mut index = Self {
      root: root.to_path_buf(),
      ..Self::default()
    };
    for path in paths {
      index.insert(path);
    }
    index
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  pub fn len(&self) -> usize {
    self.files.len()
  }

  pub fn is_empty(&self) -> bool {
    self.files.is_empty()
  }

  /// Whether the file limit was reached, so some files are missing.
  pub fn is_truncated(&self) -> bool {
    self.truncated
  }

  /// Applies file-change notifications for `paths` (absolute): new files and
  /// directories are added, deleted ones removed.
  pub fn update(&mut self, paths: &[PathBuf], exclude: &ExcludeFilter) {
    let never = AtomicBool::new(false);
    for path in paths {
      let Ok(relative) = path.strip_prefix(&self.root) else {
        continue;
      };
      if relative.as_os_str().is_empty() {
        continue;
      }
      let key = to_slash(relative);
      self.remove_tree(&key);
      if !path.exists() || is_excluded_or_ignored(&self.root, path, exclude) {
        continue;
      }
      if path.is_dir() {
        self.add_tree(path, exclude, &never);
      } else {
        self.insert(key);
      }
    }
  }

  /// Fuzzy-matches `query` against the indexed paths. Returns the best
  /// `limit` matches, best first. An empty query returns the first files.
  pub fn query(&self, query: &str, limit: usize) -> Vec<FileMatch> {
    if query.trim().is_empty() {
      return self
        .files
        .iter()
        .take(limit)
        .map(|relative| FileMatch {
          relative: relative.clone(),
          indices: Vec::new(),
        })
        .collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buffer = Vec::new();
    let mut scored: Vec<(u32, &String)> = self
      .files
      .iter()
      .filter_map(|relative| {
        let score = pattern.score(Utf32Str::new(relative, &mut buffer), &mut matcher)?;
        Some((score, relative))
      })
      .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.len().cmp(&b.1.len())));
    scored
      .into_iter()
      .take(limit)
      .map(|(_, relative)| {
        let mut indices = Vec::new();
        pattern.indices(
          Utf32Str::new(relative, &mut buffer),
          &mut matcher,
          &mut indices,
        );
        indices.sort_unstable();
        indices.dedup();
        FileMatch {
          relative: relative.clone(),
          indices: char_to_utf16_indices(relative, &indices),
        }
      })
      .collect()
  }

  fn add_tree(&mut self, dir: &Path, exclude: &ExcludeFilter, cancel: &AtomicBool) {
    let root = self.root.clone();
    let filter = exclude.clone();
    let walker = WalkBuilder::new(dir)
      .hidden(false)
      .filter_entry(move |entry| !filter.is_excluded(&root, entry.path()))
      .build();
    for entry in walker.filter_map(Result::ok) {
      if cancel.load(Ordering::Relaxed) || self.truncated {
        return;
      }
      if entry.file_type().is_some_and(|kind| kind.is_file())
        && let Ok(relative) = entry.path().strip_prefix(&self.root)
      {
        self.insert(to_slash(relative));
      }
    }
  }

  fn insert(&mut self, relative: String) {
    if self.files.len() >= MAX_INDEXED_FILES {
      self.truncated = true;
    } else {
      self.files.insert(relative);
    }
  }

  /// Removes `key` and everything below it.
  fn remove_tree(&mut self, key: &str) {
    self.files.remove(key);
    let prefix = format!("{key}/");
    let below: Vec<String> = self
      .files
      .range(prefix.clone()..)
      .take_while(|path| path.starts_with(&prefix))
      .cloned()
      .collect();
    for path in below {
      self.files.remove(&path);
    }
  }
}

fn is_excluded_or_ignored(root: &Path, path: &Path, exclude: &ExcludeFilter) -> bool {
  let mut current = path;
  while let Some(parent) = current.parent() {
    if exclude.is_excluded(root, current) || !non_ignored_children(parent).contains(current) {
      return true;
    }
    if parent == root {
      return false;
    }
    current = parent;
  }
  false
}

fn char_to_utf16_indices(text: &str, char_indices: &[u32]) -> Vec<u32> {
  let mut utf16 = Vec::with_capacity(char_indices.len());
  let mut wanted = char_indices.iter().peekable();
  let mut offset = 0u32;
  for (index, c) in text.chars().enumerate() {
    if wanted.peek().is_some_and(|&&w| w as usize == index) {
      utf16.push(offset);
      wanted.next();
    }
    offset += c.len_utf16() as u32;
  }
  utf16
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::*;

  fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::create_dir_all(root.join("src/editor")).expect("mkdir");
    fs::create_dir_all(root.join(".git")).expect("mkdir");
    fs::create_dir_all(root.join("target/debug")).expect("mkdir");
    fs::write(root.join(".gitignore"), "target/\n").expect("write");
    fs::write(root.join("src/main.rs"), "").expect("write");
    fs::write(root.join("src/editor/view.rs"), "").expect("write");
    fs::write(root.join("target/debug/out.bin"), "").expect("write");
    fs::write(root.join("README.md"), "").expect("write");
    dir
  }

  fn files(index: &FileIndex) -> Vec<String> {
    index
      .query("", usize::MAX)
      .into_iter()
      .map(|m| m.relative)
      .collect()
  }

  #[test]
  fn indexes_non_ignored_files() {
    let dir = workspace();
    let exclude = ExcludeFilter::new(&[".git"]).expect("valid");
    let index = scan_files(dir.path(), &exclude, &AtomicBool::new(false));
    assert_eq!(
      files(&index),
      vec![
        ".gitignore",
        "README.md",
        "src/editor/view.rs",
        "src/main.rs"
      ]
    );
  }

  #[test]
  fn fuzzy_queries_rank_and_highlight_matches() {
    let dir = workspace();
    let exclude = ExcludeFilter::new(&[".git"]).expect("valid");
    let index = scan_files(dir.path(), &exclude, &AtomicBool::new(false));
    let results = index.query("edview", 10);
    assert_eq!(
      results.first().map(|m| m.relative.as_str()),
      Some("src/editor/view.rs")
    );
    assert!(!results[0].indices.is_empty());
    assert!(index.query("zzzz", 10).is_empty());
  }

  #[test]
  fn updates_follow_file_changes() {
    let dir = workspace();
    let root = dir.path();
    let exclude = ExcludeFilter::new(&[".git"]).expect("valid");
    let mut index = scan_files(root, &exclude, &AtomicBool::new(false));

    fs::create_dir_all(root.join("docs")).expect("mkdir");
    fs::write(root.join("docs/guide.md"), "").expect("write");
    fs::write(root.join("target/new.bin"), "").expect("write");
    fs::remove_dir_all(root.join("src/editor")).expect("rm");
    index.update(
      &[
        root.join("docs"),
        root.join("target/new.bin"),
        root.join("src/editor"),
      ],
      &exclude,
    );
    assert_eq!(
      files(&index),
      vec![".gitignore", "README.md", "docs/guide.md", "src/main.rs"]
    );
  }

  #[test]
  fn highlight_indices_are_utf16_offsets() {
    assert_eq!(char_to_utf16_indices("🦀ab", &[1, 2]), vec![2, 3]);
  }
}
