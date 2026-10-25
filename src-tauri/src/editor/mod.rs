//! Integration of the editor core with the desktop shell: shared editor
//! state, document lifecycle (open, save, close) with file dialogs and
//! unsaved-changes prompts, and the editing commands the frontend calls.

mod commands;
mod counts;
pub(crate) mod protocol;
mod syntax;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use std::time::Instant;

use ferrite_core::Editor;
use ferrite_tasks::Priority;
use ferrite_workspace::load_text_file;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::events::{self, AppEvent};
use crate::perf::Perf;
use crate::ui_state::UiStateStore;
use protocol::DocumentSummary;

pub use commands::*;

/// The authoritative editor state. Commands hold the lock only briefly; file
/// I/O and dialogs run without it.
#[derive(Default)]
pub struct EditorState {
  editor: Arc<Mutex<Editor>>,
  /// Background find-match counts for large documents.
  pub(crate) counts: counts::MatchCounts,
  /// Syntax trees and highlighting state.
  pub(crate) syntax: syntax::SyntaxStates,
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

/// Opens the previous session's files and those named on the command line,
/// on the worker pool so the window does not wait for them. Each opened file
/// is announced to the frontend; the one matching the saved active file
/// becomes the active tab. Failures are logged and skipped.
pub fn open_startup_files(app: &AppHandle, files: Vec<PathBuf>) {
  let task_app = app.clone();
  app
    .state::<Perf>()
    .scheduler
    .spawn("open-startup-files", Priority::High, None, move |token| {
      let active = task_app.state::<UiStateStore>().get().active_file;
      for file in files {
        if token.is_cancelled() {
          break;
        }
        let path = crate::paths::normalize(&file);
        let started = Instant::now();
        let loaded = match load_text_file(&path) {
          Ok(loaded) => loaded,
          Err(error) => {
            tracing::warn!(%error, "could not open file at startup");
            continue;
          }
        };
        task_app
          .state::<Perf>()
          .record("document.load", started.elapsed());
        let summary = {
          let state = task_app.state::<EditorState>();
          let mut editor = state.lock();
          if editor.find_by_path(&path).is_some() {
            continue;
          }
          let id = editor.open(path.clone(), loaded);
          tracing::info!(document = %id, path = %path.display(), "opened file at startup");
          editor.document(id).ok().map(|document| {
            task_app
              .state::<crate::extensions::ExtensionsState>()
              .sync(document);
            DocumentSummary::of(document)
          })
        };
        if let Some(document) = summary {
          let is_active = active.as_deref() == Some(path.as_path());
          events::emit(
            &task_app,
            AppEvent::DocumentOpened {
              document,
              active: is_active,
            },
          );
        }
      }
      crate::ui_state::record_open_files(&task_app);
    });
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
