//! Integration of the editor core with the desktop shell: shared editor
//! state, document lifecycle (open, save, close) with file dialogs and
//! unsaved-changes prompts, and the editing commands the frontend calls.

mod commands;
mod counts;
mod minimap;
pub(crate) mod protocol;
mod syntax;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use std::time::Instant;

use ferrite_core::Editor;
use ferrite_tasks::Priority;
use ferrite_workspace::{Abandoned, load_text_file};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::events::{self, AppEvent, NoticeLevel};
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
  /// The text last copied as whole lines (copy with nothing selected), so
  /// pasting it can insert whole lines too.
  copied_lines: Mutex<Option<String>>,
  /// Set once the user has confirmed closing the window with unsaved changes.
  close_confirmed: AtomicBool,
  close_prompt_open: AtomicBool,
}

impl EditorState {
  pub(crate) fn lock(&self) -> MutexGuard<'_, Editor> {
    // A panic while holding the lock must not make every document inaccessible.
    self.editor.lock().unwrap_or_else(PoisonError::into_inner)
  }

  fn copied_lines(&self) -> Option<String> {
    self
      .copied_lines
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .clone()
  }

  fn set_copied_lines(&self, text: Option<String>) {
    *self
      .copied_lines
      .lock()
      .unwrap_or_else(PoisonError::into_inner) = text;
  }

  fn has_unsaved_changes(&self) -> bool {
    self.lock().documents().any(|document| document.is_dirty())
  }

  /// Whether the app is exiting with every unsaved change saved or
  /// deliberately discarded.
  pub(crate) fn exit_is_clean(&self) -> bool {
    self.close_confirmed.load(Ordering::SeqCst) || !self.has_unsaved_changes()
  }
}

/// Opens the previous session's files and those named on the command line,
/// on the worker pool so the window does not wait for them, after restoring
/// unsaved changes from crashed sessions. Each opened file is announced to
/// the frontend; the one matching the saved active file becomes the active
/// tab. Files that can't be opened are skipped and reported together.
pub fn open_startup_files(app: &AppHandle, files: Vec<PathBuf>, crashed: Option<Abandoned>) {
  let task_app = app.clone();
  app
    .state::<Perf>()
    .scheduler
    .spawn("open-startup-files", Priority::High, None, move |token| {
      if let Some(crashed) = crashed {
        crate::backups::restore(&task_app, crashed);
      }
      let active = task_app.state::<UiStateStore>().get().active_file;
      let mut failed = Vec::new();
      for file in files {
        if token.is_cancelled() {
          break;
        }
        let path = crate::paths::normalize(&file);
        // Already open (restored from a backup): only activate it if wanted.
        let open = {
          let state = task_app.state::<EditorState>();
          let editor = state.lock();
          editor
            .find_by_path(&path)
            .and_then(|id| editor.document(id).ok().map(DocumentSummary::of))
        };
        if let Some(document) = open {
          if active.as_deref() == Some(path.as_path()) {
            events::emit(
              &task_app,
              AppEvent::DocumentOpened {
                document,
                active: true,
              },
            );
          }
          continue;
        }
        let started = Instant::now();
        let loaded = match load_text_file(&path) {
          Ok(loaded) => loaded,
          Err(error) => {
            tracing::warn!(%error, "could not open file at startup");
            failed.push(error.to_string());
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
      if !failed.is_empty() {
        let message = match failed.as_slice() {
          [one] => format!("A file from the last session could not be reopened: {one}."),
          many => format!(
            "{} files from the last session could not be reopened: {}.",
            many.len(),
            many.join("; ")
          ),
        };
        events::emit(
          &task_app,
          AppEvent::Notice {
            level: NoticeLevel::Warning,
            message,
          },
        );
      }
    });
}

/// Decides whether the window may close. With unsaved documents, prevents
/// the close and asks the user (asynchronously) whether to save them.
pub fn on_close_requested(window: &WebviewWindow, api: &tauri::CloseRequestApi) {
  if window.state::<EditorState>().exit_is_clean() {
    return;
  }
  api.prevent_close();
  confirm_then_close(window);
}

/// Decides whether the app may exit when that's requested other than by
/// closing the window (Quit on macOS). With unsaved documents, prevents the
/// exit and asks first, as when closing the window.
pub fn on_exit_requested(app: &AppHandle, api: &tauri::ExitRequestApi) {
  if app.state::<EditorState>().exit_is_clean() {
    return;
  }
  match app.webview_windows().into_values().next() {
    Some(window) => {
      api.prevent_exit();
      confirm_then_close(&window);
    }
    // No window to ask in; the backups keep the changes.
    None => tracing::warn!("exiting with unsaved changes; they are kept as backups"),
  }
}

/// Asks about unsaved changes (once at a time) and closes the window if the
/// user saves or discards them.
fn confirm_then_close(window: &WebviewWindow) {
  let state = window.state::<EditorState>();
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
