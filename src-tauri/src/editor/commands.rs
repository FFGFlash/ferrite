//! Document and editing commands.
//!
//! Lifecycle commands (`document_*`) may show dialogs and touch the disk, so
//! they are async and never hold the editor lock across an await. Editing
//! commands (`editor_*`) are fast and synchronous; each returns the updated
//! view for the caller's viewport, so a keystroke costs one round trip and
//! transfers only visible lines.

use std::path::Path;

use ferrite_core::{CoreError, Document, DocumentId, EditorOp};
use ferrite_text::SearchQuery;
use ferrite_workspace::{FileError, load_text_file, save_text_file};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_clipboard_manager::ClipboardExt;

use super::EditorState;
use super::minimap::{MinimapDto, Snapshot};
use super::protocol::{
  DocumentSummary, EditorView, KnownDto, MAX_OPS_PER_BATCH, OpContext, OpDto, SearchQueryDto,
  SearchView, Viewport, same_text,
};
use super::{fold_memory, folding};
use crate::backups::BackupState;
use crate::dialogs::{self, UnsavedChoice};
use crate::error::CommandError;
use crate::extensions::ExtensionsState;
use crate::languages::LanguagesState;
use crate::lsp::LspState;
use crate::paths;
use crate::perf::Perf;
use crate::{settings, ui_state};

#[tauri::command]
pub fn document_list(state: State<'_, EditorState>) -> Vec<DocumentSummary> {
  state.lock().documents().map(DocumentSummary::of).collect()
}

/// Creates an untitled document, optionally containing `text` (e.g. the
/// keyboard shortcuts list), inserted as its first edit.
#[tauri::command]
pub fn document_new(
  app: AppHandle,
  state: State<'_, EditorState>,
  text: Option<String>,
) -> Result<DocumentSummary, CommandError> {
  let mut editor = state.lock();
  let id = editor.create_untitled();
  tracing::info!(document = %id, "created untitled document");
  if let Some(text) = text.filter(|text| !text.is_empty()) {
    let document = editor.document_mut(id)?;
    document.apply(EditorOp::Insert(text))?;
    document.apply(EditorOp::SetCursor {
      offset: 0,
      extend: false,
      add: false,
    })?;
  }
  let document = editor.document(id)?;
  app.state::<ExtensionsState>().sync(document);
  Ok(DocumentSummary::of(document))
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
  let state = window.state::<EditorState>();
  let closed = state.lock().close(id)?;
  fold_memory::remember(window.app_handle(), &closed);
  fold_memory::forget(window.app_handle(), id);
  window.state::<BackupState>().forget(id);
  state.counts.forget(id);
  state.syntax.forget(id);
  state.regions.forget(id);
  window.state::<LspState>().close(&closed);
  window.state::<ExtensionsState>().closed(id);
  tracing::info!(document = %id, "closed document");
  ui_state::record_open_files(window.app_handle());
  Ok(true)
}

/// Syncs the document with its language server and builds a view,
/// including find matches when `search` is given, syntax highlights, fold
/// markers, and diagnostics. Match counts for large documents and syntax
/// parses that take too long are scheduled in the background. Folds
/// remembered for the document are restored first, once they can be.
fn build_view(
  app: &AppHandle,
  document: &mut Document,
  viewport: Viewport,
  known: Option<KnownDto>,
  search: Option<SearchQueryDto>,
) -> EditorView {
  let state = app.state::<EditorState>();
  let lsp = app.state::<LspState>();
  let extensions = app.state::<ExtensionsState>();
  fold_memory::restore(app, document);
  let document = &*document;
  lsp.sync(app, document);
  extensions.sync(document);
  app.state::<BackupState>().touch();
  let mut view = EditorView::of(
    document,
    viewport,
    known.as_ref(),
    |start, end| {
      let query = SearchQuery::from(search?);
      let counted = state.counts.get(document.id());
      let (view, needs_count) = SearchView::build(document, &query, start, end, counted.as_deref());
      if needs_count {
        state.counts.schedule(app, document, &query);
      }
      Some(view)
    },
    |ranges, chunks| {
      state
        .syntax
        .highlights(app, document, ranges, known.as_ref(), chunks)
    },
  );
  let visible: Vec<_> = view
    .visible
    .iter()
    .map(|&[start, end]| start..end)
    .collect();
  view.foldable = folding::foldable_lines(app, document, &visible);
  let lines = view.start_line..view.end_line;
  (view.diagnostics, view.language) = lsp.view(app, document, lines.clone());
  view.decorations = extensions.add_to_view(document, lines, &mut view.diagnostics);
  view
}

/// The view for `viewport`. `known` describes the lines the client already
/// holds, so only missing lines are sent. With `search`, the view includes
/// find matches.
#[tauri::command]
pub fn editor_view(
  app: AppHandle,
  state: State<'_, EditorState>,
  perf: State<'_, Perf>,
  id: u64,
  viewport: Viewport,
  known: Option<KnownDto>,
  search: Option<SearchQueryDto>,
) -> Result<EditorView, CommandError> {
  let _timer = perf.metrics.time("editor.view");
  let mut editor = state.lock();
  let document = editor.document_mut(DocumentId::from_raw(id))?;
  Ok(build_view(&app, document, viewport, known, search))
}

/// The minimap for display rows `start_row..end_row`, clamped to the
/// document and to `MAX_MINIMAP_LINES`. Runs on a worker thread, and
/// highlights from a snapshot after releasing the editor lock, so editing
/// never waits for it.
#[tauri::command]
pub async fn editor_minimap(
  app: AppHandle,
  id: u64,
  start_row: usize,
  end_row: usize,
) -> Result<MinimapDto, CommandError> {
  tauri::async_runtime::spawn_blocking(move || {
    let started = std::time::Instant::now();
    let state = app.state::<EditorState>();
    let snapshot = {
      let editor = state.lock();
      let document = editor.document(DocumentId::from_raw(id))?;
      Snapshot::of(
        document,
        state.syntax.tree(&app, document),
        start_row..end_row,
        |lines| folding::section_headers(&app, document, lines),
      )
    };
    let perf = app.state::<Perf>();
    perf.record("editor.minimap.lock", started.elapsed());
    let minimap = snapshot.build();
    perf.record("editor.minimap", started.elapsed());
    Ok(minimap)
  })
  .await
  .map_err(CommandError::internal)?
}

/// Applies a batch of operations in order and returns the resulting view.
/// Find operations with an invalid pattern are skipped; the view's search
/// section reports the problem.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn editor_apply(
  app: AppHandle,
  state: State<'_, EditorState>,
  perf: State<'_, Perf>,
  id: u64,
  ops: Vec<OpDto>,
  viewport: Viewport,
  known: Option<KnownDto>,
  search: Option<SearchQueryDto>,
) -> Result<EditorView, CommandError> {
  let _timer = perf.metrics.time("editor.apply");
  if ops.len() > MAX_OPS_PER_BATCH {
    return Err(CommandError::new(
      "tooManyOps",
      format!("At most {MAX_OPS_PER_BATCH} operations can be applied at once."),
    ));
  }
  let mut editor = state.lock();
  let document = editor.document_mut(DocumentId::from_raw(id))?;
  let mut context = op_context(&app, &state, document, &ops);
  let changes_folds = ops.iter().any(OpDto::changes_folds);
  for op in ops {
    // Looked up for each fold operation, as the ops before it may have
    // moved the cursors.
    if let Some(scope) = folding::scope(&op, document) {
      context.folds = folding::regions(&app, document, scope);
    }
    let op = op.into_core(document.buffer(), &context);
    match document.apply(op) {
      Ok(_) | Err(CoreError::Search(_)) => {}
      Err(error) => return Err(error.into()),
    }
  }
  if changes_folds {
    fold_memory::remember(&app, document);
  }
  Ok(build_view(&app, document, viewport, known, search))
}

/// The context the operations in `ops` need, looked up only if they need it.
fn op_context(
  app: &AppHandle,
  state: &EditorState,
  document: &Document,
  ops: &[OpDto],
) -> OpContext {
  let mut context = OpContext::default();
  if ops.iter().any(|op| matches!(op, OpDto::ToggleComment))
    && let Some(path) = document.path()
  {
    context.comments = app.state::<LanguagesState>().comments_for(path);
  }
  if ops.iter().any(|op| matches!(op, OpDto::Paste { .. })) {
    context.copied_lines = state.copied_lines();
  }
  context
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardAction {
  Copy,
  Cut,
  Paste,
}

/// Copies (or cuts) the selected text to the system clipboard, or pastes
/// from it. With nothing selected, copies (or cuts) the cursors' whole
/// lines, which then paste as whole lines. The text goes straight between
/// the backend and the clipboard without crossing IPC. (Keyboard paste
/// arrives as a `paste` operation instead.)
#[tauri::command]
pub fn editor_clipboard(
  app: AppHandle,
  state: State<'_, EditorState>,
  id: u64,
  action: ClipboardAction,
  viewport: Viewport,
  known: Option<KnownDto>,
  search: Option<SearchQueryDto>,
) -> Result<EditorView, CommandError> {
  if let ClipboardAction::Paste = action {
    // Read before taking the editor lock: the clipboard can be slow.
    let text = app
      .clipboard()
      .read_text()
      .map_err(|error| CommandError::new("clipboard", format!("Could not paste: {error}")))?;
    let whole_lines = state
      .copied_lines()
      .is_some_and(|copied| same_text(&copied, &text));
    let mut editor = state.lock();
    let document = editor.document_mut(DocumentId::from_raw(id))?;
    if !text.is_empty() {
      document.apply(EditorOp::Paste { text, whole_lines })?;
    }
    return Ok(build_view(&app, document, viewport, known, search));
  }
  let mut editor = state.lock();
  let document = editor.document_mut(DocumentId::from_raw(id))?;
  let (text, whole_lines) = match document.selected_text() {
    Some(text) => (text, false),
    None => (document.cursor_lines_text(), true),
  };
  let remembered = whole_lines.then(|| text.clone());
  app
    .clipboard()
    .write_text(text)
    .map_err(|error| CommandError::new("clipboard", format!("Could not copy: {error}")))?;
  state.set_copied_lines(remembered);
  if let ClipboardAction::Cut = action {
    document.apply(if whole_lines {
      EditorOp::DeleteLines
    } else {
      EditorOp::DeleteSelection
    })?;
  }
  Ok(build_view(&app, document, viewport, known, search))
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
  let started = std::time::Instant::now();
  let loaded = blocking(move || load_text_file(&to_load)).await??;
  app
    .state::<Perf>()
    .record("document.load", started.elapsed());

  let mut editor = state.lock();
  // Another request may have opened the same file while this one was loading.
  if let Some(id) = editor.find_by_path(&path) {
    return Ok(DocumentSummary::of(editor.document(id)?));
  }
  let bytes = loaded.text.len();
  let id = editor.open(path.clone(), loaded);
  tracing::info!(document = %id, path = %path.display(), bytes, "opened file");
  let document = editor.document(id)?;
  fold_memory::recall(app, document);
  app.state::<ExtensionsState>().sync(document);
  let summary = DocumentSummary::of(document);
  drop(editor);
  ui_state::record_open_files(app);
  Ok(summary)
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
    let started = std::time::Instant::now();
    let result =
      blocking(move || save_text_file(&target, &snapshot, has_bom, check.as_ref())).await?;
    window
      .state::<Perf>()
      .record("document.save", started.elapsed());
    match result {
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

  let saved_path = paths::normalize(&path);
  let summary = {
    let mut editor = state.lock();
    let document = editor.document_mut(id)?;
    document.mark_saved(&point, saved_path.clone(), stamp);
    window.state::<LspState>().saved(document);
    fold_memory::remember(window.app_handle(), document);
    tracing::info!(document = %id, path = %path.display(), bytes = point.snapshot.len(), "saved file");
    DocumentSummary::of(document)
  };
  let app = window.app_handle();
  app.state::<BackupState>().touch();
  if !same_file {
    ui_state::record_open_files(app);
  }
  if settings::is_settings_file(app, &saved_path) {
    // The file is saved either way; report invalid settings to the user.
    settings::reload(app)?;
  }
  Ok(Some(summary))
}

async fn blocking<T: Send + 'static>(
  f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CommandError> {
  tauri::async_runtime::spawn_blocking(f)
    .await
    .map_err(CommandError::internal)
}
