//! Renaming, moving, copying, and deleting files and folders from the
//! explorer. Nothing here replaces an existing file or folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::create::{Created, prepare};
use crate::file::FileError;

#[derive(Debug, thiserror::Error)]
pub enum EntryError {
  #[error("{0}")]
  InvalidName(String),
  #[error("{} is not a folder", .0.display())]
  NotADirectory(PathBuf),
  #[error("{} can't be moved or copied into itself", .0.display())]
  IntoItself(PathBuf),
  #[error("{} couldn't be moved to the trash: {reason}", .path.display())]
  Trash { path: PathBuf, reason: String },
  #[error(transparent)]
  File(#[from] FileError),
}

/// Renames `path` to `name`, which may name subfolders of its folder (created
/// as needed), as in [`crate::create_entry`]. Changing only the case of a
/// name works on file systems that ignore case. Returns the new path and the
/// folders whose listings changed.
pub fn rename_entry(path: &Path, name: &str) -> Result<Created, EntryError> {
  let parent = path
    .parent()
    .ok_or_else(|| EntryError::InvalidName("This can't be renamed.".into()))?;
  exists(path)?;
  let created = prepare(parent, name)?;
  if created.path.starts_with(path) && created.path != path {
    return Err(EntryError::IntoItself(path.to_path_buf()));
  }
  // The same file under another case (`readme.md` to `README.md`) where
  // case is ignored: renaming it is what's wanted.
  if fs::symlink_metadata(&created.path).is_ok() && !same_entry(path, &created.path) {
    return Err(FileError::AlreadyExists { path: created.path }.into());
  }
  fs::rename(path, &created.path).map_err(|error| FileError::from_io(path, error))?;
  Ok(created)
}

/// Moves `path` into the folder `into`, keeping its name. Moving it to where
/// it is does nothing. Returns the new path.
pub fn move_entry(path: &Path, into: &Path) -> Result<PathBuf, EntryError> {
  let (name, target) = target_in(path, into)?;
  if target == path {
    return Ok(target);
  }
  if into.starts_with(path) {
    return Err(EntryError::IntoItself(path.to_path_buf()));
  }
  if fs::symlink_metadata(&target).is_ok() {
    return Err(FileError::AlreadyExists { path: target }.into());
  }
  match fs::rename(path, &target) {
    Ok(()) => {}
    // Another drive: copy, then delete the original.
    Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
      tracing::debug!(name = %name, "moving across devices by copying");
      copy_tree(path, &target).inspect_err(|_| remove_partial(&target))?;
      remove(path)?;
    }
    Err(error) => return Err(FileError::from_io(path, error).into()),
  }
  Ok(target)
}

/// Copies `path` (a file, or a folder and everything in it) into the folder
/// `into`. If the name is taken there, as when copying beside the original,
/// the copy is named `name copy.ext`, `name copy 2.ext`, and so on. Returns
/// the copy's path.
pub fn copy_entry(path: &Path, into: &Path) -> Result<PathBuf, EntryError> {
  exists(path)?;
  if path.is_dir() && into.starts_with(path) {
    return Err(EntryError::IntoItself(path.to_path_buf()));
  }
  let target = free_name(path, into)?;
  copy_tree(path, &target).inspect_err(|_| remove_partial(&target))?;
  Ok(target)
}

/// Deletes `path` (and everything in it): to the system's trash (the
/// Recycle Bin on Windows), or permanently.
pub fn delete_entry(path: &Path, to_trash: bool) -> Result<(), EntryError> {
  exists(path)?;
  if to_trash {
    trash::delete(path).map_err(|error| EntryError::Trash {
      path: path.to_path_buf(),
      reason: error.to_string(),
    })
  } else {
    remove(path)
  }
}

fn exists(path: &Path) -> Result<(), EntryError> {
  fs::symlink_metadata(path)
    .map(drop)
    .map_err(|error| FileError::from_io(path, error).into())
}

/// The name of `path` and where it goes in `into`, which must be a folder.
fn target_in(path: &Path, into: &Path) -> Result<(String, PathBuf), EntryError> {
  exists(path)?;
  let name = path
    .file_name()
    .ok_or_else(|| EntryError::InvalidName("This can't be moved.".into()))?;
  let metadata = fs::metadata(into).map_err(|error| FileError::from_io(into, error))?;
  if !metadata.is_dir() {
    return Err(EntryError::NotADirectory(into.to_path_buf()));
  }
  Ok((name.to_string_lossy().into_owned(), into.join(name)))
}

/// Whether two existing paths are the same file or folder.
fn same_entry(a: &Path, b: &Path) -> bool {
  match (fs::canonicalize(a), fs::canonicalize(b)) {
    (Ok(a), Ok(b)) => a == b,
    _ => false,
  }
}

/// Where a copy of `path` goes in `into`: its own name if that's free, or
/// the first free `name copy.ext`, `name copy 2.ext`, ….
fn free_name(path: &Path, into: &Path) -> Result<PathBuf, EntryError> {
  let (name, target) = target_in(path, into)?;
  if fs::symlink_metadata(&target).is_err() {
    return Ok(target);
  }
  // Folders and names like `.gitignore` keep their whole name before `copy`.
  let (stem, extension) = match name.rfind('.') {
    Some(dot) if dot > 0 && !path.is_dir() => name.split_at(dot),
    _ => (name.as_str(), ""),
  };
  (1..)
    .map(|n| match n {
      1 => format!("{stem} copy{extension}"),
      n => format!("{stem} copy {n}{extension}"),
    })
    .map(|candidate| into.join(candidate))
    .find(|candidate| fs::symlink_metadata(candidate).is_err())
    .ok_or_else(|| FileError::AlreadyExists { path: target }.into())
}

/// Copies a file, or a folder and everything in it. Links are copied as
/// what they point to.
fn copy_tree(from: &Path, to: &Path) -> Result<(), EntryError> {
  let metadata = fs::metadata(from).map_err(|error| FileError::from_io(from, error))?;
  if !metadata.is_dir() {
    fs::copy(from, to).map_err(|error| FileError::from_io(from, error))?;
    return Ok(());
  }
  fs::create_dir(to).map_err(|error| FileError::from_io(to, error))?;
  for entry in fs::read_dir(from).map_err(|error| FileError::from_io(from, error))? {
    let entry = entry.map_err(|error| FileError::from_io(from, error))?;
    copy_tree(&entry.path(), &to.join(entry.file_name()))?;
  }
  Ok(())
}

/// Removes what a failed copy left behind.
fn remove_partial(target: &Path) {
  if let Err(error) = remove(target)
    && !matches!(error, EntryError::File(FileError::NotFound { .. }))
  {
    tracing::warn!(%error, "could not remove a partial copy");
  }
}

fn remove(path: &Path) -> Result<(), EntryError> {
  let metadata = fs::symlink_metadata(path).map_err(|error| FileError::from_io(path, error))?;
  let result = if metadata.is_dir() {
    fs::remove_dir_all(path)
  } else {
    fs::remove_file(path)
  };
  result.map_err(|error| FileError::from_io(path, error).into())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
      .expect("lists")
      .map(|entry| {
        entry
          .expect("entry")
          .file_name()
          .to_string_lossy()
          .into_owned()
      })
      .collect();
    names.sort();
    names
  }

  #[test]
  fn renames_files_and_folders_without_replacing_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::write(root.join("a.txt"), "a").expect("write");
    fs::write(root.join("b.txt"), "b").expect("write");
    fs::create_dir(root.join("src")).expect("mkdir");

    let renamed = rename_entry(&root.join("a.txt"), "c.txt").expect("renames");
    assert_eq!(renamed.path, root.join("c.txt"));
    assert_eq!(renamed.directories, vec![root.to_path_buf()]);
    assert_eq!(fs::read_to_string(root.join("c.txt")).expect("reads"), "a");

    assert!(matches!(
      rename_entry(&root.join("c.txt"), "b.txt"),
      Err(EntryError::File(FileError::AlreadyExists { .. }))
    ));
    assert!(matches!(
      rename_entry(&root.join("c.txt"), "../out.txt"),
      Err(EntryError::InvalidName(_))
    ));
    // Into a new subfolder, as when creating.
    let moved = rename_entry(&root.join("c.txt"), "docs/c.txt").expect("renames");
    assert_eq!(moved.path, root.join("docs").join("c.txt"));
    assert_eq!(names(root), vec!["b.txt", "docs", "src"]);
    assert!(matches!(
      rename_entry(&root.join("src"), "src/inner"),
      Err(EntryError::IntoItself(_))
    ));
  }

  #[test]
  fn changes_only_the_case_of_a_name() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("readme.md");
    fs::write(&path, "x").expect("write");
    let renamed = rename_entry(&path, "README.md").expect("renames");
    assert_eq!(names(dir.path()), vec!["README.md"]);
    assert_eq!(renamed.path, dir.path().join("README.md"));
  }

  #[test]
  fn moves_into_folders_but_not_into_itself_or_over_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::create_dir_all(root.join("src").join("net")).expect("mkdir");
    fs::create_dir(root.join("lib")).expect("mkdir");
    fs::write(root.join("a.txt"), "a").expect("write");
    fs::write(root.join("lib").join("a.txt"), "other").expect("write");

    assert_eq!(
      move_entry(&root.join("src"), &root.join("lib")).expect("moves"),
      root.join("lib").join("src")
    );
    assert!(root.join("lib").join("src").join("net").is_dir());
    assert!(matches!(
      move_entry(&root.join("lib"), &root.join("lib").join("src")),
      Err(EntryError::IntoItself(_))
    ));
    assert!(matches!(
      move_entry(&root.join("a.txt"), &root.join("lib")),
      Err(EntryError::File(FileError::AlreadyExists { .. }))
    ));
    assert!(matches!(
      move_entry(&root.join("a.txt"), &root.join("lib").join("a.txt")),
      Err(EntryError::NotADirectory(_))
    ));
    // Where it already is: nothing to do.
    assert_eq!(
      move_entry(&root.join("a.txt"), root).expect("stays"),
      root.join("a.txt")
    );
    assert_eq!(fs::read_to_string(root.join("a.txt")).expect("reads"), "a");
  }

  #[test]
  fn copies_beside_the_original_under_a_free_name() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::write(root.join("notes.md"), "n").expect("write");
    fs::write(root.join(".gitignore"), "g").expect("write");
    fs::create_dir_all(root.join("src.d").join("inner")).expect("mkdir");
    fs::write(root.join("src.d").join("inner").join("x.rs"), "x").expect("write");

    let file = |name: &str| root.join(name);
    assert_eq!(
      copy_entry(&file("notes.md"), root).expect("copies"),
      file("notes copy.md")
    );
    assert_eq!(
      copy_entry(&file("notes.md"), root).expect("copies"),
      file("notes copy 2.md")
    );
    assert_eq!(
      copy_entry(&file(".gitignore"), root).expect("copies"),
      file(".gitignore copy")
    );
    let folder = copy_entry(&file("src.d"), root).expect("copies");
    assert_eq!(folder, file("src.d copy"));
    assert_eq!(
      fs::read_to_string(folder.join("inner").join("x.rs")).expect("reads"),
      "x"
    );
    assert!(matches!(
      copy_entry(&file("src.d"), &file("src.d").join("inner")),
      Err(EntryError::IntoItself(_))
    ));

    fs::create_dir(file("other")).expect("mkdir");
    assert_eq!(
      copy_entry(&file("notes.md"), &file("other")).expect("copies"),
      file("other").join("notes.md")
    );
  }

  #[test]
  fn deletes_files_and_folders_permanently() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path();
    fs::create_dir_all(root.join("src").join("net")).expect("mkdir");
    fs::write(root.join("src").join("net").join("a.rs"), "a").expect("write");
    fs::write(root.join("b.txt"), "b").expect("write");

    delete_entry(&root.join("src"), false).expect("deletes");
    delete_entry(&root.join("b.txt"), false).expect("deletes");
    assert!(names(root).is_empty());
    assert!(matches!(
      delete_entry(&root.join("b.txt"), false),
      Err(EntryError::File(FileError::NotFound { .. }))
    ));
  }
}
