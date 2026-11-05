use std::path::{Path, PathBuf};

/// The form of `path` used to identify open files (see
/// [`flareite_workspace::normalize_path`]).
pub fn normalize(path: &Path) -> PathBuf {
  flareite_workspace::normalize_path(path)
}
