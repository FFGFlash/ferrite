//! Find in files: searches every non-ignored, non-excluded text file in the
//! workspace, reporting results file by file as they are found.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ferrite_text::{Searcher, TextSnapshot};
use ignore::WalkBuilder;

use crate::filters::{ExcludeFilter, to_slash};

/// Longest preview of a matching line, in characters, around the match.
const PREVIEW_CHARS: usize = 200;
/// Bytes inspected for NUL to detect binary files.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct SearchLimits {
  /// Files larger than this are skipped.
  pub max_file_bytes: u64,
  /// The search stops after this many matches.
  pub max_matches: usize,
  /// Matches per file beyond this are not reported.
  pub max_matches_per_file: usize,
}

impl Default for SearchLimits {
  fn default() -> Self {
    Self {
      max_file_bytes: 4 * 1024 * 1024,
      max_matches: 20_000,
      max_matches_per_file: 1_000,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMatch {
  /// Zero-based line number.
  pub line: usize,
  /// Match start and end on that line, in UTF-16 columns. Matches spanning
  /// lines end at the end of the first line.
  pub start_column: usize,
  pub end_column: usize,
  /// The line's text, shortened around the match if long.
  pub preview: String,
  /// The match within `preview`, in UTF-16 columns.
  pub preview_start: usize,
  pub preview_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatches {
  pub path: PathBuf,
  pub relative: String,
  pub matches: Vec<LineMatch>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchSummary {
  pub files_searched: usize,
  pub files_with_matches: usize,
  pub matches: usize,
  /// The match limit was reached.
  pub truncated: bool,
  pub cancelled: bool,
}

/// Searches the workspace. Files in `open` are searched as they are in the
/// editor (including unsaved changes) instead of on disk. Binary, non-UTF-8,
/// and oversized files are skipped. Stops early when `cancel` is set.
pub fn search_workspace(
  root: &Path,
  exclude: &ExcludeFilter,
  searcher: &Searcher,
  open: &HashMap<PathBuf, TextSnapshot>,
  limits: SearchLimits,
  cancel: &AtomicBool,
  mut on_file: impl FnMut(FileMatches),
) -> SearchSummary {
  let mut summary = SearchSummary::default();
  let filter_root = root.to_path_buf();
  let filter = exclude.clone();
  let walker = WalkBuilder::new(root)
    .hidden(false)
    .filter_entry(move |entry| !filter.is_excluded(&filter_root, entry.path()))
    .build();

  for entry in walker.filter_map(Result::ok) {
    if cancel.load(Ordering::Relaxed) {
      summary.cancelled = true;
      break;
    }
    if !entry.file_type().is_some_and(|kind| kind.is_file()) {
      continue;
    }
    let path = entry.path();
    let Some(text) = read_searchable(path, open, limits.max_file_bytes) else {
      continue;
    };
    summary.files_searched += 1;

    let remaining = limits.max_matches - summary.matches;
    let per_file = limits.max_matches_per_file.min(remaining);
    let ranges: Vec<_> = searcher.find_in_str(&text).take(per_file + 1).collect();
    if ranges.is_empty() {
      continue;
    }
    if ranges.len() > per_file && per_file == remaining {
      summary.truncated = true;
    }
    let matches = locate(&text, &ranges[..ranges.len().min(per_file)]);
    summary.files_with_matches += 1;
    summary.matches += matches.len();
    on_file(FileMatches {
      path: path.to_path_buf(),
      relative: path.strip_prefix(root).map(to_slash).unwrap_or_default(),
      matches,
    });
    if summary.matches >= limits.max_matches {
      summary.truncated = true;
      break;
    }
  }
  summary
}

fn read_searchable(
  path: &Path,
  open: &HashMap<PathBuf, TextSnapshot>,
  max_bytes: u64,
) -> Option<String> {
  if let Some(snapshot) = open.get(path) {
    return Some(snapshot.chunks().collect());
  }
  if fs::metadata(path).ok()?.len() > max_bytes {
    return None;
  }
  let bytes = fs::read(path).ok()?;
  if bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) {
    return None;
  }
  String::from_utf8(bytes).ok()
}

/// Converts byte ranges (sorted) into line/column matches with previews.
/// Lines break at LF, CRLF, and CR, like the editor.
fn locate(text: &str, ranges: &[std::ops::Range<usize>]) -> Vec<LineMatch> {
  let bytes = text.as_bytes();
  let (mut line, mut line_start, mut scanned) = (0, 0, 0);
  ranges
    .iter()
    .map(|range| {
      while scanned < range.start {
        match bytes[scanned] {
          b'\n' => {
            line += 1;
            line_start = scanned + 1;
          }
          b'\r' if bytes.get(scanned + 1) != Some(&b'\n') => {
            line += 1;
            line_start = scanned + 1;
          }
          _ => {}
        }
        scanned += 1;
      }
      let line_end = text[line_start..]
        .find(['\n', '\r'])
        .map_or(text.len(), |end| line_start + end);
      let end = range.end.min(line_end);
      let line_text = &text[line_start..line_end];
      let start_column = utf16_len(&text[line_start..range.start]);
      let end_column = start_column + utf16_len(&text[range.start..end]);
      let (preview, offset) = preview_window(line_text, range.start - line_start);
      let preview_start = start_column - offset;
      LineMatch {
        line,
        start_column,
        end_column,
        preview_end: (preview_start + end_column - start_column).min(utf16_len(&preview)),
        preview,
        preview_start,
      }
    })
    .collect()
}

/// The line, or a window of it around byte `at` if the line is long, plus
/// the UTF-16 length of the text cut from the start.
fn preview_window(line: &str, at: usize) -> (String, usize) {
  if line.chars().count() <= PREVIEW_CHARS {
    return (line.to_owned(), 0);
  }
  let before = line[..at]
    .chars()
    .rev()
    .take(PREVIEW_CHARS / 4)
    .map(char::len_utf8)
    .sum::<usize>();
  let start = at - before;
  let window: String = line[start..].chars().take(PREVIEW_CHARS).collect();
  (window, utf16_len(&line[..start]))
}

fn utf16_len(text: &str) -> usize {
  text.chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
  use ferrite_text::{SearchQuery, TextBuffer};

  use super::*;

  fn searcher(pattern: &str) -> Searcher {
    SearchQuery {
      pattern: pattern.into(),
      ..SearchQuery::default()
    }
    .compile()
    .expect("compiles")
  }

  fn run(
    root: &Path,
    pattern: &str,
    open: &HashMap<PathBuf, TextSnapshot>,
    limits: SearchLimits,
  ) -> (Vec<FileMatches>, SearchSummary) {
    let mut files = Vec::new();
    let exclude = ExcludeFilter::new(&[".git"]).expect("valid");
    let summary = search_workspace(
      root,
      &exclude,
      &searcher(pattern),
      open,
      limits,
      &AtomicBool::new(false),
      |file| files.push(file),
    );
    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    (files, summary)
  }

  #[test]
  fn finds_matches_with_lines_columns_and_previews() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
      dir.path().join("a.rs"),
      "fn main() {\r\n  let 🦀 = needle;\r\n}\n",
    )
    .expect("write");
    fs::write(dir.path().join("b.txt"), "no match here").expect("write");

    let (files, summary) = run(
      dir.path(),
      "needle",
      &HashMap::new(),
      SearchLimits::default(),
    );
    assert_eq!(summary.files_searched, 2);
    assert_eq!(summary.matches, 1);
    assert_eq!(files.len(), 1);
    let found = &files[0].matches[0];
    assert_eq!(files[0].relative, "a.rs");
    assert_eq!(
      (found.line, found.start_column, found.end_column),
      (1, 11, 17)
    );
    assert_eq!(found.preview, "  let 🦀 = needle;");
    assert_eq!((found.preview_start, found.preview_end), (11, 17));
  }

  #[test]
  fn skips_ignored_binary_and_large_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::create_dir(root.join(".git")).expect("mkdir");
    fs::write(root.join(".gitignore"), "ignored.txt\n").expect("write");
    fs::write(root.join("ignored.txt"), "needle").expect("write");
    fs::write(root.join("binary.bin"), b"needle\0").expect("write");
    fs::write(root.join("big.txt"), "needle ".repeat(100)).expect("write");
    fs::write(root.join("kept.txt"), "needle").expect("write");

    let limits = SearchLimits {
      max_file_bytes: 100,
      ..SearchLimits::default()
    };
    let (files, _) = run(root, "needle", &HashMap::new(), limits);
    let names: Vec<_> = files.iter().map(|f| f.relative.as_str()).collect();
    assert_eq!(names, vec!["kept.txt"]);
  }

  #[test]
  fn searches_open_documents_as_edited() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("open.txt");
    fs::write(&path, "old text").expect("write");
    let open = HashMap::from([(path, TextBuffer::from_text("new needle").snapshot())]);
    let (files, _) = run(dir.path(), "needle", &open, SearchLimits::default());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].matches[0].preview, "new needle");
  }

  #[test]
  fn stops_at_the_match_limit() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("many.txt"), "x\n".repeat(50)).expect("write");
    let limits = SearchLimits {
      max_matches: 10,
      ..SearchLimits::default()
    };
    let (files, summary) = run(dir.path(), "x", &HashMap::new(), limits);
    assert_eq!(files[0].matches.len(), 10);
    assert!(summary.truncated);
  }

  #[test]
  fn cancellation_stops_the_search() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a.txt"), "needle").expect("write");
    let summary = search_workspace(
      dir.path(),
      &ExcludeFilter::default(),
      &searcher("needle"),
      &HashMap::new(),
      SearchLimits::default(),
      &AtomicBool::new(true),
      |_| panic!("no results after cancellation"),
    );
    assert!(summary.cancelled);
  }

  #[test]
  fn long_lines_get_a_window_preview() {
    let line = format!("{}needle{}", "a".repeat(500), "b".repeat(500));
    let matches = locate(&line, std::slice::from_ref(&(500..506)));
    let found = &matches[0];
    assert!(found.preview.chars().count() <= PREVIEW_CHARS);
    let units: Vec<u16> = found.preview.encode_utf16().collect();
    assert_eq!(
      String::from_utf16_lossy(&units[found.preview_start..found.preview_end]),
      "needle"
    );
    assert_eq!(found.start_column, 500);
  }
}
