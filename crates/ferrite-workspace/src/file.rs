//! Loading and saving text files.
//!
//! Files are read as UTF-8. A leading byte order mark is stripped on load and
//! restored on save; all other bytes, including line endings, round-trip
//! unchanged. Saving writes a temporary file beside the target, flushes it to
//! disk, and renames it over the target, so a crash never leaves a partially
//! written file.

use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ferrite_text::TextSnapshot;

/// Largest file that will be opened. Larger files are refused rather than
/// partially loaded.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// The observable on-disk state of a file, recorded when it was read or
/// written. A different stamp later means someone else changed the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
  modified: Option<SystemTime>,
  len: u64,
}

impl FileStamp {
  fn of(metadata: &fs::Metadata) -> Self {
    Self {
      modified: metadata.modified().ok(),
      len: metadata.len(),
    }
  }

  /// The current stamp of `path`, or `None` if it does not exist.
  pub fn read(path: &Path) -> Result<Option<Self>, FileError> {
    match fs::metadata(path) {
      Ok(metadata) => Ok(Some(Self::of(&metadata))),
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
      Err(error) => Err(FileError::from_io(path, error)),
    }
  }
}

#[derive(Debug)]
pub struct LoadedFile {
  pub text: String,
  pub has_bom: bool,
  pub stamp: FileStamp,
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
  #[error("{} was not found", .path.display())]
  NotFound { path: PathBuf },
  #[error("permission denied for {}", .path.display())]
  PermissionDenied { path: PathBuf },
  #[error("{} is a directory", .path.display())]
  IsDirectory { path: PathBuf },
  #[error("{} is too large to open ({size} bytes; the limit is {limit} bytes)", .path.display())]
  TooLarge {
    path: PathBuf,
    size: u64,
    limit: u64,
  },
  #[error("{} is not valid UTF-8 (first invalid byte at offset {offset})", .path.display())]
  NotUtf8 { path: PathBuf, offset: usize },
  #[error("{} was changed on disk since it was opened", .path.display())]
  ModifiedOnDisk { path: PathBuf },
  #[error("could not access {}: {source}", .path.display())]
  Io {
    path: PathBuf,
    #[source]
    source: io::Error,
  },
}

impl FileError {
  pub(crate) fn from_io(path: &Path, source: io::Error) -> Self {
    let path = path.to_path_buf();
    match source.kind() {
      io::ErrorKind::NotFound => Self::NotFound { path },
      io::ErrorKind::PermissionDenied => Self::PermissionDenied { path },
      io::ErrorKind::IsADirectory => Self::IsDirectory { path },
      _ => Self::Io { path, source },
    }
  }

  pub fn path(&self) -> &Path {
    match self {
      Self::NotFound { path }
      | Self::PermissionDenied { path }
      | Self::IsDirectory { path }
      | Self::TooLarge { path, .. }
      | Self::NotUtf8 { path, .. }
      | Self::ModifiedOnDisk { path }
      | Self::Io { path, .. } => path,
    }
  }
}

pub fn load_text_file(path: &Path) -> Result<LoadedFile, FileError> {
  load_with_limit(path, MAX_FILE_BYTES)
}

fn load_with_limit(path: &Path, limit: u64) -> Result<LoadedFile, FileError> {
  let metadata = fs::metadata(path).map_err(|error| FileError::from_io(path, error))?;
  if metadata.is_dir() {
    return Err(FileError::IsDirectory {
      path: path.to_path_buf(),
    });
  }
  let too_large = |size| FileError::TooLarge {
    path: path.to_path_buf(),
    size,
    limit,
  };
  if metadata.len() > limit {
    return Err(too_large(metadata.len()));
  }

  // Stamp from before the read: if the file changes in between, the next save
  // sees a mismatch and asks before overwriting.
  let stamp = FileStamp::of(&metadata);
  let mut bytes = fs::read(path).map_err(|error| FileError::from_io(path, error))?;
  if bytes.len() as u64 > limit {
    return Err(too_large(bytes.len() as u64));
  }

  let has_bom = bytes.starts_with(UTF8_BOM);
  if has_bom {
    bytes.drain(..UTF8_BOM.len());
  }
  let text = String::from_utf8(bytes).map_err(|error| FileError::NotUtf8 {
    path: path.to_path_buf(),
    offset: error.utf8_error().valid_up_to() + if has_bom { UTF8_BOM.len() } else { 0 },
  })?;
  Ok(LoadedFile {
    text,
    has_bom,
    stamp,
  })
}

/// Atomically writes `snapshot` to `path` and returns the new stamp.
///
/// If `expected` is given and the file on disk no longer matches it, nothing
/// is written and [`FileError::ModifiedOnDisk`] is returned. A file that was
/// deleted externally is recreated. Symbolic links are followed, so the link
/// target is replaced rather than the link.
pub fn save_text_file(
  path: &Path,
  snapshot: &TextSnapshot,
  has_bom: bool,
  expected: Option<&FileStamp>,
) -> Result<FileStamp, FileError> {
  let target = resolve_link(path)?;
  let io_error = |error| FileError::from_io(&target, error);

  let existing = match fs::metadata(&target) {
    Ok(metadata) => Some(metadata),
    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
    Err(error) => return Err(io_error(error)),
  };
  if let Some(metadata) = &existing {
    if metadata.is_dir() {
      return Err(FileError::IsDirectory { path: target });
    }
    if metadata.permissions().readonly() {
      return Err(FileError::PermissionDenied { path: target });
    }
    if expected.is_some_and(|stamp| *stamp != FileStamp::of(metadata)) {
      return Err(FileError::ModifiedOnDisk { path: target });
    }
  }

  let directory = match target.parent() {
    Some(parent) if !parent.as_os_str().is_empty() => parent,
    _ => Path::new("."),
  };
  let mut temp = tempfile::Builder::new()
    .prefix(".ferrite-")
    .suffix(".tmp")
    .tempfile_in(directory)
    .map_err(io_error)?;
  {
    let mut writer = BufWriter::new(temp.as_file_mut());
    if has_bom {
      writer.write_all(UTF8_BOM).map_err(io_error)?;
    }
    for chunk in snapshot.chunks() {
      writer.write_all(chunk.as_bytes()).map_err(io_error)?;
    }
    writer.flush().map_err(io_error)?;
  }
  #[cfg(unix)]
  if let Some(metadata) = &existing {
    fs::set_permissions(temp.path(), metadata.permissions()).map_err(io_error)?;
  }
  temp.as_file().sync_all().map_err(io_error)?;
  temp
    .persist(&target)
    .map_err(|error| io_error(error.error))?;

  tracing::debug!(path = %target.display(), bytes = snapshot.len(), "saved file");
  FileStamp::read(&target)?.ok_or(FileError::NotFound { path: target })
}

fn resolve_link(path: &Path) -> Result<PathBuf, FileError> {
  match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() => {
      fs::canonicalize(path).map_err(|error| FileError::from_io(path, error))
    }
    _ => Ok(path.to_path_buf()),
  }
}

#[cfg(test)]
mod tests {
  use ferrite_text::TextBuffer;

  use super::*;

  fn snapshot(text: &str) -> TextSnapshot {
    TextBuffer::from_text(text).snapshot()
  }

  #[test]
  fn loads_utf8_and_strips_the_bom() {
    let dir = tempfile::tempdir().expect("temp dir");
    let plain = dir.path().join("plain.txt");
    let bom = dir.path().join("bom.txt");
    fs::write(&plain, "héllo\r\n").expect("write");
    fs::write(&bom, b"\xEF\xBB\xBFhi").expect("write");

    let loaded = load_text_file(&plain).expect("loads");
    assert_eq!(loaded.text, "héllo\r\n");
    assert!(!loaded.has_bom);
    let loaded = load_text_file(&bom).expect("loads");
    assert_eq!(loaded.text, "hi");
    assert!(loaded.has_bom);
  }

  #[test]
  fn reports_invalid_utf8_with_its_offset() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("latin1.txt");
    fs::write(&path, b"caf\xE9").expect("write");
    assert!(matches!(
      load_text_file(&path),
      Err(FileError::NotUtf8 { offset: 3, .. })
    ));
  }

  #[test]
  fn reports_missing_files_directories_and_size_limits() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(matches!(
      load_text_file(&dir.path().join("missing")),
      Err(FileError::NotFound { .. })
    ));
    assert!(matches!(
      load_text_file(dir.path()),
      Err(FileError::IsDirectory { .. })
    ));
    let path = dir.path().join("big.txt");
    fs::write(&path, "0123456789").expect("write");
    assert!(matches!(
      load_with_limit(&path, 4),
      Err(FileError::TooLarge {
        size: 10,
        limit: 4,
        ..
      })
    ));
  }

  #[test]
  fn save_round_trips_bytes_exactly() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("crlf.txt");
    fs::write(&path, b"\xEF\xBB\xBFa\r\nb\rc\n").expect("write");

    let loaded = load_text_file(&path).expect("loads");
    let buffer = TextBuffer::from_text(&loaded.text);
    save_text_file(
      &path,
      &buffer.snapshot(),
      loaded.has_bom,
      Some(&loaded.stamp),
    )
    .expect("saves");
    assert_eq!(fs::read(&path).expect("read"), b"\xEF\xBB\xBFa\r\nb\rc\n");
  }

  #[test]
  fn save_creates_new_files_and_leaves_no_temporary_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("new.txt");
    let stamp = save_text_file(&path, &snapshot("fresh"), false, None).expect("saves");
    assert_eq!(fs::read_to_string(&path).expect("read"), "fresh");
    assert_eq!(FileStamp::read(&path).expect("stat"), Some(stamp));
    let entries: Vec<_> = fs::read_dir(dir.path()).expect("list").collect();
    assert_eq!(entries.len(), 1);
  }

  #[test]
  fn save_refuses_to_overwrite_external_changes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("shared.txt");
    fs::write(&path, "original").expect("write");
    let loaded = load_text_file(&path).expect("loads");

    fs::write(&path, "changed by another program").expect("write");
    let result = save_text_file(&path, &snapshot("mine"), false, Some(&loaded.stamp));
    assert!(matches!(result, Err(FileError::ModifiedOnDisk { .. })));
    assert_eq!(
      fs::read_to_string(&path).expect("read"),
      "changed by another program"
    );

    // Without an expected stamp (the user chose to overwrite) the save proceeds.
    save_text_file(&path, &snapshot("mine"), false, None).expect("saves");
    assert_eq!(fs::read_to_string(&path).expect("read"), "mine");
  }

  #[test]
  fn save_recreates_files_deleted_externally() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("gone.txt");
    fs::write(&path, "x").expect("write");
    let loaded = load_text_file(&path).expect("loads");
    fs::remove_file(&path).expect("remove");
    save_text_file(&path, &snapshot("back"), false, Some(&loaded.stamp)).expect("saves");
    assert_eq!(fs::read_to_string(&path).expect("read"), "back");
  }

  #[test]
  fn save_rejects_read_only_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("locked.txt");
    fs::write(&path, "locked").expect("write");
    let mut permissions = fs::metadata(&path).expect("stat").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).expect("chmod");

    let result = save_text_file(&path, &snapshot("nope"), false, None);
    assert!(matches!(result, Err(FileError::PermissionDenied { .. })));

    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).expect("chmod");
  }
}
