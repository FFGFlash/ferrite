//! Loading and saving text files.
//!
//! Files are read whole, in the encoding [`crate::encoding`] detects (or the
//! one asked for), and saved in the same encoding. A byte order mark is
//! stripped on load and restored on save; all other bytes, including line
//! endings, round-trip unchanged. Saving writes a temporary file beside the
//! target, flushes it to disk, and renames it over the target, so a crash
//! never leaves a partially written file.

use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ferrite_text::TextSnapshot;

use crate::encoding::{self, DecodeError, DecodeOptions, EncodeError, TextEncoding};

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
  pub encoding: TextEncoding,
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
  #[error("{} already exists", .path.display())]
  AlreadyExists { path: PathBuf },
  #[error("{} is binary or uses an unsupported text encoding", .path.display())]
  Binary { path: PathBuf },
  #[error("{} isn't valid {encoding} text", .path.display())]
  InvalidText { path: PathBuf, encoding: String },
  #[error(
    "{} can't be opened as {encoding}: saving it would change bytes you didn't edit",
    .path.display()
  )]
  LossyEncoding { path: PathBuf, encoding: String },
  #[error(
    "{} can't be saved as {encoding}, which has no {character:?}. Save it with another encoding",
    .path.display()
  )]
  Unencodable {
    path: PathBuf,
    encoding: String,
    character: char,
  },
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
      io::ErrorKind::AlreadyExists => Self::AlreadyExists { path },
      _ => Self::Io { path, source },
    }
  }

  pub fn path(&self) -> &Path {
    match self {
      Self::NotFound { path }
      | Self::PermissionDenied { path }
      | Self::IsDirectory { path }
      | Self::AlreadyExists { path }
      | Self::Binary { path }
      | Self::InvalidText { path, .. }
      | Self::LossyEncoding { path, .. }
      | Self::Unencodable { path, .. }
      | Self::ModifiedOnDisk { path }
      | Self::Io { path, .. } => path,
    }
  }
}

/// Reads a text file whole, detecting its encoding. Files that look binary
/// are refused with [`FileError::Binary`].
pub fn load_text_file(path: &Path) -> Result<LoadedFile, FileError> {
  load_text_file_with(path, DecodeOptions::default())
}

/// Reads a text file whole, in the encoding `options` names (or detecting
/// one), and if asked, even if it looks binary.
pub fn load_text_file_with(path: &Path, options: DecodeOptions) -> Result<LoadedFile, FileError> {
  let metadata = fs::metadata(path).map_err(|error| FileError::from_io(path, error))?;
  if metadata.is_dir() {
    return Err(FileError::IsDirectory {
      path: path.to_path_buf(),
    });
  }
  // Stamp from before the read: if the file changes in between, the next save
  // sees a mismatch and asks before overwriting.
  let stamp = FileStamp::of(&metadata);
  let bytes = fs::read(path).map_err(|error| FileError::from_io(path, error))?;
  let path = path.to_path_buf();
  let (text, encoding) = encoding::decode(bytes, options).map_err(|error| match error {
    DecodeError::Binary => FileError::Binary { path },
    DecodeError::Invalid(encoding) => FileError::InvalidText {
      path,
      encoding: encoding.label(),
    },
    DecodeError::Lossy(encoding) => FileError::LossyEncoding {
      path,
      encoding: encoding.label(),
    },
  })?;
  Ok(LoadedFile {
    text,
    encoding,
    stamp,
  })
}

/// Atomically writes `snapshot` to `path` in `encoding` and returns the new
/// stamp. Text the encoding can't represent fails with
/// [`FileError::Unencodable`], and the file is left as it was.
///
/// If `expected` is given and the file on disk no longer matches it, nothing
/// is written and [`FileError::ModifiedOnDisk`] is returned. A file that was
/// deleted externally is recreated. Symbolic links are followed, so the link
/// target is replaced rather than the link.
pub fn save_text_file(
  path: &Path,
  snapshot: &TextSnapshot,
  encoding: TextEncoding,
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
    // On failure the temporary file is removed when dropped.
    encoding
      .write(snapshot.chunks(), &mut writer)
      .map_err(|error| match error {
        EncodeError::Unmappable(character) => FileError::Unencodable {
          path: target.clone(),
          encoding: encoding.label(),
          character,
        },
        EncodeError::Io(error) => io_error(error),
      })?;
    writer.flush().map_err(io_error)?;
  }
  #[cfg(unix)]
  if let Some(metadata) = &existing {
    fs::set_permissions(temp.path(), metadata.permissions()).map_err(io_error)?;
  }
  temp.as_file().sync_all().map_err(io_error)?;
  persist(temp, &target).map_err(io_error)?;

  tracing::debug!(path = %target.display(), bytes = snapshot.len(), "saved file");
  FileStamp::read(&target)?.ok_or(FileError::NotFound { path: target })
}

/// Atomically replaces the file at `path` with `bytes`, creating its
/// directory if needed: for small files the app writes itself (settings,
/// session state). Symbolic links are followed, as when saving documents.
pub fn write_file_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
  let target = match fs::symlink_metadata(path) {
    Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
    _ => path.to_path_buf(),
  };
  let directory = match target.parent() {
    Some(parent) if !parent.as_os_str().is_empty() => parent,
    _ => Path::new("."),
  };
  fs::create_dir_all(directory)?;
  let mut temp = tempfile::Builder::new()
    .prefix(".ferrite-")
    .suffix(".tmp")
    .tempfile_in(directory)?;
  temp.write_all(bytes)?;
  temp.as_file().sync_all()?;
  persist(temp, &target)
}

/// Renames a finished temporary file over `target`. On Windows the rename
/// fails while another program (a virus scanner, an indexer, a sync client)
/// briefly has the target open, so it is retried for a short while.
fn persist(mut temp: tempfile::NamedTempFile, target: &Path) -> io::Result<()> {
  const RETRIES: u32 = 5;
  let mut attempt = 0;
  loop {
    match temp.persist(target) {
      Ok(_) => return Ok(()),
      Err(error) if attempt < RETRIES && is_transient(&error.error) => {
        attempt += 1;
        tracing::debug!(path = %target.display(), attempt, error = %error.error, "retrying save");
        std::thread::sleep(Duration::from_millis(10 << attempt));
        temp = error.file;
      }
      Err(error) => return Err(error.error),
    }
  }
}

/// Access denied, sharing and lock violations: what Windows reports while
/// another process has the file open.
#[cfg(windows)]
fn is_transient(error: &io::Error) -> bool {
  matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(not(windows))]
fn is_transient(_error: &io::Error) -> bool {
  false
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
  fn small_files_are_replaced_atomically() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("config").join("settings.toml");
    write_file_atomically(&path, b"theme = 1").expect("creates");
    write_file_atomically(&path, b"theme = 2").expect("replaces");
    assert_eq!(fs::read_to_string(&path).expect("read"), "theme = 2");
    let leftovers = fs::read_dir(path.parent().expect("parent"))
      .expect("list")
      .count();
    assert_eq!(leftovers, 1, "no temporary files are left behind");
  }

  #[cfg(unix)]
  #[test]
  fn atomic_writes_follow_symbolic_links() {
    let dir = tempfile::tempdir().expect("temp dir");
    let real = dir.path().join("dotfiles-settings.toml");
    let link = dir.path().join("settings.toml");
    fs::write(&real, "old").expect("write");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    write_file_atomically(&link, b"new").expect("writes");
    assert!(
      fs::symlink_metadata(&link)
        .expect("stat")
        .file_type()
        .is_symlink()
    );
    assert_eq!(fs::read_to_string(&real).expect("read"), "new");
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
    assert_eq!(loaded.encoding, TextEncoding::UTF8);
    let loaded = load_text_file(&bom).expect("loads");
    assert_eq!(loaded.text, "hi");
    assert_eq!(loaded.encoding, TextEncoding::UTF8_BOM);
  }

  #[test]
  fn loads_other_encodings_and_refuses_binary_files_unless_asked() {
    let dir = tempfile::tempdir().expect("temp dir");
    let latin1 = dir.path().join("latin1.txt");
    fs::write(&latin1, b"caf\xE9 cr\xE8me").expect("write");
    let loaded = load_text_file(&latin1).expect("loads");
    assert_eq!(loaded.text, "café crème");
    assert_eq!(loaded.encoding.id(), "windows-1252");

    let binary = dir.path().join("image.png");
    fs::write(&binary, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("write");
    assert!(matches!(
      load_text_file(&binary),
      Err(FileError::Binary { .. })
    ));
    let anyway = DecodeOptions {
      binary: true,
      ..DecodeOptions::default()
    };
    let loaded = load_text_file_with(&binary, anyway).expect("loads");
    assert_eq!(loaded.text.chars().count(), 16);

    let as_utf8 = DecodeOptions {
      encoding: Some(TextEncoding::UTF8),
      binary: false,
    };
    assert!(matches!(
      load_text_file_with(&latin1, as_utf8),
      Err(FileError::InvalidText { encoding, .. }) if encoding == "UTF-8"
    ));
  }

  #[test]
  fn reports_missing_files_and_directories() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(matches!(
      load_text_file(&dir.path().join("missing")),
      Err(FileError::NotFound { .. })
    ));
    assert!(matches!(
      load_text_file(dir.path()),
      Err(FileError::IsDirectory { .. })
    ));
  }

  #[test]
  fn saves_in_the_files_encoding() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("utf16.txt");
    fs::write(&path, b"\xFF\xFEa\0\r\0\n\0").expect("write");
    let loaded = load_text_file(&path).expect("loads");
    let edited = snapshot(&format!("{}é", loaded.text));
    save_text_file(&path, &edited, loaded.encoding, Some(&loaded.stamp)).expect("saves");
    assert_eq!(fs::read(&path).expect("read"), b"\xFF\xFEa\0\r\0\n\0\xE9\0");
  }

  #[test]
  fn refuses_to_save_characters_the_encoding_lacks() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("latin1.txt");
    fs::write(&path, b"caf\xE9").expect("write");
    let loaded = load_text_file(&path).expect("loads");
    let result = save_text_file(
      &path,
      &snapshot("snowman ☃"),
      loaded.encoding,
      Some(&loaded.stamp),
    );
    assert!(matches!(
      result,
      Err(FileError::Unencodable {
        character: '☃', ..
      })
    ));
    assert_eq!(fs::read(&path).expect("read"), b"caf\xE9");
    let entries: Vec<_> = fs::read_dir(dir.path()).expect("list").collect();
    assert_eq!(entries.len(), 1, "the temporary file is removed");
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
      loaded.encoding,
      Some(&loaded.stamp),
    )
    .expect("saves");
    assert_eq!(fs::read(&path).expect("read"), b"\xEF\xBB\xBFa\r\nb\rc\n");
  }

  #[test]
  fn save_creates_new_files_and_leaves_no_temporary_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("new.txt");
    let stamp = save_text_file(&path, &snapshot("fresh"), TextEncoding::UTF8, None).expect("saves");
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
    let result = save_text_file(
      &path,
      &snapshot("mine"),
      TextEncoding::UTF8,
      Some(&loaded.stamp),
    );
    assert!(matches!(result, Err(FileError::ModifiedOnDisk { .. })));
    assert_eq!(
      fs::read_to_string(&path).expect("read"),
      "changed by another program"
    );

    // Without an expected stamp (the user chose to overwrite) the save proceeds.
    save_text_file(&path, &snapshot("mine"), TextEncoding::UTF8, None).expect("saves");
    assert_eq!(fs::read_to_string(&path).expect("read"), "mine");
  }

  #[test]
  fn save_recreates_files_deleted_externally() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("gone.txt");
    fs::write(&path, "x").expect("write");
    let loaded = load_text_file(&path).expect("loads");
    fs::remove_file(&path).expect("remove");
    save_text_file(
      &path,
      &snapshot("back"),
      TextEncoding::UTF8,
      Some(&loaded.stamp),
    )
    .expect("saves");
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

    let result = save_text_file(&path, &snapshot("nope"), TextEncoding::UTF8, None);
    assert!(matches!(result, Err(FileError::PermissionDenied { .. })));

    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).expect("chmod");
  }
}
