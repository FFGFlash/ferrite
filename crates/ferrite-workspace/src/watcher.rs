//! Filesystem change notifications for the workspace.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

/// How long changes to a path must settle before they are reported.
const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error)]
#[error("could not watch {}: {message}", .path.display())]
pub struct WatchError {
  pub path: PathBuf,
  pub message: String,
}

/// Watches a directory tree. Changes are reported in debounced batches of
/// paths on a background thread. Watching stops when this value is dropped.
pub struct WorkspaceWatcher {
  _debouncer: Debouncer<RecommendedWatcher>,
}

impl WorkspaceWatcher {
  pub fn new(
    root: &Path,
    on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
  ) -> Result<Self, WatchError> {
    let error = |error: notify_debouncer_mini::notify::Error| WatchError {
      path: root.to_path_buf(),
      message: error.to_string(),
    };
    let mut debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| match result {
      Ok(events) => on_change(events.into_iter().map(|event| event.path).collect()),
      Err(error) => tracing::warn!(%error, "file watcher error"),
    })
    .map_err(error)?;
    debouncer
      .watcher()
      .watch(root, RecursiveMode::Recursive)
      .map_err(error)?;
    Ok(Self {
      _debouncer: debouncer,
    })
  }
}

#[cfg(test)]
mod tests {
  use std::sync::mpsc;

  use super::*;

  #[test]
  fn reports_changed_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = std::fs::canonicalize(dir.path()).expect("canonical");
    let (sender, receiver) = mpsc::channel();
    let _watcher = WorkspaceWatcher::new(&root, move |paths| {
      let _ = sender.send(paths);
    })
    .expect("watches");

    let file = root.join("new.txt");
    std::fs::write(&file, "hello").expect("write");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut seen = Vec::new();
    while std::time::Instant::now() < deadline {
      if let Ok(paths) = receiver.recv_timeout(Duration::from_millis(200)) {
        seen.extend(paths);
        if seen.iter().any(|path| path.ends_with("new.txt")) {
          return;
        }
      }
    }
    panic!("no change reported for {}; saw {seen:?}", file.display());
  }
}
