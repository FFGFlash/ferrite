//! Backups of unsaved documents, for recovery after a crash.
//!
//! Each running instance owns a session directory under the backup root,
//! holding one file per unsaved document and a `lock` file that stays
//! exclusively locked while the instance runs. The operating system releases
//! the lock when the process ends, however it ends, so a session directory
//! whose lock can be taken belongs to an instance that did not shut down
//! cleanly: its backups are the unsaved work to recover. A clean exit
//! removes its own session directory.
//!
//! A backup file is a format line, a line of JSON metadata, and the text:
//!
//! ```text
//! ferrite-backup 1
//! {"path":"C:\\src\\main.rs","title":"main.rs","cursor":120}
//! fn main() { ...
//! ```

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ferrite_text::TextSnapshot;
use serde::{Deserialize, Serialize};

use crate::MAX_FILE_BYTES;

const FORMAT_LINE: &str = "ferrite-backup 1";
const LOCK_FILE: &str = "lock";
const EXTENSION: &str = "backup";
const MAX_METADATA_BYTES: u64 = 64 * 1024;

/// What a backup records besides the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupMeta {
  /// The document's file, or `None` for an untitled document.
  pub path: Option<PathBuf>,
  /// The tab title when the backup was made.
  pub title: String,
  /// Byte offset of the primary cursor.
  #[serde(default)]
  pub cursor: usize,
}

/// A document recovered from a session that ended without cleaning up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredBackup {
  pub meta: BackupMeta,
  pub text: String,
  /// When the backup was written.
  pub written: Option<SystemTime>,
}

/// The running instance's backup directory. Dropping it keeps the backups
/// (as after a crash); [`BackupSession::discard`] removes them.
#[derive(Debug)]
pub struct BackupSession {
  dir: PathBuf,
  lock: Option<File>,
}

impl BackupSession {
  /// Creates and locks a new session directory under `root`.
  pub fn create(root: &Path) -> io::Result<Self> {
    fs::create_dir_all(root)?;
    let nanos = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_or(0, |elapsed| elapsed.as_nanos());
    let dir = root.join(format!("{}-{nanos:x}", std::process::id()));
    fs::create_dir(&dir)?;
    let lock = OpenOptions::new()
      .create(true)
      .truncate(false)
      .write(true)
      .open(dir.join(LOCK_FILE))?;
    lock.try_lock().map_err(|error| match error {
      TryLockError::Error(error) => error,
      TryLockError::WouldBlock => io::Error::other("the new backup directory is already locked"),
    })?;
    Ok(Self {
      dir,
      lock: Some(lock),
    })
  }

  pub fn dir(&self) -> &Path {
    &self.dir
  }

  /// Writes (or replaces) the backup named `name`. The file is written
  /// next to its final name and renamed into place, so a crash while
  /// writing leaves the previous backup intact.
  pub fn write(&self, name: &str, meta: &BackupMeta, text: &TextSnapshot) -> io::Result<()> {
    let target = self.path_of(name);
    let temp = target.with_extension("tmp");
    let result = (|| {
      let file = File::create(&temp)?;
      let mut writer = BufWriter::new(&file);
      writeln!(writer, "{FORMAT_LINE}")?;
      serde_json::to_writer(&mut writer, meta)?;
      writeln!(writer)?;
      for chunk in text.chunks() {
        writer.write_all(chunk.as_bytes())?;
      }
      writer.flush()?;
      drop(writer);
      file.sync_data()?;
      drop(file);
      fs::rename(&temp, &target)
    })();
    if result.is_err() {
      let _ = fs::remove_file(&temp);
    }
    result
  }

  /// Removes the backup named `name`, if there is one.
  pub fn remove(&self, name: &str) -> io::Result<()> {
    match fs::remove_file(self.path_of(name)) {
      Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
      _ => Ok(()),
    }
  }

  /// Removes the session directory and its backups, after a clean exit.
  pub fn discard(mut self) -> io::Result<()> {
    // The lock file can't be removed with the directory while it is open
    // (on Windows).
    drop(self.lock.take());
    fs::remove_dir_all(&self.dir)
  }

  fn path_of(&self, name: &str) -> PathBuf {
    self.dir.join(format!("{}.{EXTENSION}", sanitize(name)))
  }
}

/// Sessions under a backup root whose instance has exited without cleaning
/// up, locked while their documents are restored.
#[derive(Debug, Default)]
pub struct Abandoned {
  sessions: Vec<(PathBuf, File)>,
  pub documents: Vec<RecoveredBackup>,
  /// Backups that could not be read, with the reason.
  pub unreadable: Vec<(PathBuf, String)>,
}

impl Abandoned {
  /// Finds abandoned sessions under `root` and reads their backups. Sessions
  /// of running instances are left alone. Nothing is removed until
  /// [`Abandoned::finish`], so the caller can first restore the documents
  /// (and back them up again in its own session).
  pub fn find(root: &Path, own: Option<&Path>) -> io::Result<Self> {
    let mut abandoned = Self::default();
    let entries = match fs::read_dir(root) {
      Ok(entries) => entries,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(abandoned),
      Err(error) => return Err(error),
    };
    let mut dirs: Vec<PathBuf> = entries
      .filter_map(Result::ok)
      .map(|entry| entry.path())
      .filter(|path| path.is_dir() && Some(path.as_path()) != own)
      .collect();
    dirs.sort();
    for dir in dirs {
      let lock = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK_FILE))
      {
        Ok(lock) => lock,
        Err(error) => {
          abandoned.unreadable.push((dir, error.to_string()));
          continue;
        }
      };
      match lock.try_lock() {
        Ok(()) => {}
        // A running instance owns it.
        Err(TryLockError::WouldBlock) => continue,
        Err(TryLockError::Error(error)) => {
          abandoned.unreadable.push((dir, error.to_string()));
          continue;
        }
      }
      abandoned.read_session(&dir);
      abandoned.sessions.push((dir, lock));
    }
    abandoned.documents.sort_by_key(|document| document.written);
    Ok(abandoned)
  }

  fn read_session(&mut self, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
      return;
    };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
      if path.extension().and_then(|extension| extension.to_str()) != Some(EXTENSION) {
        continue;
      }
      match read_backup(&path) {
        Ok(backup) => self.documents.push(backup),
        Err(error) => self.unreadable.push((path, error.to_string())),
      }
    }
  }

  /// Whether anything was found.
  pub fn is_empty(&self) -> bool {
    self.sessions.is_empty()
  }

  /// Removes the abandoned sessions.
  pub fn finish(self) -> io::Result<()> {
    let mut result = Ok(());
    for (dir, lock) in self.sessions {
      drop(lock);
      if let Err(error) = fs::remove_dir_all(&dir) {
        result = Err(error);
      }
    }
    result
  }
}

fn read_backup(path: &Path) -> io::Result<RecoveredBackup> {
  let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidData, message.to_owned());
  let file = File::open(path)?;
  let written = file.metadata()?.modified().ok();
  let mut reader = BufReader::new(file);
  let mut line = String::new();
  reader.read_line(&mut line)?;
  if line.trim_end() != FORMAT_LINE {
    return Err(invalid("not a Ferrite backup"));
  }
  line.clear();
  (&mut reader)
    .take(MAX_METADATA_BYTES)
    .read_line(&mut line)?;
  let meta: BackupMeta =
    serde_json::from_str(&line).map_err(|error| invalid(&format!("invalid metadata: {error}")))?;
  let mut bytes = Vec::new();
  reader.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
  if bytes.len() as u64 > MAX_FILE_BYTES {
    return Err(invalid("too large"));
  }
  let text = String::from_utf8(bytes).map_err(|_| invalid("the text is not UTF-8"))?;
  Ok(RecoveredBackup {
    meta,
    text,
    written,
  })
}

/// Keeps backup names to characters that are valid in file names on every
/// platform.
fn sanitize(name: &str) -> String {
  name
    .chars()
    .map(|c| {
      if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
        c
      } else {
        '_'
      }
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use ferrite_text::TextBuffer;

  use super::*;

  fn meta(path: Option<&str>, title: &str) -> BackupMeta {
    BackupMeta {
      path: path.map(PathBuf::from),
      title: title.into(),
      cursor: 3,
    }
  }

  fn snapshot(text: &str) -> TextSnapshot {
    TextBuffer::from_text(text).snapshot()
  }

  #[test]
  fn backups_of_a_crashed_session_are_recovered_once() {
    let root = tempfile::tempdir().expect("temp dir");
    let crashed = BackupSession::create(root.path()).expect("session");
    let text = "line one\r\nline two\nü\n";
    crashed
      .write("1", &meta(Some("C:/a.rs"), "a.rs"), &snapshot(text))
      .expect("write");
    crashed
      .write("2", &meta(None, "Untitled-1"), &snapshot("draft"))
      .expect("write");
    crashed
      .write("2", &meta(None, "Untitled-1"), &snapshot("draft, longer"))
      .expect("replace");
    // The process ends without discarding: the lock is released.
    drop(crashed);

    let current = BackupSession::create(root.path()).expect("session");
    let found = Abandoned::find(root.path(), Some(current.dir())).expect("find");
    assert!(!found.is_empty());
    assert!(found.unreadable.is_empty(), "{:?}", found.unreadable);
    let mut texts: Vec<_> = found
      .documents
      .iter()
      .map(|backup| (backup.meta.title.as_str(), backup.text.as_str()))
      .collect();
    texts.sort();
    assert_eq!(texts, vec![("Untitled-1", "draft, longer"), ("a.rs", text)]);
    assert_eq!(found.documents[0].meta.cursor, 3);

    found.finish().expect("finish");
    let again = Abandoned::find(root.path(), Some(current.dir())).expect("find");
    assert!(again.is_empty() && again.documents.is_empty());
    assert!(current.dir().is_dir(), "the current session is kept");
  }

  #[test]
  fn sessions_of_running_instances_are_left_alone() {
    let root = tempfile::tempdir().expect("temp dir");
    let running = BackupSession::create(root.path()).expect("session");
    running
      .write("1", &meta(None, "Untitled-1"), &snapshot("mine"))
      .expect("write");
    let found = Abandoned::find(root.path(), None).expect("find");
    assert!(found.is_empty() && found.documents.is_empty());
    found.finish().expect("finish");
    assert!(running.dir().join("1.backup").is_file());
  }

  #[test]
  fn a_clean_exit_removes_the_session() {
    let root = tempfile::tempdir().expect("temp dir");
    let session = BackupSession::create(root.path()).expect("session");
    session
      .write("7", &meta(None, "Untitled-1"), &snapshot("x"))
      .expect("write");
    session.remove("7").expect("remove");
    session.remove("7").expect("removing again is fine");
    let dir = session.dir().to_path_buf();
    session.discard().expect("discard");
    assert!(!dir.exists());
    assert!(Abandoned::find(root.path(), None).expect("find").is_empty());
  }

  #[test]
  fn damaged_backups_are_reported_and_skipped() {
    let root = tempfile::tempdir().expect("temp dir");
    let session = BackupSession::create(root.path()).expect("session");
    session
      .write("good", &meta(None, "Untitled-1"), &snapshot("ok"))
      .expect("write");
    let dir = session.dir().to_path_buf();
    fs::write(dir.join("bad.backup"), "something else\n").expect("write");
    fs::write(dir.join("worse.backup"), format!("{FORMAT_LINE}\n{{\n")).expect("write");
    fs::write(dir.join("half.tmp"), "interrupted").expect("write");
    drop(session);

    let found = Abandoned::find(root.path(), None).expect("find");
    assert_eq!(found.documents.len(), 1);
    assert_eq!(found.unreadable.len(), 2);
    found.finish().expect("finish");
    assert!(!dir.exists());
  }

  #[test]
  fn names_are_safe_file_names() {
    assert_eq!(sanitize("a/b\\c:1"), "a_b_c_1");
  }
}
