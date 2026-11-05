//! Creating files and folders from the explorer.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::entries::EntryError;
use crate::file::FileError;
use crate::tree::EntryKind;

/// What [`create_entry`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
  /// The new file or folder.
  pub path: PathBuf,
  /// The folders from the parent down to the one holding `path`: the
  /// listings that changed.
  pub directories: Vec<PathBuf>,
}

/// Creates an empty file or a folder called `name` in `parent`. Names may
/// contain `/` or `\` to create it in subfolders, which are created as needed
/// (`src/lib.rs`). Existing files and folders are never replaced.
pub fn create_entry(parent: &Path, name: &str, kind: EntryKind) -> Result<Created, EntryError> {
  let Created { path, directories } = prepare(parent, name)?;
  // Windows reports some name clashes (a file over a folder) as access
  // denied, so check first; `create_new` still guards against races.
  if fs::symlink_metadata(&path).is_ok() {
    return Err(FileError::AlreadyExists { path }.into());
  }
  let result = match kind {
    EntryKind::File => fs::OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&path)
      .map(drop),
    EntryKind::Directory => fs::create_dir(&path),
  };
  result.map_err(|error| FileError::from_io(&path, error))?;
  Ok(Created { path, directories })
}

/// Checks `name` (see [`create_entry`]) and creates the subfolders it names
/// in `parent`, returning the path it names. Nothing is created at that path.
pub(crate) fn prepare(parent: &Path, name: &str) -> Result<Created, EntryError> {
  let segments = parse_name(name)?;
  let Some((last, folders)) = segments.split_last() else {
    return Err(EntryError::InvalidName("Enter a name.".into()));
  };
  let metadata = fs::metadata(parent).map_err(|error| FileError::from_io(parent, error))?;
  if !metadata.is_dir() {
    return Err(EntryError::NotADirectory(parent.to_path_buf()));
  }

  let mut directories = vec![parent.to_path_buf()];
  let mut dir = parent.to_path_buf();
  for folder in folders {
    dir.push(folder);
    match fs::create_dir(&dir) {
      Ok(()) => {}
      Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
        if !dir.is_dir() {
          return Err(EntryError::NotADirectory(dir));
        }
      }
      Err(error) => return Err(FileError::from_io(&dir, error).into()),
    }
    directories.push(dir.clone());
  }
  Ok(Created {
    path: dir.join(last),
    directories,
  })
}

/// Splits a name typed in the explorer into path segments, refusing ones
/// that would leave the parent folder or that the platform can't store.
fn parse_name(name: &str) -> Result<Vec<&str>, EntryError> {
  let invalid = |message: String| Err(EntryError::InvalidName(message));
  let segments: Vec<&str> = name
    .trim()
    .split(['/', '\\'])
    .filter(|segment| !segment.is_empty())
    .collect();
  if segments.is_empty() {
    return invalid("Enter a name.".into());
  }
  for segment in &segments {
    if *segment == "." || *segment == ".." {
      return invalid(format!("\"{segment}\" can't be used as a name."));
    }
    if let Some(c) = segment.chars().find(|&c| is_forbidden(c)) {
      return invalid(match c {
        c if c.is_control() => "Names can't contain control characters.".into(),
        c => format!("Names can't contain \"{c}\"."),
      });
    }
    if cfg!(windows) {
      if segment.ends_with(['.', ' ']) {
        return invalid("Names can't end with a period or a space.".into());
      }
      if is_reserved_on_windows(segment) {
        return invalid(format!("\"{segment}\" is reserved by Windows."));
      }
    }
  }
  Ok(segments)
}

fn is_forbidden(c: char) -> bool {
  c.is_control() || (cfg!(windows) && matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
}

/// Device names, which Windows reserves with any extension (`nul.txt`).
fn is_reserved_on_windows(segment: &str) -> bool {
  let stem = segment.split('.').next().unwrap_or(segment).trim_end();
  let upper = stem.to_ascii_uppercase();
  matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
    || ((upper.starts_with("COM") || upper.starts_with("LPT"))
      && upper.len() == 4
      && upper.as_bytes()[3].is_ascii_digit()
      && upper.as_bytes()[3] != b'0')
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_empty_files_and_folders() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = create_entry(dir.path(), "notes.md", EntryKind::File).expect("creates");
    assert_eq!(file.path, dir.path().join("notes.md"));
    assert_eq!(file.directories, vec![dir.path().to_path_buf()]);
    assert_eq!(fs::read(&file.path).expect("reads"), b"");

    let folder = create_entry(dir.path(), "  docs  ", EntryKind::Directory).expect("creates");
    assert!(folder.path.is_dir());
    assert_eq!(folder.path, dir.path().join("docs"));
  }

  #[test]
  fn creates_missing_subfolders_along_the_way() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::create_dir(dir.path().join("src")).expect("mkdir");
    let created = create_entry(dir.path(), "src/net\\http.rs", EntryKind::File).expect("creates");
    let net = dir.path().join("src").join("net");
    assert_eq!(created.path, net.join("http.rs"));
    assert!(created.path.is_file());
    assert_eq!(
      created.directories,
      vec![dir.path().to_path_buf(), dir.path().join("src"), net]
    );
  }

  #[test]
  fn never_replaces_existing_entries() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("a.txt"), "keep").expect("write");
    fs::create_dir(dir.path().join("src")).expect("mkdir");

    for (name, kind) in [
      ("a.txt", EntryKind::File),
      ("a.txt", EntryKind::Directory),
      ("src", EntryKind::File),
      ("src", EntryKind::Directory),
    ] {
      assert!(
        matches!(
          create_entry(dir.path(), name, kind),
          Err(EntryError::File(FileError::AlreadyExists { .. }))
        ),
        "{name} as {kind:?}"
      );
    }
    assert_eq!(
      fs::read_to_string(dir.path().join("a.txt")).expect("reads"),
      "keep"
    );
    assert!(matches!(
      create_entry(dir.path(), "a.txt/b.txt", EntryKind::File),
      Err(EntryError::NotADirectory(_))
    ));
  }

  #[test]
  fn refuses_names_that_leave_the_folder_or_are_empty() {
    let dir = tempfile::tempdir().expect("temp dir");
    let inner = dir.path().join("inner");
    fs::create_dir(&inner).expect("mkdir");
    for name in [
      "",
      "   ",
      "/",
      "..",
      "../escape.txt",
      "a/./b",
      "bad\u{7}name",
    ] {
      assert!(
        matches!(
          create_entry(&inner, name, EntryKind::File),
          Err(EntryError::InvalidName(_))
        ),
        "{name:?}"
      );
    }
    assert!(!dir.path().join("escape.txt").exists());
    // A leading separator stays inside the folder rather than meaning the root.
    let created = create_entry(&inner, "/rooted.txt", EntryKind::File).expect("creates");
    assert_eq!(created.path, inner.join("rooted.txt"));
  }

  #[cfg(windows)]
  #[test]
  fn refuses_names_windows_cannot_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    for name in [
      "a:b",
      "what?",
      "trailing.",
      "trailing /x.txt",
      "CON",
      "nul.txt",
      "Com1.log",
      "C:\\x",
    ] {
      assert!(
        matches!(
          create_entry(dir.path(), name, EntryKind::File),
          Err(EntryError::InvalidName(_))
        ),
        "{name:?}"
      );
    }
    create_entry(dir.path(), "COM0.txt", EntryKind::File).expect("not reserved");
    create_entry(dir.path(), "console.txt", EntryKind::File).expect("not reserved");
  }

  #[test]
  fn reports_a_missing_parent() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(matches!(
      create_entry(&dir.path().join("gone"), "a.txt", EntryKind::File),
      Err(EntryError::File(FileError::NotFound { .. }))
    ));
  }
}
