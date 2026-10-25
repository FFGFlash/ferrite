//! WebAssembly extensions: connects the extension host to the editor, the
//! settings, and the frontend.
//!
//! Installed extensions live in `<config dir>/extensions/<name>/`;
//! development extensions are loaded in place from the folders listed in
//! `extensions.development` in the settings. The user's permission
//! decisions are kept in `<config dir>/extension-permissions.json`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ferrite_core::{Document, DocumentId, Editor};
use ferrite_extension_host::{
  Embedder, ExtensionHost, ExtensionStatus, HostOptions, Limits, MANIFEST_FILE, Notification,
  Permission,
};
use ferrite_syntax::LanguageId;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::editor::EditorState;
use crate::error::CommandError;
use crate::events::{self, AppEvent};
use crate::lsp::{DiagnosticDto, DiagnosticMessageDto, LanguageStatusDto};
use crate::settings::{self, Settings, SettingsState};

const EXTENSIONS_DIR: &str = "extensions";
const PERMISSIONS_FILE: &str = "extension-permissions.json";

/// The extension host, once the app is set up.
#[derive(Default)]
pub struct ExtensionsState(OnceLock<ExtensionHost>);

impl ExtensionsState {
  pub fn host(&self) -> Option<&ExtensionHost> {
    self.0.get()
  }

  /// Announces a new or changed document to extensions. Call with the
  /// editor locked.
  pub fn sync(&self, document: &Document) {
    if let Some(host) = self.host() {
      host.sync(document);
    }
  }

  pub fn closed(&self, id: DocumentId) {
    if let Some(host) = self.host() {
      host.closed(id);
    }
  }

  /// Adds extension diagnostics on `lines` to a view's, and their counts to
  /// the language status. Call with the editor locked.
  pub fn add_to_view(
    &self,
    document: &Document,
    lines: std::ops::Range<usize>,
    diagnostics: &mut Vec<DiagnosticDto>,
    language: Option<&mut LanguageStatusDto>,
  ) {
    let Some(host) = self.host() else {
      return;
    };
    let buffer = document.buffer();
    let visible = buffer.line_start(lines.start)..buffer.line_end(lines.end.saturating_sub(1));
    diagnostics.extend(
      host
        .diagnostics(document, visible)
        .into_iter()
        .map(|diagnostic| DiagnosticDto {
          start: buffer.offset_to_utf16(diagnostic.range.start).into(),
          end: buffer.offset_to_utf16(diagnostic.range.end).into(),
          severity: diagnostic.severity.number(),
        }),
    );
    if let Some(language) = language {
      let (errors, warnings) = host.problem_counts(document);
      language.errors += errors;
      language.warnings += warnings;
    }
  }

  /// Messages of extension diagnostics covering `offset`, for hover.
  pub fn messages_at(&self, document: &Document, offset: usize) -> Vec<DiagnosticMessageDto> {
    self.host().map_or_else(Vec::new, |host| {
      host
        .diagnostics_at(document, offset)
        .into_iter()
        .map(|diagnostic| DiagnosticMessageDto {
          severity: diagnostic.severity.number(),
          message: diagnostic.message,
          source: Some(diagnostic.source),
        })
        .collect()
    })
  }
}

struct AppEmbedder {
  app: AppHandle,
}

impl Embedder for AppEmbedder {
  fn with_editor(&self, f: &mut dyn FnMut(&mut Editor)) {
    let state = self.app.state::<EditorState>();
    let mut editor = state.lock();
    f(&mut editor);
  }

  fn language_of(&self, path: &Path) -> Option<String> {
    LanguageId::from_path(path).map(|language| language.id().to_owned())
  }

  // Called with the editor lock possibly held, so this only emits events.
  fn notify(&self, notification: Notification) {
    let event = match notification {
      Notification::Message {
        extension,
        level,
        message,
      } => AppEvent::ExtensionMessage {
        extension,
        level,
        message,
      },
      Notification::DocumentEdited(id) => AppEvent::DocumentEdited {
        document_id: id.get(),
      },
      Notification::DiagnosticsChanged(id) => AppEvent::DiagnosticsChanged {
        document_id: id.get(),
      },
      Notification::ExtensionsChanged => AppEvent::ExtensionsChanged,
      Notification::ExtensionFailed { id, name, reason } => {
        AppEvent::ExtensionFailed { id, name, reason }
      }
    };
    events::emit(&self.app, event);
  }
}

fn config_dir(app: &AppHandle) -> Option<PathBuf> {
  app.path().app_config_dir().ok()
}

fn options(app: &AppHandle, settings: &Settings) -> HostOptions {
  let editor_version = semver::Version::parse(&app.package_info().version.to_string())
    .unwrap_or_else(|_| semver::Version::new(0, 0, 0));
  let enabled = settings.extensions.enabled;
  let dir = config_dir(app);
  HostOptions {
    editor_version,
    extensions_dir: dir
      .as_ref()
      .filter(|_| enabled)
      .map(|dir| dir.join(EXTENSIONS_DIR)),
    development: if enabled {
      settings
        .extensions
        .development
        .iter()
        .map(PathBuf::from)
        .collect()
    } else {
      Vec::new()
    },
    permissions_file: dir.map(|dir| dir.join(PERMISSIONS_FILE)),
    settings: settings.extensions.settings.clone(),
    limits: Limits::default(),
  }
}

/// Creates the host and loads extensions in the background.
pub fn init(app: &AppHandle, load: bool) {
  let settings = app.state::<SettingsState>().get();
  let host = ExtensionHost::new(
    std::sync::Arc::new(AppEmbedder { app: app.clone() }),
    options(app, &settings),
  );
  let state = app.state::<ExtensionsState>();
  if state.0.set(host).is_err() || !load {
    return;
  }
  // Documents opened before the host existed.
  {
    let editor_state = app.state::<EditorState>();
    let editor = editor_state.lock();
    for document in editor.documents() {
      state.sync(document);
    }
  }
  let app = app.clone();
  let spawned = std::thread::Builder::new()
    .name("extensions-load".into())
    .spawn(move || {
      if let Some(host) = app.state::<ExtensionsState>().host() {
        host.load();
      }
    });
  if let Err(error) = spawned {
    tracing::error!(%error, "could not load extensions");
  }
}

/// Applies changed extension settings.
pub fn settings_changed(app: &AppHandle, previous: &Settings, settings: &Settings) {
  if previous.extensions == settings.extensions {
    return;
  }
  if let Some(host) = app.state::<ExtensionsState>().host() {
    host.configure(options(app, settings));
  }
}

pub fn shutdown(app: &AppHandle) {
  if let Some(host) = app.state::<ExtensionsState>().host() {
    host.shutdown();
  }
}

fn host<'a>(state: &'a State<'_, ExtensionsState>) -> Result<&'a ExtensionHost, CommandError> {
  state
    .host()
    .ok_or_else(|| CommandError::new("extensions", "Extensions are not available."))
}

#[tauri::command]
pub fn extensions_list(state: State<'_, ExtensionsState>) -> Vec<ExtensionStatus> {
  state.host().map_or_else(Vec::new, ExtensionHost::statuses)
}

/// Runs an extension's command for the active document and waits for it.
#[tauri::command]
pub async fn extension_run_command(
  app: AppHandle,
  command: String,
  document_id: Option<u64>,
) -> Result<(), CommandError> {
  let ticket = {
    let state = app.state::<ExtensionsState>();
    host(&state)?
      .run_command(&command, document_id.map(DocumentId::from_raw))
      .map_err(|message| CommandError::new("extension", message))?
  };
  tauri::async_runtime::spawn_blocking(move || ticket.wait())
    .await
    .map_err(CommandError::internal)?
    .map_err(|message| CommandError::new("extension", message))
}

#[tauri::command]
pub fn extension_set_permissions(
  state: State<'_, ExtensionsState>,
  id: String,
  granted: Vec<Permission>,
) -> Result<(), CommandError> {
  tracing::info!(extension = %id, ?granted, "extension permissions decided");
  host(&state)?.set_permissions(&id, &granted);
  Ok(())
}

#[tauri::command]
pub fn extension_set_enabled(
  state: State<'_, ExtensionsState>,
  id: String,
  enabled: bool,
) -> Result<(), CommandError> {
  host(&state)?.set_enabled(&id, enabled);
  Ok(())
}

#[tauri::command]
pub fn extension_restart(
  state: State<'_, ExtensionsState>,
  id: String,
) -> Result<(), CommandError> {
  host(&state)?.restart(&id);
  Ok(())
}

/// Rediscovers extensions and restarts them, e.g. after rebuilding one.
#[tauri::command]
pub async fn extensions_reload(app: AppHandle) -> Result<(), CommandError> {
  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExtensionsState>();
    host(&state).map(ExtensionHost::load)
  })
  .await
  .map_err(CommandError::internal)?
}

/// Asks for an extension folder and adds it to `extensions.development`
/// in the settings, which loads it. `None` if cancelled.
#[tauri::command]
pub async fn extension_add_development(
  app: AppHandle,
  window: WebviewWindow,
) -> Result<Option<String>, CommandError> {
  let Some(folder) = crate::dialogs::pick_folder(&window).await else {
    return Ok(None);
  };
  if !folder.join(MANIFEST_FILE).is_file() {
    return Err(CommandError::new(
      "notAnExtension",
      format!("{} has no {MANIFEST_FILE}.", folder.display()),
    ));
  }
  let folder = crate::paths::normalize(&folder);
  let path = folder.to_string_lossy().into_owned();
  settings::edit(&app, |document| {
    if !document.contains_table("extensions") {
      document["extensions"] = toml_edit::table();
    }
    let list = document["extensions"]["development"]
      .or_insert(toml_edit::value(toml_edit::Array::new()))
      .as_array_mut()
      .ok_or_else(|| "extensions.development must be a list".to_owned())?;
    if !list.iter().any(|item| item.as_str() == Some(path.as_str())) {
      list.push(path.as_str());
    }
    Ok(())
  })?;
  Ok(Some(path))
}
