//! Ferrite's extension host: loads WebAssembly extensions and runs them in
//! a sandbox.
//!
//! An extension is a folder with an `extension.toml` manifest and a
//! WebAssembly component (or a core module built with wit-bindgen, which
//! is turned into a component on load) implementing the `extension` world
//! of `ferrite-extension-api`. Its only capabilities are the functions of
//! that interface: there is no WASI, so no filesystem, network,
//! environment, or clock access.
//!
//! - **Permissions** requested in the manifest must be granted by the user
//!   before the extension starts, and are checked on every call.
//! - **Each extension runs on its own thread** with a bounded event queue;
//!   the editor only ever queues events, so a slow extension can't block
//!   typing.
//! - **Limits:** memory per instance, a deadline per call (interrupting
//!   infinite loops), stack depth, sizes of reads, edits, diagnostics, and
//!   output. A failing extension is stopped and reported; the editor
//!   carries on.
//!
//! The embedder (the app) implements [`Embedder`] to give the host access
//! to the editor core and to receive [`Notification`]s.

mod api;
mod config;
pub mod contributions;
mod host;
pub mod icons;
pub mod keys;
pub mod manifest;
mod permissions;
pub mod preview;
mod runtime;
mod usage;
mod worker;

/// Host bindings generated from the WIT interface.
mod bindings {
  wasmtime::component::bindgen!({
    path: "../ferrite-extension-api/wit",
    world: "extension",
    additional_derives: [PartialEq],
  });
}

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

pub use api::{MAX_DIAGNOSTICS, MAX_EDITS, MAX_READ_BYTES, MAX_READ_LINES};
pub use config::{ConfigProperty, ConfigType, ConfigValue};
pub use contributions::Contributions;
pub use host::{CommandTicket, ExtensionHost};
pub use manifest::{MANIFEST_FILE, Manifest, ManifestError};
pub use permissions::{Decision, Permission};
pub use preview::{PreviewContent, PreviewInfo};
pub use usage::ResourceUsage;

/// What the host needs from the application.
pub trait Embedder: Send + Sync + 'static {
  /// Runs `f` with the editor locked. Called from extension threads.
  fn with_editor(&self, f: &mut dyn FnMut(&mut ferrite_core::Editor));
  /// The language identifier for a file, e.g. `rust`.
  fn language_of(&self, path: &Path) -> Option<String>;
  /// Receives notifications. Must not block or call back into the host.
  fn notify(&self, notification: Notification);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageLevel {
  Info,
  Warning,
  Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notification {
  /// An extension asked to show a message.
  Message {
    extension: String,
    level: MessageLevel,
    message: String,
  },
  /// An extension changed a document's text.
  DocumentEdited(ferrite_core::DocumentId),
  /// A document's extension diagnostics changed.
  DiagnosticsChanged(ferrite_core::DocumentId),
  /// Extensions were loaded, started, stopped, or changed state.
  ExtensionsChanged,
  /// An extension failed and was stopped.
  ExtensionFailed {
    id: String,
    name: String,
    reason: String,
  },
  /// Status bar items changed.
  StatusItemsChanged,
  /// The content of a view changed (by full view id).
  ViewChanged(String),
  /// An extension asked to show one of its views.
  ShowView(String),
  /// A document's extension decorations changed.
  DecorationsChanged(ferrite_core::DocumentId),
  /// An extension asked to select and show a range.
  Reveal {
    document: ferrite_core::DocumentId,
    start: ferrite_text::Utf16Position,
    end: ferrite_text::Utf16Position,
  },
  /// Previews were opened or closed, or one's title changed.
  PreviewsChanged,
  /// The content of a preview changed (by preview id).
  PreviewChanged(String),
  /// An extension asked to show one of its previews, beside its document
  /// or in the same group.
  ShowPreview { id: String, beside: bool },
}

/// What a status bar item shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusContent {
  pub text: String,
  pub tooltip: Option<String>,
  /// The extension command run on click.
  pub command: Option<String>,
}

/// A status bar item to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusItem {
  /// `<extension id>.<item id>`.
  pub id: String,
  /// The extension's name.
  pub extension: String,
  pub alignment: contributions::Alignment,
  pub priority: i32,
  #[serde(flatten)]
  pub content: StatusContent,
}

/// A row of an extension view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewEntry {
  pub id: String,
  pub parent: Option<String>,
  pub label: String,
  pub description: Option<String>,
  pub tooltip: Option<String>,
  pub expanded: bool,
  pub command: Option<String>,
  pub argument: Option<String>,
}

/// The content of an extension view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewContent {
  pub message: Option<String>,
  pub items: Vec<ViewEntry>,
}

/// A range decorated by an extension, in the current text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionDecoration {
  pub range: Range<usize>,
  /// The decoration type's key, `<extension id>/<name>`.
  pub key: String,
}

/// Resource limits for each extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
  /// Linear memory per instance.
  pub memory_bytes: usize,
  /// Linear memory of all running extensions together.
  pub total_memory_bytes: usize,
  /// Instantiating and activating.
  pub activation_time: Duration,
  /// Running one command.
  pub command_time: Duration,
  /// Handling one document notification, and deactivating.
  pub event_time: Duration,
  /// Events waiting for an extension.
  pub queue_capacity: usize,
}

impl Default for Limits {
  fn default() -> Self {
    Self {
      memory_bytes: 128 * 1024 * 1024,
      total_memory_bytes: 512 * 1024 * 1024,
      activation_time: Duration::from_secs(5),
      command_time: Duration::from_secs(10),
      event_time: Duration::from_secs(1),
      queue_capacity: 1_024,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostOptions {
  /// Checked against manifests' `engines.ferrite` requirement.
  pub editor_version: semver::Version,
  /// Installed extensions: each subfolder with a manifest.
  pub extensions_dir: Option<PathBuf>,
  /// Development extensions: folders with a manifest, loaded in place.
  pub development: Vec<PathBuf>,
  /// Where the user's permission decisions are kept.
  pub permissions_file: Option<PathBuf>,
  /// User configuration, by extension id.
  pub settings: BTreeMap<String, toml::Table>,
  pub limits: Limits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
  Error,
  Warning,
  Information,
  Hint,
}

impl Severity {
  /// The LSP number: 1 (error) to 4 (hint).
  pub fn number(self) -> u8 {
    match self {
      Self::Error => 1,
      Self::Warning => 2,
      Self::Information => 3,
      Self::Hint => 4,
    }
  }
}

/// A diagnostic published by an extension, in the current text.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionDiagnostic {
  pub range: Range<usize>,
  pub severity: Severity,
  pub message: String,
  pub code: Option<String>,
  /// The extension's name.
  pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionState {
  /// Loaded; starts on its activation event.
  Inactive,
  /// Waiting for the user to allow or deny requested permissions.
  AwaitingPermission,
  Starting,
  Active,
  /// Stopped after a failure (see `error`).
  Failed,
  /// Turned off by the user.
  Disabled,
  /// The manifest or folder is invalid (see `error`).
  Invalid,
  /// Contributes only data (themes, languages); there is no code to run.
  Ready,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
  pub permission: Permission,
  pub description: String,
  pub granted: bool,
  /// Whether the user has decided (granted or denied).
  pub decided: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
  pub id: String,
  pub title: String,
  pub category: String,
  pub hidden: bool,
}

/// The settings an extension declares, for a settings editor.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionConfiguration {
  pub id: String,
  pub name: String,
  /// By key.
  pub settings: Vec<ConfiguredSetting>,
}

/// A declared setting and the value in effect.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfiguredSetting {
  pub key: String,
  pub property: ConfigProperty,
  /// The user's value if it's valid, the default otherwise.
  pub value: ConfigValue,
  /// Why the user's value isn't used, if they set an invalid one.
  pub problem: Option<String>,
}

/// An extension and its state, for display.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStatus {
  pub id: String,
  pub name: String,
  pub version: String,
  pub description: String,
  /// The extension's folder.
  pub path: String,
  pub development: bool,
  pub state: ExtensionState,
  pub error: Option<String>,
  pub permissions: Vec<PermissionStatus>,
  pub commands: Vec<CommandInfo>,
  /// Problems with the user's configuration values.
  pub config_errors: Vec<String>,
  /// Whether it declares settings (see [`ExtensionHost::configurations`]).
  pub has_settings: bool,
  pub contributions: Contributions,
  pub usage: ResourceUsage,
}
