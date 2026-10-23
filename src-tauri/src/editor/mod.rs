//! Integration of the editor core with the desktop shell: shared editor
//! state, document lifecycle (open, save, close) with file dialogs and
//! unsaved-changes prompts, and the editing commands the frontend calls.

mod commands;
pub(crate) mod protocol;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ferrite_core::Editor;
use ferrite_workspace::load_text_file;
use tauri::{Manager, WebviewWindow};

pub use commands::*;

/// The authoritative editor state. Commands hold the lock only briefly; file
/// I/O and dialogs run without it.
#[derive(Default)]
pub struct EditorState {
  editor: Arc<Mutex<Editor>>,
  /// Set once the user has confirmed closing the window with unsaved changes.
  close_confirmed: AtomicBool,
  close_prompt_open: AtomicBool,
}

impl EditorState {
  pub(crate) fn lock(&self) -> MutexGuard<'_, Editor> {
    // A panic while holding the lock must not make every document inaccessible.
    self.editor.lock().unwrap_or_else(PoisonError::into_inner)
  }

  fn has_unsaved_changes(&self) -> bool {
    self.lock().documents().any(|document| document.is_dirty())
  }
}

/// Opens files before the frontend loads, so they appear as tabs
/// immediately: those from the previous session and those named on the
/// command line. Arguments starting with `-` are ignored. Failures are
/// logged and skipped.
pub fn open_startup_files(state: &EditorState, files: impl IntoIterator<Item = PathBuf>) {
  for arg in files {
    if arg.to_string_lossy().starts_with('-') {
      continue;
    }
    let path = crate::paths::normalize(&arg);
    match load_text_file(&path) {
      Ok(file) => {
        let mut editor = state.lock();
        if editor.find_by_path(&path).is_none() {
          let id = editor.open(path.clone(), file);
          tracing::info!(document = %id, path = %path.display(), "opened file at startup");
        }
      }
      Err(error) => tracing::warn!(%error, "could not open file at startup"),
    }
  }
}

/// Decides whether the window may close. With unsaved documents, prevents
/// the close and asks the user (asynchronously) whether to save them.
pub fn on_close_requested(window: &WebviewWindow, api: &tauri::CloseRequestApi) {
  let state = window.state::<EditorState>();
  if state.close_confirmed.load(Ordering::SeqCst) || !state.has_unsaved_changes() {
    return;
  }
  api.prevent_close();
  if state.close_prompt_open.swap(true, Ordering::SeqCst) {
    return;
  }
  let window = window.clone();
  tauri::async_runtime::spawn(async move {
    let state = window.state::<EditorState>();
    let close = commands::confirm_close_with_unsaved(&window).await;
    state.close_prompt_open.store(false, Ordering::SeqCst);
    if close {
      state.close_confirmed.store(true, Ordering::SeqCst);
      if let Err(error) = window.destroy() {
        tracing::error!(%error, "failed to close the window");
      }
    }
  });
}
