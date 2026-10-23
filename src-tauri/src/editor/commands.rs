//! Document and editing commands.
//!
//! Lifecycle commands (`document_*`) may show dialogs and touch the disk, so
//! they are async and never hold the editor lock across an await. Editing
//! commands (`editor_*`) are fast and synchronous; each returns the updated
//! view for the caller's viewport, so a keystroke costs one round trip and
//! transfers only visible lines.

use std::path::Path;

use ferrite_core::{DocumentId, EditorOp};
use ferrite_workspace::{FileError, load_text_file, save_text_file};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_clipboard_manager::ClipboardExt;

use super::EditorState;
use super::dialogs::{self, UnsavedChoice};
use super::error::CommandError;
use super::paths;
use super::protocol::{DocumentSummary, EditorView, MAX_OPS_PER_BATCH, OpDto, Viewport};

#[tauri::command]
pub fn document_list(state: State<'_, EditorState>) -> Vec<DocumentSummary> {
  state.lock().documents().map(DocumentSummary::of).collect()
}

#[tauri::command]
pub fn document_new(state: State<'_, EditorState>) -> Result<DocumentSummary, CommandError> {
  let mut editor = state.lock();
  let id = editor.create_untitled();
  tracing::info!(document = %id, "created untitled document");
  Ok(DocumentSummary::of(editor.document(id)?))
}

#[tauri::command]
pub async fn document_open(app: AppHandle, path: String) -> Result<DocumentSummary, CommandError> {
  open_path(&app, Path::new(&path)).await
}

/// Shows an open dialog and opens the chosen file. `None` if cancelled.
#[tauri::command]
pub async fn document_open_dialog(
  app: AppHandle,
  window: WebviewWindow,
) -> Result<Option<DocumentSummary>, CommandError> {
  match dialogs::pick_file_to_open(&window).await {
    Some(path) => open_path(&app, &path).await.map(Some),
    None => Ok(None),
  }
}

/// Saves a document, asking for a path if it has none. `None` if the user
/// cancelled.
#[tauri::command]
pub async fn document_save(
  window: WebviewWindow,
  id: u64,
) -> Result<Option<DocumentSummary>, CommandError> {
  save(&window, DocumentId::from_raw(id), false).await
}

#[tauri::command]
pub async fn document_save_as(
  window: WebviewWindow,
  id: u64,
) -> Result<Option<DocumentSummary>, CommandError> {
  save(&window, DocumentId::from_raw(id), true).await
}

/// Closes a document, first asking whether to save unsaved changes. Returns
/// whether it was closed.
#[tauri::command]
pub async fn document_close(window: WebviewWindow, id: u64) -> Result<bool, CommandError> {
  let id = DocumentId::from_raw(id);
  let (dirty, title) = {
    let state = window.state::<EditorState>();
    let editor = state.lock();
    let document = editor.document(id)?;
    (document.is_dirty(), document.title())
  };
  if dirty {
    let message = format!("Do you want to save the changes you made to {title}?");
    match dialogs::ask_unsaved(&window, message, "Save").await {
      UnsavedChoice::Save => {
        if save(&window, id, false).await?.is_none() {
          return Ok(false);
        }
      }
      UnsavedChoice::Discard => {}
      UnsavedChoice::Cancel => return Ok(false),
    }
  }
  window.state::<EditorState>().lock().close(id)?;
  tracing::info!(document = %id, "closed document");
  Ok(true)
}

#[tauri::command]
pub fn editor_view(
  state: State<'_, EditorState>,
  id: u64,
  viewport: Viewport,
) -> Result<EditorView, CommandError> {
  let editor = state.lock();
  let document = editor.document(DocumentId::from_raw(id))?;
  Ok(EditorView::of(document, viewport))
}

/// Applies a batch of operations in order and returns the resulting view.
#[tauri::command]
pub fn editor_apply(
  state: State<'_, EditorState>,
  id: u64,
  ops: Vec<OpDto>,
  viewport: Viewport,
) -> Result<EditorView, CommandError> {
  if ops.len() > MAX_OPS_PER_BATCH {
    return Err(CommandError::new(
      "tooManyOps",
      format!("At most {MAX_OPS_PER_BATCH} operations can be applied at once."),
    ));
  }
  let mut editor = state.lock();
  let document = editor.document_mut(DocumentId::from_raw(id))?;
  for op in ops {
    let op = op.into_core(document.buffer());
    document.apply(op)?;
  }
  Ok(EditorView::of(document, viewport))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardAction {
  Copy,
  Cut,
}

/// Copies (or cuts) the selected text to the system clipboard. The text goes
/// straight from the backend to the clipboard without crossing IPC.
#[tauri::command]
pub fn editor_clipboard(
  app: AppHandle,
  state: State<'_, EditorState>,
  id: u64,
  action: ClipboardAction,
  viewport: Viewport,
) -> Result<EditorView, CommandError> {
  let mut editor = state.lock();
  let document = editor.document_mut(DocumentId::from_raw(id))?;
  if let Some(text) = document.selected_text() {
    app
      .clipboard()
      .write_text(text)
      .map_err(|error| CommandError::new("clipboard", format!("Could not copy: {error}")))?;
    if let ClipboardAction::Cut = action {
      document.apply(EditorOp::DeleteSelection)?;
    }
  }
  Ok(EditorView::of(document, viewport))
}

/// Asks whether to save unsaved documents before the window closes, saving
/// them if requested. Returns whether the window should close.
pub async fn confirm_close_with_unsaved(window: &WebviewWindow) -> bool {
  let dirty: Vec<(DocumentId, String)> = window
    .state::<EditorState>()
    .lock()
    .documents()
    .filter(|document| document.is_dirty())
    .map(|document| (document.id(), document.title()))
    .collect();
  let message = match dirty.as_slice() {
    [] => return true,
    [(_, title)] => format!("Do you want to save the changes you made to {title}?"),
    many => format!("Do you want to save the changes to {} files?", many.len()),
  };
  match dialogs::ask_unsaved(window, message, "Save All").await {
    UnsavedChoice::Discard => true,
    UnsavedChoice::Cancel => false,
    UnsavedChoice::Save => {
      for (id, title) in dirty {
        match save(window, id, false).await {
          Ok(Some(_)) => {}
          Ok(None) => return false,
          Err(error) => {
            tracing::error!(document = %id, %title, message = %error.message, "save before close failed");
            return false;
          }
        }
      }
      true
    }
  }
}

async fn open_path(app: &AppHandle, path: &Path) -> Result<DocumentSummary, CommandError> {
  let path = paths::normalize(path);
  let state = app.state::<EditorState>();
  if let Some(summary) = find_open(&state, &path) {
    return Ok(summary);
  }

  let to_load = path.clone();
  let loaded = blocking(move || load_text_file(&to_load)).await??;

  let mut editor = state.lock();
  // Another request may have opened the same file while this one was loading.
  if let Some(id) = editor.find_by_path(&path) {
    return Ok(DocumentSummary::of(editor.document(id)?));
  }
  let bytes = loaded.text.len();
  let id = editor.open(path.clone(), loaded);
  tracing::info!(document = %id, path = %path.display(), bytes, "opened file");
  Ok(DocumentSummary::of(editor.document(id)?))
}

fn find_open(state: &EditorState, path: &Path) -> Option<DocumentSummary> {
  let editor = state.lock();
  let id = editor.find_by_path(path)?;
  editor.document(id).ok().map(DocumentSummary::of)
}

/// Saves `id`, showing a Save As dialog when it has no path or `choose_path`
/// is set, and asking before overwriting a file changed by another program.
async fn save(
  window: &WebviewWindow,
  id: DocumentId,
  choose_path: bool,
) -> Result<Option<DocumentSummary>, CommandError> {
  let state = window.state::<EditorState>();
  let (point, current_path, title) = {
    let editor = state.lock();
    let document = editor.document(id)?;
    (
      document.save_point(),
      document.path().map(Path::to_path_buf),
      document.title(),
    )
  };

  let path = match (&current_path, choose_path) {
    (Some(path), false) => path.clone(),
    _ => match dialogs::pick_save_path(window, current_path.as_deref(), &title).await {
      Some(path) => paths::normalize(&path),
      None => return Ok(None),
    },
  };
  let same_file = current_path.as_deref() == Some(path.as_path());
  if !same_file {
    let editor = state.lock();
    if editor.find_by_path(&path).is_some_and(|other| other != id) {
      return Err(CommandError::new(
        "alreadyOpen",
        format!("{} is open in another tab. Close it first.", path.display()),
      ));
    }
  }
  // Save As to another path: the dialog already confirmed any overwrite.
  let mut expected = if same_file {
    point.expected_stamp.clone()
  } else {
    None
  };

  let stamp = loop {
    let (target, snapshot, has_bom, check) = (
      path.clone(),
      point.snapshot.clone(),
      point.has_bom,
      expected.clone(),
    );
    match blocking(move || save_text_file(&target, &snapshot, has_bom, check.as_ref())).await? {
      Ok(stamp) => break stamp,
      Err(FileError::ModifiedOnDisk { .. }) if expected.is_some() => {
        tracing::warn!(document = %id, path = %path.display(), "file changed on disk before save");
        if !dialogs::confirm_overwrite(window, &title).await {
          return Ok(None);
        }
        expected = None;
      }
      Err(error) => return Err(error.into()),
    }
  };

  let mut editor = state.lock();
  let document = editor.document_mut(id)?;
  document.mark_saved(&point, paths::normalize(&path), stamp);
  tracing::info!(document = %id, path = %path.display(), bytes = point.snapshot.len(), "saved file");
  Ok(Some(DocumentSummary::of(document)))
}

async fn blocking<T: Send + 'static>(
  f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CommandError> {
  tauri::async_runtime::spawn_blocking(f)
    .await
    .map_err(CommandError::internal)
}
