//! Backend-to-frontend notifications.
//!
//! The frontend subscribes once with a Tauri `Channel`; events are pushed
//! through it. A channel is part of IPC itself, so no event permissions are
//! granted to the webview. A new subscription (e.g. after a reload) replaces
//! the old one.

use std::sync::{Mutex, PoisonError};

use ferrite_extension_host::MessageLevel;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::editor::protocol::DocumentSummary;
use crate::settings::FrontendSettings;

#[derive(Debug, Clone, Serialize)]
#[serde(
  tag = "type",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
pub enum AppEvent {
  /// A workspace folder was opened or closed.
  WorkspaceChanged {
    workspace: Option<WorkspaceInfo>,
  },
  /// Listings of these directories may have changed.
  FilesChanged {
    directories: Vec<String>,
  },
  /// A clean document was reloaded because its file changed on disk.
  DocumentReloaded {
    document: DocumentSummary,
  },
  /// A document with unsaved changes was modified on disk by another program.
  DocumentChangedOnDisk {
    document: DocumentSummary,
  },
  SettingsChanged {
    settings: FrontendSettings,
  },
  /// A background match count for a large document finished.
  SearchCounted {
    document_id: u64,
  },
  /// A document's diagnostics changed.
  DiagnosticsChanged {
    document_id: u64,
  },
  /// A language server started, stopped, or began or finished work.
  LanguageServerChanged,
  /// A background parse finished, so a document's highlighting may have
  /// changed.
  SyntaxUpdated {
    document_id: u64,
  },
  /// A file was opened in the background (e.g. at startup).
  DocumentOpened {
    document: DocumentSummary,
    /// Whether it should become the active tab.
    active: bool,
  },
  /// Progress of the Go to File index.
  IndexStatus {
    indexing: bool,
    files: usize,
    truncated: bool,
  },
  /// Extensions were loaded or changed state.
  ExtensionsChanged,
  /// An extension asked to show a message.
  ExtensionMessage {
    extension: String,
    level: MessageLevel,
    message: String,
  },
  /// An extension failed and was stopped.
  ExtensionFailed {
    id: String,
    name: String,
    reason: String,
  },
  /// An extension edited a document.
  DocumentEdited {
    document_id: u64,
  },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
  pub root: String,
  pub name: String,
}

#[derive(Default)]
pub struct EventHub(Mutex<Option<Channel<AppEvent>>>);

impl EventHub {
  pub fn emit(&self, event: AppEvent) {
    let channel = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(channel) = channel.as_ref()
      && let Err(error) = channel.send(event)
    {
      tracing::debug!(%error, "event not delivered");
    }
  }
}

/// Sends `event` to the frontend, if it has subscribed.
pub fn emit(app: &AppHandle, event: AppEvent) {
  app.state::<EventHub>().emit(event);
}

#[tauri::command]
pub fn events_subscribe(hub: State<'_, EventHub>, channel: Channel<AppEvent>) {
  *hub.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(channel);
}
