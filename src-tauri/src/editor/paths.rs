use std::path::{Path, PathBuf};

/// The form of `path` used to identify open files: canonical (symlinks and
/// `..` resolved) when the file exists, otherwise absolute. On Windows, the
/// `\\?\` prefix that canonicalization adds to drive paths is removed so paths
/// stay readable.
pub fn normalize(path: &Path) -> PathBuf {
  match std::fs::canonicalize(path) {
    Ok(canonical) => strip_verbatim_prefix(canonical),
    Err(_) => std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()),
  }
}

#[cfg(windows)]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
  match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
    Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
    _ => path,
  }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
  path
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalizes_existing_and_missing_paths_to_absolute_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "").expect("write");

    let via_dot_dot = dir.path().join("sub").join("..").join("a.txt");
    std::fs::create_dir(dir.path().join("sub")).expect("mkdir");
    assert_eq!(normalize(&via_dot_dot), normalize(&file));
    assert!(normalize(&file).is_absolute());
    assert!(!normalize(&file).to_string_lossy().starts_with(r"\\?\"));
    assert!(normalize(Path::new("missing.txt")).is_absolute());
  }
}
