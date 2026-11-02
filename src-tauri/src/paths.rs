use std::path::{Path, PathBuf};

/// The form of `path` used to identify open files (see
/// [`ferrite_workspace::normalize_path`]).
pub fn normalize(path: &Path) -> PathBuf {
  ferrite_workspace::normalize_path(path)
}
