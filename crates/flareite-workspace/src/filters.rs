use std::path::Path;

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};

/// User-configured paths to hide from the explorer, the file index, and
/// search (for example `.git` or `**/dist/**`).
///
/// Patterns without a `/` match a file or directory name at any depth, like
/// gitignore. Patterns with a `/` match the path relative to the workspace
/// root, with `/` as the separator.
#[derive(Debug, Clone)]
pub struct ExcludeFilter {
  names: GlobSet,
  paths: GlobSet,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid exclude pattern {pattern:?}: {message}")]
pub struct FilterError {
  pub pattern: String,
  pub message: String,
}

impl Default for ExcludeFilter {
  fn default() -> Self {
    Self {
      names: GlobSet::empty(),
      paths: GlobSet::empty(),
    }
  }
}

impl ExcludeFilter {
  pub fn new<S: AsRef<str>>(patterns: &[S]) -> Result<Self, FilterError> {
    let mut names = GlobSetBuilder::new();
    let mut paths = GlobSetBuilder::new();
    for pattern in patterns {
      let pattern = pattern.as_ref().trim_end_matches('/');
      let error = |error: globset::Error| FilterError {
        pattern: pattern.to_owned(),
        message: error.kind().to_string(),
      };
      if pattern.contains('/') {
        let glob = GlobBuilder::new(pattern.trim_start_matches('/'))
          .literal_separator(true)
          .build()
          .map_err(error)?;
        paths.add(glob);
      } else {
        names.add(Glob::new(pattern).map_err(error)?);
      }
    }
    let build = |builder: GlobSetBuilder| {
      builder.build().map_err(|error| FilterError {
        pattern: String::new(),
        message: error.to_string(),
      })
    };
    Ok(Self {
      names: build(names)?,
      paths: build(paths)?,
    })
  }

  /// Whether `path`, a descendant of `root`, is excluded. Only the path's
  /// own name is checked against name patterns; callers that walk a tree
  /// skip excluded directories, so their contents are never reached.
  pub fn is_excluded(&self, root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
      return false;
    };
    if relative
      .file_name()
      .is_some_and(|name| self.names.is_match(name))
    {
      return true;
    }
    !self.paths.is_empty() && self.paths.is_match(to_slash(relative))
  }
}

/// `relative` with `/` separators, for display and pattern matching.
pub fn to_slash(relative: &Path) -> String {
  let mut out = String::new();
  for (index, component) in relative.components().enumerate() {
    if index > 0 {
      out.push('/');
    }
    out.push_str(&component.as_os_str().to_string_lossy());
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn name_patterns_match_at_any_depth() {
    let filter = ExcludeFilter::new(&[".git", "*.log"]).expect("valid");
    let root = Path::new("/w");
    assert!(filter.is_excluded(root, Path::new("/w/.git")));
    assert!(filter.is_excluded(root, Path::new("/w/sub/.git")));
    assert!(filter.is_excluded(root, Path::new("/w/a/b/debug.log")));
    assert!(!filter.is_excluded(root, Path::new("/w/.gitignore")));
  }

  #[test]
  fn path_patterns_match_relative_paths() {
    let filter = ExcludeFilter::new(&["build/out", "**/gen/**"]).expect("valid");
    let root = Path::new("/w");
    assert!(filter.is_excluded(root, Path::new("/w/build/out")));
    assert!(!filter.is_excluded(root, Path::new("/w/other/build/out")));
    assert!(filter.is_excluded(root, Path::new("/w/a/gen/x.rs")));
  }

  #[test]
  fn invalid_patterns_are_reported() {
    let error = ExcludeFilter::new(&["[unclosed"]).expect_err("invalid");
    assert_eq!(error.pattern, "[unclosed");
  }
}
