//! Language servers for open documents: starting them on demand, keeping
//! documents in sync, diagnostics, and (in `commands`) hover, go to
//! definition and completion.
//!
//! A document of a language with a configured server is opened on the
//! server for its workspace root the first time it is viewed; the server is
//! started then if needed, on its own thread. Every view syncs the document
//! first, so the server sees each edit right after it is applied (as an
//! incremental change built from the buffer's edit log). Diagnostics are
//! stored as byte ranges in the text they were computed for and mapped
//! through later edits when shown. A server that exits is restarted on
//! demand a few times, then given up on.
//!
//! Lock order: the editor lock, then this state's lock. Nothing blocks on a
//! server while holding either: notifications are queued, and requests are
//! made after the locks are released.

mod commands;

use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use ferrite_core::{Document, DocumentId};
use ferrite_lsp::convert::{content_changes, full_change, path_to_uri, to_offsets, uri_to_path};
use ferrite_lsp::lsp_types::notification::{
  DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
};
use ferrite_lsp::lsp_types::{
  DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
  DidOpenTextDocumentParams, DidSaveTextDocumentParams, NumberOrString, PublishDiagnosticsParams,
  TextDocumentIdentifier, TextDocumentItem, TextDocumentSyncKind, Uri,
  VersionedTextDocumentIdentifier,
};
use ferrite_lsp::{LanguageServer, ServerConfig, ServerEvent, find_root};
use ferrite_syntax::LanguageId;
use ferrite_text::{TextBuffer, TextSnapshot, Version, map_offset_through};
use serde::Serialize;
use tauri::{AppHandle, Manager};

pub use commands::*;

use crate::editor::EditorState;
use crate::editor::protocol::Position;
use crate::events::{self, AppEvent};
use crate::settings::SettingsState;
use crate::workspace::WorkspaceState;

/// Larger documents are not sent to language servers.
const MAX_LSP_BYTES: usize = 8 * 1024 * 1024;
/// Automatic restarts of a server that keeps exiting.
const MAX_RESTARTS: u32 = 3;
/// Synced versions kept to interpret diagnostics computed for older text.
const SYNC_HISTORY: usize = 16;
/// Most diagnostics included in one view.
const MAX_VIEW_DIAGNOSTICS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ServerKey {
  name: String,
  root: PathBuf,
}

enum Slot {
  /// Not running; started when a document needs it.
  Idle,
  Starting,
  Running(Arc<LanguageServer>),
  /// Not installed, failed to start, or exited too often.
  Failed(String),
}

struct ServerEntry {
  slot: Slot,
  restarts: u32,
  /// Titles of work in progress (e.g. indexing), by progress token.
  progress: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct Diagnostic {
  range: Range<usize>,
  severity: u8,
  message: String,
  source: Option<String>,
}

/// A document open on a server.
struct OpenDocument {
  key: ServerKey,
  server: Arc<LanguageServer>,
  uri: Uri,
  path: PathBuf,
  lsp_version: i32,
  /// Texts recently sent, by LSP version, oldest first; the last is what
  /// the server has now.
  history: VecDeque<(i32, TextSnapshot)>,
  /// Diagnostics as byte ranges in the text at `diagnostics_version`.
  diagnostics: Vec<Diagnostic>,
  diagnostics_version: Version,
}

impl OpenDocument {
  fn synced(&self) -> &TextSnapshot {
    // The history is never empty: opening records the first version.
    &self.history[self.history.len() - 1].1
  }

  /// Sends the edits since the last sync.
  fn send_changes(&mut self, buffer: &TextBuffer) {
    if self.synced().version() == buffer.version() {
      return;
    }
    let encoding = self.server.encoding();
    let changes = match (
      self.server.sync_kind(),
      buffer.edits_since(self.synced().version()),
    ) {
      (None, _) => Vec::new(),
      (Some(TextDocumentSyncKind::INCREMENTAL), Some(edits)) => {
        content_changes(self.synced(), &edits, encoding)
      }
      _ => vec![full_change(buffer.contents())],
    };
    self.lsp_version += 1;
    if !changes.is_empty() {
      self
        .server
        .notify::<DidChangeTextDocument>(DidChangeTextDocumentParams {
          text_document: VersionedTextDocumentIdentifier::new(self.uri.clone(), self.lsp_version),
          content_changes: changes,
        });
    }
    if self.history.len() == SYNC_HISTORY {
      self.history.pop_front();
    }
    self
      .history
      .push_back((self.lsp_version, buffer.snapshot()));
  }

  /// Maps the diagnostics through the edits since they were stored.
  fn catch_up(&mut self, buffer: &TextBuffer) {
    if self.diagnostics_version == buffer.version() {
      return;
    }
    match buffer.edits_since(self.diagnostics_version) {
      Some(edits) => {
        for diagnostic in &mut self.diagnostics {
          let start = map_offset_through(&edits, diagnostic.range.start, true);
          let end = map_offset_through(&edits, diagnostic.range.end, false).max(start);
          diagnostic.range = start..end;
        }
      }
      // Too old to map; the server will send fresh ones.
      None => self.diagnostics.clear(),
    }
    self.diagnostics_version = buffer.version();
  }

  fn close(&self) {
    if self.server.is_alive() {
      self
        .server
        .notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
          text_document: TextDocumentIdentifier::new(self.uri.clone()),
        });
    }
  }
}

/// The server a document should be open on.
struct Wanted {
  key: ServerKey,
  config: ServerConfig,
}

#[derive(Default)]
struct Inner {
  /// Configured servers; loaded from settings on first use.
  configs: Option<Vec<ServerConfig>>,
  servers: HashMap<ServerKey, ServerEntry>,
  documents: HashMap<DocumentId, OpenDocument>,
  /// Documents waiting for their server to start (or that cannot have one
  /// because it failed), with the path they were checked for.
  waiting: HashMap<DocumentId, (PathBuf, ServerKey)>,
  shutting_down: bool,
}

#[derive(Default)]
pub struct LspState(Mutex<Inner>);

/// A diagnostic in a view.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDto {
  pub start: Position,
  pub end: Position,
  /// 1 error, 2 warning, 3 information, 4 hint.
  pub severity: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatusDto {
  pub name: String,
  /// `starting`, `ready`, `busy`, or `failed`.
  pub state: &'static str,
  pub message: Option<String>,
}

/// Language features for a view's document, for the status bar and editor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageStatusDto {
  pub language: &'static str,
  pub server: Option<ServerStatusDto>,
  pub errors: usize,
  pub warnings: usize,
  /// Characters that open completion; `None` without completion support.
  pub completion_triggers: Option<Vec<String>>,
}

fn severity(severity: Option<DiagnosticSeverity>) -> u8 {
  match severity {
    Some(DiagnosticSeverity::WARNING) => 2,
    Some(DiagnosticSeverity::INFORMATION) => 3,
    Some(DiagnosticSeverity::HINT) => 4,
    // Servers may omit it; clients are to treat that as an error.
    _ => 1,
  }
}

impl LspState {
  fn lock(&self) -> MutexGuard<'_, Inner> {
    self.0.lock().unwrap_or_else(PoisonError::into_inner)
  }

  fn wanted(&self, app: &AppHandle, inner: &mut Inner, path: &Path) -> Option<Wanted> {
    let language = LanguageId::from_path(path)?;
    let configs = inner
      .configs
      .get_or_insert_with(|| app.state::<SettingsState>().get().language_servers());
    let config = configs
      .iter()
      .find(|config| config.languages.iter().any(|id| id == language.id()))?
      .clone();
    let workspace = app.state::<WorkspaceState>().root();
    let root = find_root(path, &config.root_markers, workspace.as_deref());
    Some(Wanted {
      key: ServerKey {
        name: config.name.clone(),
        root,
      },
      config,
    })
  }

  /// Brings the server's copy of `document` up to date, opening it (and
  /// starting its server) if needed. Called with the editor locked.
  pub fn sync(&self, app: &AppHandle, document: &Document) {
    let id = document.id();
    let buffer = document.buffer();
    let mut inner = self.lock();
    if inner.shutting_down {
      return;
    }
    let path = document.path();
    if let Some(open) = inner.documents.get_mut(&id) {
      if Some(open.path.as_path()) == path && open.server.is_alive() {
        open.send_changes(buffer);
        return;
      }
      // Saved under another name, or its server exited.
      if let Some(open) = inner.documents.remove(&id) {
        open.close();
      }
    }
    let Some(path) = path.filter(|_| buffer.len() <= MAX_LSP_BYTES) else {
      inner.waiting.remove(&id);
      return;
    };
    let key = match inner.waiting.get(&id) {
      Some((waited, key)) if waited == path => key.clone(),
      _ => {
        let Some(wanted) = self.wanted(app, &mut inner, path) else {
          inner.waiting.remove(&id);
          return;
        };
        let key = wanted.key.clone();
        let entry = inner
          .servers
          .entry(key.clone())
          .or_insert_with(|| ServerEntry {
            slot: Slot::Idle,
            restarts: 0,
            progress: HashMap::new(),
          });
        if let Slot::Idle = entry.slot {
          entry.slot = Slot::Starting;
          start_server(app, wanted.key, wanted.config);
          events::emit(app, AppEvent::LanguageServerChanged);
        }
        inner.waiting.insert(id, (path.to_path_buf(), key.clone()));
        key
      }
    };
    let Some(ServerEntry {
      slot: Slot::Running(server),
      ..
    }) = inner.servers.get(&key)
    else {
      return;
    };
    let Some(uri) = path_to_uri(path) else {
      return;
    };
    let server = server.clone();
    let language = LanguageId::from_path(path).map_or("plaintext", LanguageId::id);
    server.notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
      text_document: TextDocumentItem::new(uri.clone(), language.into(), 1, buffer.contents()),
    });
    inner.waiting.remove(&id);
    inner.documents.insert(
      id,
      OpenDocument {
        key,
        server,
        uri,
        path: path.to_path_buf(),
        lsp_version: 1,
        history: VecDeque::from([(1, buffer.snapshot())]),
        diagnostics: Vec::new(),
        diagnostics_version: buffer.version(),
      },
    );
  }

  /// Tells the server a document was saved.
  pub fn saved(&self, document: &Document) {
    let mut inner = self.lock();
    if let Some(open) = inner.documents.get_mut(&document.id()) {
      open.send_changes(document.buffer());
      let text = open
        .server
        .wants_text_on_save()
        .then(|| document.buffer().contents());
      open
        .server
        .notify::<DidSaveTextDocument>(DidSaveTextDocumentParams {
          text_document: TextDocumentIdentifier::new(open.uri.clone()),
          text,
        });
    }
  }

  /// Closes a document on its server.
  pub fn close(&self, id: DocumentId) {
    let mut inner = self.lock();
    inner.waiting.remove(&id);
    if let Some(open) = inner.documents.remove(&id) {
      open.close();
    }
  }

  /// Diagnostics on `lines` and the language status, for a view. Called with
  /// the editor locked, after [`Self::sync`].
  pub fn view(
    &self,
    document: &Document,
    lines: Range<usize>,
  ) -> (Vec<DiagnosticDto>, Option<LanguageStatusDto>) {
    let Some(language) = document.path().and_then(LanguageId::from_path) else {
      return (Vec::new(), None);
    };
    let buffer = document.buffer();
    let mut inner = self.lock();
    let mut status = LanguageStatusDto {
      language: language.display_name(),
      server: None,
      errors: 0,
      warnings: 0,
      completion_triggers: None,
    };
    let id = document.id();
    let key = match (inner.documents.get(&id), inner.waiting.get(&id)) {
      (Some(open), _) => open.key.clone(),
      (None, Some((_, key))) => key.clone(),
      (None, None) => return (Vec::new(), Some(status)),
    };
    if let Some(entry) = inner.servers.get(&key) {
      let (state, message) = match &entry.slot {
        Slot::Idle | Slot::Starting => ("starting", None),
        Slot::Failed(message) => ("failed", Some(message.clone())),
        Slot::Running(_) => match entry.progress.values().next() {
          Some(title) => ("busy", Some(title.clone())),
          None => ("ready", None),
        },
      };
      status.server = Some(ServerStatusDto {
        name: key.name.clone(),
        state,
        message,
      });
    }
    let Some(open) = inner.documents.get_mut(&id) else {
      return (Vec::new(), Some(status));
    };
    status.completion_triggers = open
      .server
      .capabilities()
      .completion_provider
      .as_ref()
      .map(|completion| completion.trigger_characters.clone().unwrap_or_default());
    open.catch_up(buffer);
    status.errors = open.diagnostics.iter().filter(|d| d.severity == 1).count();
    status.warnings = open.diagnostics.iter().filter(|d| d.severity == 2).count();
    let visible = buffer.line_start(lines.start)..buffer.line_end(lines.end.saturating_sub(1));
    let diagnostics = open
      .diagnostics
      .iter()
      .filter(|d| d.range.start <= visible.end && d.range.end >= visible.start)
      .take(MAX_VIEW_DIAGNOSTICS)
      .map(|d| DiagnosticDto {
        start: buffer.offset_to_utf16(d.range.start).into(),
        end: buffer.offset_to_utf16(d.range.end).into(),
        severity: d.severity,
      })
      .collect();
    (diagnostics, Some(status))
  }

  /// Messages of the diagnostics covering `offset`, most severe first.
  fn diagnostics_at(&self, document: &Document, offset: usize) -> Vec<DiagnosticMessageDto> {
    let mut inner = self.lock();
    let Some(open) = inner.documents.get_mut(&document.id()) else {
      return Vec::new();
    };
    open.catch_up(document.buffer());
    let mut found: Vec<DiagnosticMessageDto> = open
      .diagnostics
      .iter()
      .filter(|d| d.range.start <= offset && offset <= d.range.end)
      .map(|d| DiagnosticMessageDto {
        severity: d.severity,
        message: d.message.clone(),
        source: d.source.clone(),
      })
      .collect();
    found.sort_by_key(|d| d.severity);
    found
  }

  fn on_diagnostics(&self, key: &ServerKey, params: PublishDiagnosticsParams) -> Option<u64> {
    let mut inner = self.lock();
    let path = uri_to_path(&params.uri);
    let (&id, open) = inner.documents.iter_mut().find(|(_, open)| {
      open.key == *key
        && (open.uri == params.uri
          || path
            .as_deref()
            .is_some_and(|path| same_path(path, &open.path)))
    })?;
    let snapshot = match params.version {
      Some(version) => &open.history.iter().find(|(v, _)| *v == version)?.1,
      None => open.synced(),
    };
    let text = TextBuffer::from_snapshot(snapshot);
    let version = snapshot.version();
    let encoding = open.server.encoding();
    open.diagnostics = params
      .diagnostics
      .into_iter()
      .map(|diagnostic| Diagnostic {
        range: to_offsets(&text, diagnostic.range, encoding),
        severity: severity(diagnostic.severity),
        message: diagnostic.message,
        source: diagnostic.source,
      })
      .collect();
    open.diagnostics_version = version;
    Some(id.get())
  }

  fn on_progress(&self, key: &ServerKey, params: &serde_json::Value) -> bool {
    let token = match serde_json::from_value::<NumberOrString>(params["token"].clone()) {
      Ok(NumberOrString::Number(number)) => number.to_string(),
      Ok(NumberOrString::String(string)) => string,
      Err(_) => return false,
    };
    let mut inner = self.lock();
    let Some(entry) = inner.servers.get_mut(key) else {
      return false;
    };
    let value = &params["value"];
    match value["kind"].as_str() {
      Some("begin") => {
        let title = value["title"].as_str().unwrap_or("Working").to_owned();
        entry.progress.insert(token, title);
        true
      }
      Some("end") => entry.progress.remove(&token).is_some(),
      _ => false,
    }
  }

  /// Handles a server exiting. Returns the documents that lost their
  /// diagnostics.
  fn on_exit(&self, key: &ServerKey) -> Vec<u64> {
    let mut inner = self.lock();
    if inner.shutting_down {
      return Vec::new();
    }
    let Some(entry) = inner.servers.get_mut(key) else {
      return Vec::new();
    };
    let Slot::Running(server) = &entry.slot else {
      return Vec::new();
    };
    if server.is_alive() {
      // A late event from an earlier instance.
      return Vec::new();
    }
    entry.restarts += 1;
    entry.progress.clear();
    entry.slot = if entry.restarts > MAX_RESTARTS {
      tracing::error!(server = %key.name, "language server keeps exiting; giving up");
      Slot::Failed(format!("{} stopped unexpectedly too many times.", key.name))
    } else {
      tracing::warn!(server = %key.name, restarts = entry.restarts, "language server exited; restarting");
      Slot::Idle
    };
    let affected: Vec<DocumentId> = inner
      .documents
      .iter()
      .filter(|(_, open)| open.key == *key)
      .map(|(&id, _)| id)
      .collect();
    for id in &affected {
      inner.documents.remove(id);
    }
    inner.waiting.retain(|_, (_, waiting)| waiting != key);
    affected.into_iter().map(DocumentId::get).collect()
  }

  /// Stops every server and forgets all documents, so the next views start
  /// over with the current settings.
  pub fn restart_all(&self, app: &AppHandle) {
    let servers = {
      let mut inner = self.lock();
      inner.configs = None;
      inner.documents.clear();
      inner.waiting.clear();
      std::mem::take(&mut inner.servers)
    };
    let app = app.clone();
    spawn(String::from("lsp-restart"), move || {
      for entry in servers.into_values() {
        if let Slot::Running(server) = entry.slot {
          server.shutdown();
        }
      }
      resync_all(&app);
    });
  }

  /// Shuts every server down, waiting for them to exit.
  pub fn shutdown_all(&self) {
    let servers = {
      let mut inner = self.lock();
      inner.shutting_down = true;
      inner.documents.clear();
      std::mem::take(&mut inner.servers)
    };
    let handles: Vec<_> = servers
      .into_values()
      .filter_map(|entry| match entry.slot {
        Slot::Running(server) => Some(thread::spawn(move || server.shutdown())),
        _ => None,
      })
      .collect();
    for handle in handles {
      let _ = handle.join();
    }
  }
}

fn same_path(a: &Path, b: &Path) -> bool {
  if cfg!(windows) {
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
  } else {
    a == b
  }
}

fn spawn(name: String, work: impl FnOnce() + Send + 'static) {
  if let Err(error) = thread::Builder::new().name(name).spawn(work) {
    tracing::error!(%error, "could not start a language server thread");
  }
}

/// Syncs every open document, e.g. to open them on a server that started.
fn resync_all(app: &AppHandle) {
  let state = app.state::<EditorState>();
  let editor = state.lock();
  let lsp = app.state::<LspState>();
  for document in editor.documents() {
    lsp.sync(app, document);
  }
}

fn start_server(app: &AppHandle, key: ServerKey, config: ServerConfig) {
  let app = app.clone();
  spawn(format!("lsp-start-{}", key.name), move || {
    tracing::info!(server = %key.name, root = %key.root.display(), "starting language server");
    let handler = {
      let (app, key) = (app.clone(), key.clone());
      Arc::new(move |event| on_server_event(&app, &key, event))
    };
    let result = LanguageServer::start(&key.name, &config.command, &key.root, handler);
    let lsp = app.state::<LspState>();
    let unwanted = {
      let mut inner = lsp.lock();
      match inner.servers.get_mut(&key) {
        Some(entry) if matches!(entry.slot, Slot::Starting) => {
          entry.slot = match result {
            Ok(server) => Slot::Running(Arc::new(server)),
            Err(error) => {
              tracing::warn!(server = %key.name, %error, "could not start language server");
              Slot::Failed(if error.is_not_found() {
                format!(
                  "{} is not installed (or not on the PATH).",
                  config.command.program
                )
              } else {
                error.to_string()
              })
            }
          };
          None
        }
        // Settings changed or the app is closing.
        _ => result.ok(),
      }
    };
    if let Some(server) = unwanted {
      server.shutdown();
      return;
    }
    events::emit(&app, AppEvent::LanguageServerChanged);
    resync_all(&app);
  });
}

fn on_server_event(app: &AppHandle, key: &ServerKey, event: ServerEvent) {
  let lsp = app.state::<LspState>();
  match event {
    ServerEvent::Notification { method, params } => match method.as_str() {
      "textDocument/publishDiagnostics" => {
        let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(params) else {
          return;
        };
        if let Some(document_id) = lsp.on_diagnostics(key, params) {
          events::emit(app, AppEvent::DiagnosticsChanged { document_id });
        }
      }
      "$/progress" => {
        if lsp.on_progress(key, &params) {
          events::emit(app, AppEvent::LanguageServerChanged);
        }
      }
      "window/showMessage" | "window/logMessage" => {
        let message = params["message"].as_str().unwrap_or_default();
        match params["type"].as_u64() {
          Some(1) => tracing::warn!(server = %key.name, "{message}"),
          _ => tracing::debug!(server = %key.name, "{message}"),
        }
      }
      _ => {}
    },
    ServerEvent::Exited => {
      let affected = lsp.on_exit(key);
      events::emit(app, AppEvent::LanguageServerChanged);
      for document_id in affected {
        events::emit(app, AppEvent::DiagnosticsChanged { document_id });
      }
      // Restart for the documents that were open on it.
      let app = app.clone();
      spawn(String::from("lsp-resync"), move || resync_all(&app));
    }
  }
}

/// A diagnostic's message, for hover.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticMessageDto {
  pub severity: u8,
  pub message: String,
  pub source: Option<String>,
}

/// What requests need from an open document, taken under the locks.
struct Target {
  server: Arc<LanguageServer>,
  uri: Uri,
}

impl LspState {
  fn target(&self, id: DocumentId) -> Option<Target> {
    let inner = self.lock();
    let open = inner.documents.get(&id)?;
    open.server.is_alive().then(|| Target {
      server: open.server.clone(),
      uri: open.uri.clone(),
    })
  }
}
