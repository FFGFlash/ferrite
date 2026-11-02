//! The extension host: discovery, lifecycle, permission decisions, document
//! notifications, extension diagnostics and decorations, and the content of
//! extension status bar items, views, and previews.
//!
//! Lock order: the editor lock (held by the embedder around `sync` and the
//! diagnostics queries, and taken by host functions through
//! [`Embedder::with_editor`]), then the host's state lock. Nothing waits on
//! an extension while holding either; notifications to the embedder are
//! sent after the state lock is released.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, Weak};
use std::time::{Duration, Instant};

use ferrite_core::{Document, DocumentId};
use ferrite_text::{TextBuffer, TextSnapshot, Version, map_offset_through};

use crate::api::{self, document_info, to_position};
use crate::bindings::ferrite::extension::types::{
  ApiError, CommandContext, DocumentInfo, Range as WitRange, TextChange,
};
use crate::config::{self, ConfigValue};
use crate::contributions::{LanguageContribution, LanguageServerContribution};
use crate::icons::IconTheme;
use crate::manifest::{self, MANIFEST_FILE, Manifest};
use crate::permissions::{Decision, Decisions, Permission};
use crate::preview::{PreviewContent, PreviewInfo};
use crate::runtime::{MemoryPool, Runtime};
use crate::usage::{ResourceUsage, Usage};
use crate::worker::{
  self, Budgets, Event, Failure, MAX_PENDING_CHANGE_BYTES, Queue, Setup, Worker,
};
use crate::{
  CommandInfo, ConfiguredSetting, Embedder, ExtensionConfiguration, ExtensionDecoration,
  ExtensionDiagnostic, ExtensionState, ExtensionStatus, HostOptions, Notification,
  PermissionStatus, Severity, StatusContent, StatusItem, ViewContent,
};

/// Synced versions kept per document, to interpret diagnostics published
/// for slightly older text.
const SYNC_HISTORY: usize = 8;
/// Added to a command's deadlines when waiting for its result, to cover
/// compiling the extension and events queued before it.
const COMMAND_SAFETY_MARGIN: Duration = Duration::from_secs(120);
/// How long shutdown waits for extensions to deactivate.
const SHUTDOWN_WAIT: Duration = Duration::from_millis(1500);

/// A diagnostic as byte ranges in the text at its set's version.
#[derive(Debug, Clone)]
pub struct StoredDiagnostic {
  pub range: Range<usize>,
  pub severity: Severity,
  pub message: String,
  pub code: Option<String>,
}

struct DiagnosticSet {
  version: Version,
  items: Vec<StoredDiagnostic>,
}

impl DiagnosticSet {
  fn catch_up(&mut self, buffer: &TextBuffer) {
    if !catch_up(
      &mut self.version,
      self.items.iter_mut().map(|item| &mut item.range),
      buffer,
    ) {
      self.items.clear();
    }
  }
}

/// Ranges decorated with one decoration type.
struct DecorationSet {
  version: Version,
  ranges: Vec<Range<usize>>,
}

impl DecorationSet {
  fn catch_up(&mut self, buffer: &TextBuffer) {
    if !catch_up(&mut self.version, self.ranges.iter_mut(), buffer) {
      self.ranges.clear();
    }
  }
}

/// Maps ranges stored at `version` through the edits since, and updates
/// `version`. Returns `false` if the edits are too old to map.
fn catch_up<'a>(
  version: &mut Version,
  ranges: impl Iterator<Item = &'a mut Range<usize>>,
  buffer: &TextBuffer,
) -> bool {
  if *version == buffer.version() {
    return true;
  }
  let mapped = match buffer.edits_since(*version) {
    Some(edits) => {
      for range in ranges {
        let start = map_offset_through(&edits, range.start, true);
        let end = map_offset_through(&edits, range.end, false).max(start);
        *range = start..end;
      }
      true
    }
    None => false,
  };
  *version = buffer.version();
  mapped
}

/// What the host knows about an open document.
struct DocumentRecord {
  info: DocumentInfo,
  /// The path `info` was built for.
  path: Option<PathBuf>,
  /// Recently synced texts, oldest first; the last is the current one.
  history: VecDeque<TextSnapshot>,
  /// Diagnostics by extension id.
  diagnostics: BTreeMap<String, DiagnosticSet>,
  /// Decorations by extension id and decoration type.
  decorations: BTreeMap<(String, String), DecorationSet>,
}

impl DocumentRecord {
  fn synced(&self) -> Option<&TextSnapshot> {
    self.history.back()
  }
}

enum Run {
  /// Not running: waiting for an activation event (or a permission
  /// decision, or enabling).
  Idle,
  Running {
    worker: Worker,
    active: bool,
  },
  Failed(String),
}

struct Entry {
  manifest: Arc<Manifest>,
  development: bool,
  config: Arc<RwLock<BTreeMap<String, ConfigValue>>>,
  config_errors: Vec<String>,
  run: Run,
  /// Kept across restarts and reloads of the extension.
  usage: Arc<Usage>,
}

/// A folder that could not be loaded as an extension.
struct Invalid {
  dir: PathBuf,
  development: bool,
  error: String,
}

/// An open preview.
struct PreviewRecord {
  /// The extension's id.
  extension: String,
  info: PreviewInfo,
  content: Arc<PreviewContent>,
  /// The picture files its content shows.
  images: BTreeSet<PathBuf>,
}

struct State {
  options: HostOptions,
  decisions: Decisions,
  extensions: Vec<Entry>,
  invalid: Vec<Invalid>,
  documents: HashMap<DocumentId, DocumentRecord>,
  /// Status bar content set by extensions, by extension and item id:
  /// `None` hides the item. Items not here show their manifest text.
  status: BTreeMap<(String, String), Option<StatusContent>>,
  /// View content set by extensions, by full view id.
  views: BTreeMap<String, ViewContent>,
  /// Open previews, by preview id.
  previews: BTreeMap<String, PreviewRecord>,
  next_generation: u64,
  shutting_down: bool,
}

pub struct Shared {
  me: Weak<Shared>,
  pub embedder: Arc<dyn Embedder>,
  runtime: OnceLock<Result<Arc<Runtime>, String>>,
  /// Linear memory of all running extensions.
  memory: Arc<MemoryPool>,
  state: Mutex<State>,
}

/// Notifications collected under the state lock, sent after it is released.
#[derive(Default)]
struct Outbox(Vec<Notification>);

impl Outbox {
  fn push(&mut self, notification: Notification) {
    self.0.push(notification);
  }

  fn send(self, embedder: &dyn Embedder) {
    for notification in self.0 {
      embedder.notify(notification);
    }
  }
}

impl Shared {
  fn lock(&self) -> MutexGuard<'_, State> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }

  pub fn memory_pool(&self) -> Arc<MemoryPool> {
    self.memory.clone()
  }

  /// The runtime, created on first use.
  pub fn runtime(&self) -> Result<Arc<Runtime>, String> {
    self
      .runtime
      .get_or_init(|| Runtime::new().map(Arc::new))
      .clone()
  }

  pub fn budgets(&self) -> Budgets {
    let limits = &self.lock().options.limits;
    Budgets {
      activation: limits.activation_time,
      command: limits.command_time,
      event: limits.event_time,
      memory: limits.memory_bytes,
    }
  }

  /// Starts the extension at `index` unless it is running, disabled,
  /// waiting for a permission decision, or the host is shutting down.
  fn start(&self, state: &mut State, index: usize, outbox: &mut Outbox) {
    let entry = &state.extensions[index];
    let decision = state.decisions.get(&entry.manifest.id);
    if state.shutting_down
      || entry.manifest.entrypoint.is_none()
      || decision.disabled
      || !decision.pending(&entry.manifest.permissions).is_empty()
      || matches!(entry.run, Run::Running { .. })
    {
      return;
    }
    let granted = decision.effective(&entry.manifest.permissions);
    let queue = Arc::new(Queue::new(
      state.options.limits.queue_capacity,
      entry.usage.clone(),
    ));
    if granted.contains(&Permission::DocumentsRead) {
      let mut infos: Vec<&DocumentInfo> = state
        .documents
        .values()
        .map(|record| &record.info)
        .collect();
      infos.sort_by_key(|info| info.id);
      for info in infos {
        queue.push_document(Event::Opened(info.clone()));
      }
    }
    state.next_generation += 1;
    let setup = Setup {
      manifest: entry.manifest.clone(),
      generation: state.next_generation,
      granted,
      config: entry.config.clone(),
      queue,
      cancel: Arc::new(AtomicBool::new(false)),
      usage: entry.usage.clone(),
    };
    tracing::info!(extension = %setup.manifest.id, "starting extension");
    let worker = worker::spawn(self.me.clone(), setup);
    state.extensions[index].run = Run::Running {
      worker,
      active: false,
    };
    outbox.push(Notification::ExtensionsChanged);
  }

  /// Stops the extension at `index` and forgets its diagnostics.
  fn stop(state: &mut State, index: usize, outbox: &mut Outbox) {
    let entry = &mut state.extensions[index];
    if let Run::Running { worker, .. } = std::mem::replace(&mut entry.run, Run::Idle) {
      worker.stop(true);
    }
    let id = entry.manifest.id.clone();
    Self::forget_output(state, &id, outbox);
    outbox.push(Notification::ExtensionsChanged);
  }

  /// Removes what a stopped extension showed: diagnostics, decorations,
  /// status bar content, view content, and previews.
  fn forget_output(state: &mut State, id: &str, outbox: &mut Outbox) {
    for (&document, record) in &mut state.documents {
      if record.diagnostics.remove(id).is_some() {
        outbox.push(Notification::DiagnosticsChanged(document));
      }
      let before = record.decorations.len();
      record
        .decorations
        .retain(|(extension, _), _| extension != id);
      if record.decorations.len() != before {
        outbox.push(Notification::DecorationsChanged(document));
      }
    }
    let before = state.status.len();
    state.status.retain(|(extension, _), _| extension != id);
    if state.status.len() != before {
      outbox.push(Notification::StatusItemsChanged);
    }
    let prefix = format!("{id}.");
    let views: Vec<String> = state
      .views
      .keys()
      .filter(|view| view.starts_with(&prefix))
      .cloned()
      .collect();
    for view in views {
      state.views.remove(&view);
      outbox.push(Notification::ViewChanged(view));
    }
    let before = state.previews.len();
    state.previews.retain(|_, preview| preview.extension != id);
    if state.previews.len() != before {
      outbox.push(Notification::PreviewsChanged);
    }
  }

  /// Sets (or hides, with `None`) one of an extension's status bar items.
  pub fn set_status_item(&self, extension: &str, item: &str, content: Option<StatusContent>) {
    let changed = {
      let mut state = self.lock();
      let key = (extension.to_owned(), item.to_owned());
      if state.status.get(&key) == Some(&content) {
        false
      } else {
        state.status.insert(key, content);
        true
      }
    };
    if changed {
      self.embedder.notify(Notification::StatusItemsChanged);
    }
  }

  /// Changes the content of a view.
  pub fn update_view(&self, full_id: &str, change: impl FnOnce(&mut ViewContent)) {
    let changed = {
      let mut state = self.lock();
      let content = state.views.entry(full_id.to_owned()).or_default();
      let before = content.clone();
      change(content);
      *content != before
    };
    if changed {
      self
        .embedder
        .notify(Notification::ViewChanged(full_id.to_owned()));
    }
  }

  /// Opens a preview, or updates its title, and asks to show it.
  pub fn show_preview(&self, extension: &str, info: PreviewInfo, beside: bool) {
    let id = info.id.clone();
    let changed = {
      let mut state = self.lock();
      match state.previews.get_mut(&id) {
        Some(record) if record.info == info => false,
        Some(record) => {
          record.info = info;
          true
        }
        None => {
          state.previews.insert(
            id.clone(),
            PreviewRecord {
              extension: extension.to_owned(),
              info,
              content: Arc::default(),
              images: BTreeSet::new(),
            },
          );
          true
        }
      }
    };
    if changed {
      self.embedder.notify(Notification::PreviewsChanged);
    }
    self
      .embedder
      .notify(Notification::ShowPreview { id, beside });
  }

  /// The document an open preview shows: `None` if it isn't open.
  pub fn preview_document(&self, id: &str) -> Option<Option<DocumentId>> {
    self
      .lock()
      .previews
      .get(id)
      .map(|record| record.info.document.map(DocumentId::from_raw))
  }

  /// Replaces an open preview's content; false if it isn't open.
  pub fn set_preview_content(
    &self,
    id: &str,
    content: PreviewContent,
    images: BTreeSet<PathBuf>,
  ) -> bool {
    let changed = {
      let mut state = self.lock();
      let Some(record) = state.previews.get_mut(id) else {
        return false;
      };
      if *record.content == content {
        false
      } else {
        record.content = Arc::new(content);
        record.images = images;
        true
      }
    };
    if changed {
      self
        .embedder
        .notify(Notification::PreviewChanged(id.to_owned()));
    }
    true
  }

  /// Closes a preview, if it's open.
  pub fn close_preview(&self, id: &str) {
    let removed = self.lock().previews.remove(id).is_some();
    if removed {
      self.embedder.notify(Notification::PreviewsChanged);
    }
  }

  pub fn worker_active(&self, id: &str, generation: u64) {
    let mut state = self.lock();
    let Some(entry) = state
      .extensions
      .iter_mut()
      .find(|entry| entry.manifest.id == id)
    else {
      return;
    };
    if let Run::Running { worker, active } = &mut entry.run
      && worker.generation == generation
    {
      *active = true;
      tracing::info!(extension = %id, "extension active");
      drop(state);
      self.embedder.notify(Notification::ExtensionsChanged);
    }
  }

  pub fn worker_finished(&self, id: &str, generation: u64, outcome: Result<(), Failure>) {
    let mut outbox = Outbox::default();
    let mut reply = None;
    {
      let mut state = self.lock();
      let index = state.extensions.iter().position(|entry| {
        entry.manifest.id == id
          && matches!(&entry.run, Run::Running { worker, .. } if worker.generation == generation)
      });
      if let Some(index) = index {
        let entry = &mut state.extensions[index];
        let name = entry.manifest.name.clone();
        let (run, stopped) = match outcome {
          Ok(()) => (Run::Idle, "The extension stopped.".to_owned()),
          Err(failure) => {
            tracing::warn!(extension = %id, reason = %failure.reason, "extension failed");
            reply = failure.reply.map(|reply| (reply, failure.reason.clone()));
            outbox.push(Notification::ExtensionFailed {
              id: id.to_owned(),
              name,
              reason: failure.reason.clone(),
            });
            let stopped = format!("The extension stopped: {}", failure.reason);
            (Run::Failed(failure.reason), stopped)
          }
        };
        if let Run::Running { worker, .. } = std::mem::replace(&mut entry.run, run) {
          worker.queue.stop(&stopped);
        }
        Self::forget_output(&mut state, id, &mut outbox);
        outbox.push(Notification::ExtensionsChanged);
      } else if let Err(failure) = outcome {
        // Stopped or restarted meanwhile.
        reply = failure.reply.map(|reply| (reply, failure.reason));
      }
    }
    if let Some((reply, reason)) = reply {
      let _ = reply.send(Err(format!("The extension stopped: {reason}")));
    }
    outbox.send(self.embedder.as_ref());
  }

  /// Brings the host's view of `document` up to date and notifies
  /// extensions that read documents. Called with the editor locked.
  pub fn sync(&self, document: &Document) {
    let mut outbox = Outbox::default();
    {
      let mut guard = self.lock();
      let state = &mut *guard;
      if state.shutting_down {
        return;
      }
      let id = document.id();
      let buffer = document.buffer();
      // Most syncs (every view) find nothing new.
      if let Some(record) = state.documents.get(&id)
        && record.info.version == document.version().get()
        && record.info.dirty == document.is_dirty()
        && record.path.as_deref() == document.path()
      {
        return;
      }
      let info = document_info(self, document);
      match state.documents.get_mut(&id) {
        None => {
          let language = info.language.clone();
          state.documents.insert(
            id,
            DocumentRecord {
              info: info.clone(),
              path: document.path().map(Path::to_path_buf),
              history: VecDeque::from([buffer.snapshot()]),
              diagnostics: BTreeMap::new(),
              decorations: BTreeMap::new(),
            },
          );
          for entry in &state.extensions {
            if let Run::Running { worker, .. } = &entry.run
              && worker.reads_documents()
            {
              worker.queue.push_document(Event::Opened(info.clone()));
            }
          }
          if let Some(language) = language {
            let wanting: Vec<usize> = (0..state.extensions.len())
              .filter(|&index| {
                state.extensions[index]
                  .manifest
                  .activates_for_language(&language)
              })
              .collect();
            for index in wanting {
              self.start(state, index, &mut outbox);
            }
          }
        }
        Some(record) => {
          let synced_version = record.synced().map(TextSnapshot::version);
          record.info = info.clone();
          record.path = document.path().map(Path::to_path_buf);
          if synced_version != Some(buffer.version()) {
            let readers: Vec<&Worker> = state
              .extensions
              .iter()
              .filter_map(|entry| match &entry.run {
                Run::Running { worker, .. } if worker.reads_documents() => Some(worker),
                _ => None,
              })
              .collect();
            let changes = synced_version
              .filter(|_| !readers.is_empty())
              .and_then(|version| buffer.edits_since(version))
              .map(|edits| {
                edits
                  .into_iter()
                  .map(|edit| TextChange {
                    range: WitRange {
                      start: point(edit.start_point),
                      end: point(edit.old_end_point),
                    },
                    text: edit.text,
                  })
                  .collect::<Vec<_>>()
              })
              .filter(|changes| {
                changes
                  .iter()
                  .map(|change| change.text.len())
                  .sum::<usize>()
                  <= MAX_PENDING_CHANGE_BYTES
              });
            if record.history.len() == SYNC_HISTORY {
              record.history.pop_front();
            }
            record.history.push_back(buffer.snapshot());
            for worker in readers {
              worker.queue.push_document(Event::Changed {
                document: id.get(),
                version: info.version,
                changes: changes.clone(),
              });
            }
          }
        }
      }
    }
    outbox.send(self.embedder.as_ref());
  }

  /// Stores an extension's diagnostics, converting their ranges in the
  /// text at `version`: the current text, or a recently synced one.
  /// Called with the editor locked.
  pub fn store_diagnostics(
    &self,
    document: &Document,
    extension: &str,
    version: u64,
    convert: impl FnOnce(&TextBuffer) -> Result<Vec<StoredDiagnostic>, ApiError>,
  ) -> Result<(), ApiError> {
    let mut state = self.lock();
    let (items, at, record) = Self::convert_at(&mut state, document, extension, version, convert)?;
    record
      .diagnostics
      .insert(extension.to_owned(), DiagnosticSet { version: at, items });
    Ok(())
  }

  /// Stores an extension's ranges for one decoration type, like
  /// [`Self::store_diagnostics`].
  pub fn store_decorations(
    &self,
    document: &Document,
    extension: &str,
    decoration: &str,
    version: u64,
    convert: impl FnOnce(&TextBuffer) -> Result<Vec<Range<usize>>, ApiError>,
  ) -> Result<(), ApiError> {
    let mut state = self.lock();
    let (ranges, at, record) = Self::convert_at(&mut state, document, extension, version, convert)?;
    let key = (extension.to_owned(), decoration.to_owned());
    if ranges.is_empty() {
      record.decorations.remove(&key);
    } else {
      record.decorations.insert(
        key,
        DecorationSet {
          version: at,
          ranges,
        },
      );
    }
    Ok(())
  }

  /// Converts with the text at `version` (current or recently synced) for
  /// a running extension.
  fn convert_at<'s, T>(
    state: &'s mut State,
    document: &Document,
    extension: &str,
    version: u64,
    convert: impl FnOnce(&TextBuffer) -> Result<T, ApiError>,
  ) -> Result<(T, Version, &'s mut DocumentRecord), ApiError> {
    let current = document.version().get();
    let running = state
      .extensions
      .iter()
      .any(|entry| entry.manifest.id == extension && matches!(entry.run, Run::Running { .. }));
    if !running {
      return Err(ApiError::InvalidArgument(
        "the extension is not running".into(),
      ));
    }
    let record = state
      .documents
      .get_mut(&document.id())
      .ok_or(ApiError::UnknownDocument)?;
    if version == current {
      let converted = convert(document.buffer())?;
      return Ok((converted, document.version(), record));
    }
    let snapshot = record
      .history
      .iter()
      .find(|snapshot| snapshot.version().get() == version)
      .ok_or(ApiError::StaleVersion(current))?;
    let at = snapshot.version();
    let converted = convert(&TextBuffer::from_snapshot(snapshot))?;
    Ok((converted, at, record))
  }
}

fn point(point: ferrite_text::TextPoint) -> crate::bindings::ferrite::extension::types::Position {
  crate::bindings::ferrite::extension::types::Position {
    line: u32::try_from(point.row).unwrap_or(u32::MAX),
    column: u32::try_from(point.column).unwrap_or(u32::MAX),
  }
}

/// A command run waiting for its result.
pub struct CommandTicket {
  receiver: Receiver<Result<(), String>>,
  timeout: Duration,
}

impl CommandTicket {
  /// Waits for the command to finish. Fails with the extension's error, or
  /// if the extension stops.
  ///
  /// Every call into the extension has its own deadline, and the worker
  /// always replies, so this normally returns within those deadlines plus
  /// the time to compile the extension (which can't be interrupted). The
  /// timeout is only a safety net against a host bug.
  pub fn wait(self) -> Result<(), String> {
    match self.receiver.recv_timeout(self.timeout) {
      Ok(result) => result,
      Err(mpsc::RecvTimeoutError::Timeout) => Err("The extension did not respond.".into()),
      Err(mpsc::RecvTimeoutError::Disconnected) => Err("The extension stopped.".into()),
    }
  }
}

/// What a permission allows, for asking the user: for language servers,
/// the commands that would run.
fn describe(permission: Permission, manifest: &Manifest) -> String {
  let description = permission.description();
  if permission != Permission::LanguageServers {
    return description.to_owned();
  }
  let commands: Vec<String> = manifest
    .contributions
    .language_servers
    .iter()
    .map(|server| server.command_line())
    .collect();
  format!("{description}: {}", commands.join("; "))
}

/// Loads and runs extensions for an editor. See the crate documentation.
pub struct ExtensionHost {
  shared: Arc<Shared>,
}

impl ExtensionHost {
  /// Creates a host. Nothing is loaded until [`Self::load`].
  pub fn new(embedder: Arc<dyn Embedder>, options: HostOptions) -> Self {
    let decisions = Decisions::load(options.permissions_file.clone());
    let memory = Arc::new(MemoryPool::new(options.limits.total_memory_bytes));
    let shared = Arc::new_cyclic(|me| Shared {
      me: me.clone(),
      embedder,
      runtime: OnceLock::new(),
      memory,
      state: Mutex::new(State {
        options,
        decisions,
        extensions: Vec::new(),
        invalid: Vec::new(),
        documents: HashMap::new(),
        status: BTreeMap::new(),
        views: BTreeMap::new(),
        previews: BTreeMap::new(),
        next_generation: 0,
        shutting_down: false,
      }),
    });
    Self { shared }
  }

  /// (Re)discovers extensions and starts those that activate on startup or
  /// for an open document's language. Running extensions are restarted.
  /// Reads the disk; call without holding the editor lock.
  pub fn load(&self) {
    let (dirs, editor_version) = {
      let state = self.shared.lock();
      let mut dirs: Vec<(PathBuf, bool)> = state
        .options
        .development
        .iter()
        .map(|dir| (dir.clone(), true))
        .collect();
      if let Some(root) = &state.options.extensions_dir {
        dirs.extend(installed_dirs(root).into_iter().map(|dir| (dir, false)));
      }
      (dirs, state.options.editor_version.clone())
    };
    let mut loaded: Vec<(Manifest, bool)> = Vec::new();
    let mut invalid = Vec::new();
    for (dir, development) in dirs {
      match manifest::load(&dir, &editor_version, development) {
        Ok(manifest) => {
          if loaded.iter().any(|(other, _)| other.id == manifest.id) {
            invalid.push(Invalid {
              dir,
              development,
              error: format!(
                "another extension with the id {:?} is already loaded",
                manifest.id
              ),
            });
          } else {
            tracing::info!(extension = %manifest.id, version = %manifest.version, development, "found extension");
            loaded.push((manifest, development));
          }
        }
        Err(error) => {
          tracing::warn!(dir = %dir.display(), %error, "invalid extension");
          invalid.push(Invalid {
            dir,
            development,
            error: error.to_string(),
          });
        }
      }
    }
    loaded.sort_by_key(|(manifest, _)| manifest.name.to_lowercase());

    let mut outbox = Outbox::default();
    {
      let mut state = self.shared.lock();
      if state.shutting_down {
        return;
      }
      for index in 0..state.extensions.len() {
        Shared::stop(&mut state, index, &mut outbox);
      }
      let mut usages: HashMap<String, Arc<Usage>> = state
        .extensions
        .drain(..)
        .map(|entry| (entry.manifest.id.clone(), entry.usage))
        .collect();
      state.extensions = loaded
        .into_iter()
        .map(|(manifest, development)| {
          let (values, config_errors) = config::resolve(
            &manifest.configuration,
            state.options.settings.get(&manifest.id),
          );
          let usage = usages.remove(&manifest.id).unwrap_or_default();
          Entry {
            manifest: Arc::new(manifest),
            development,
            config: Arc::new(RwLock::new(values)),
            config_errors,
            run: Run::Idle,
            usage,
          }
        })
        .collect();
      state.invalid = invalid;
      self.start_wanted(&mut state, &mut outbox);
      outbox.push(Notification::ExtensionsChanged);
    }
    outbox.send(self.shared.embedder.as_ref());
  }

  /// Starts idle extensions that activate on startup or for the language
  /// of an open document.
  fn start_wanted(&self, state: &mut State, outbox: &mut Outbox) {
    let languages: Vec<String> = state
      .documents
      .values()
      .filter_map(|record| record.info.language.clone())
      .collect();
    for index in 0..state.extensions.len() {
      let manifest = &state.extensions[index].manifest;
      let wanted = manifest.activates_on_startup()
        || languages
          .iter()
          .any(|language| manifest.activates_for_language(language));
      if wanted && matches!(state.extensions[index].run, Run::Idle) {
        self.shared.start(state, index, outbox);
      }
    }
  }

  /// Applies new options: rediscovers extensions if the folders changed,
  /// and updates configuration values in place otherwise.
  pub fn configure(&self, options: HostOptions) {
    let reload = {
      let mut guard = self.shared.lock();
      let state = &mut *guard;
      let reload = state.options.extensions_dir != options.extensions_dir
        || state.options.development != options.development;
      state.options = options;
      for entry in &mut state.extensions {
        let (values, errors) = config::resolve(
          &entry.manifest.configuration,
          state.options.settings.get(&entry.manifest.id),
        );
        *entry.config.write().unwrap_or_else(PoisonError::into_inner) = values;
        entry.config_errors = errors;
      }
      reload
    };
    if reload {
      self.load();
    } else {
      self.shared.embedder.notify(Notification::ExtensionsChanged);
    }
  }

  pub fn statuses(&self) -> Vec<ExtensionStatus> {
    let state = self.shared.lock();
    let mut statuses: Vec<ExtensionStatus> = state
      .extensions
      .iter()
      .map(|entry| {
        let manifest = &entry.manifest;
        let decision = state.decisions.get(&manifest.id);
        let (status, error) = match &entry.run {
          _ if decision.disabled => (ExtensionState::Disabled, None),
          Run::Idle if !decision.pending(&manifest.permissions).is_empty() => {
            (ExtensionState::AwaitingPermission, None)
          }
          _ if manifest.entrypoint.is_none() => (ExtensionState::Ready, None),
          Run::Idle => (ExtensionState::Inactive, None),
          Run::Running { active: false, .. } => (ExtensionState::Starting, None),
          Run::Running { active: true, .. } => (ExtensionState::Active, None),
          Run::Failed(reason) => (ExtensionState::Failed, Some(reason.clone())),
        };
        ExtensionStatus {
          id: manifest.id.clone(),
          name: manifest.name.clone(),
          version: manifest.version.to_string(),
          description: manifest.description.clone(),
          path: manifest.dir.to_string_lossy().into_owned(),
          development: entry.development,
          state: status,
          error,
          permissions: manifest
            .permissions
            .iter()
            .map(|&permission| PermissionStatus {
              permission,
              description: describe(permission, manifest),
              granted: decision.granted.contains(&permission),
              decided: decision.granted.contains(&permission)
                || decision.denied.contains(&permission),
            })
            .collect(),
          commands: manifest
            .commands
            .iter()
            .map(|command| CommandInfo {
              id: command.id.clone(),
              title: command.title.clone(),
              category: command.category.clone(),
              hidden: command.hidden,
            })
            .collect(),
          config_errors: entry.config_errors.clone(),
          has_settings: !manifest.configuration.is_empty(),
          contributions: manifest.contributions.clone(),
          usage: {
            let mut usage = entry.usage.snapshot();
            if !matches!(entry.run, Run::Running { .. }) {
              usage.memory_bytes = 0;
            }
            usage
          },
        }
      })
      .collect();
    statuses.extend(state.invalid.iter().map(|invalid| {
      let name = invalid.dir.file_name().map_or_else(
        || invalid.dir.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
      );
      ExtensionStatus {
        id: invalid.dir.to_string_lossy().into_owned(),
        name,
        version: String::new(),
        description: String::new(),
        path: invalid.dir.to_string_lossy().into_owned(),
        development: invalid.development,
        state: ExtensionState::Invalid,
        error: Some(invalid.error.clone()),
        permissions: Vec::new(),
        commands: Vec::new(),
        config_errors: Vec::new(),
        has_settings: false,
        contributions: crate::Contributions::default(),
        usage: ResourceUsage::default(),
      }
    }));
    statuses
  }

  /// The settings each extension declares, with the values in effect, for
  /// a settings editor. Extensions that declare none are left out.
  pub fn configurations(&self) -> Vec<ExtensionConfiguration> {
    let state = self.shared.lock();
    state
      .extensions
      .iter()
      .filter(|entry| !entry.manifest.configuration.is_empty())
      .map(|entry| {
        let manifest = &entry.manifest;
        let values = entry.config.read().unwrap_or_else(PoisonError::into_inner);
        let user = state.options.settings.get(&manifest.id);
        ExtensionConfiguration {
          id: manifest.id.clone(),
          name: manifest.name.clone(),
          settings: manifest
            .configuration
            .iter()
            .map(|(key, property)| ConfiguredSetting {
              key: key.clone(),
              property: property.clone(),
              value: values
                .get(key)
                .cloned()
                .unwrap_or_else(|| property.default.clone()),
              problem: user
                .and_then(|user| user.get(key))
                .and_then(|value| property.check(value).err()),
            })
            .collect(),
        }
      })
      .collect()
  }

  /// The languages contributed by extensions that aren't turned off, with
  /// their files' paths made absolute.
  pub fn languages(&self) -> Vec<LanguageContribution> {
    let state = self.shared.lock();
    state
      .extensions
      .iter()
      .filter(|entry| !state.decisions.get(&entry.manifest.id).disabled)
      .flat_map(|entry| {
        let dir = &entry.manifest.dir;
        entry
          .manifest
          .contributions
          .languages
          .iter()
          .map(move |language| LanguageContribution {
            grammar: dir.join(&language.grammar),
            highlights: dir.join(&language.highlights),
            injections: language.injections.as_ref().map(|path| dir.join(path)),
            folds: language.folds.as_ref().map(|path| dir.join(path)),
            ..language.clone()
          })
      })
      .collect()
  }

  /// The language servers of extensions that aren't turned off and have
  /// been granted `language-servers`. Commands inside an extension's folder
  /// are made absolute.
  pub fn language_servers(&self) -> Vec<LanguageServerContribution> {
    let state = self.shared.lock();
    state
      .extensions
      .iter()
      .filter(|entry| {
        let decision = state.decisions.get(&entry.manifest.id);
        !decision.disabled && decision.granted.contains(&Permission::LanguageServers)
      })
      .flat_map(|entry| {
        let dir = &entry.manifest.dir;
        entry
          .manifest
          .contributions
          .language_servers
          .iter()
          .map(move |server| LanguageServerContribution {
            command: if server.command.contains(['/', '\\']) {
              dir.join(&server.command).to_string_lossy().into_owned()
            } else {
              server.command.clone()
            },
            ..server.clone()
          })
      })
      .collect()
  }

  /// An icon theme (`<extension id>.<icon theme id>`) from an extension that
  /// isn't turned off.
  pub fn icon_theme(&self, id: &str) -> Option<Arc<IconTheme>> {
    let state = self.shared.lock();
    state
      .extensions
      .iter()
      .filter(|entry| !state.decisions.get(&entry.manifest.id).disabled)
      .flat_map(|entry| &entry.manifest.contributions.icon_themes)
      .find(|theme| theme.id == id)
      .map(|theme| theme.theme.clone())
  }

  /// Looks up the languages of open documents again, after the embedder's
  /// languages changed (an extension contributed one), and starts
  /// extensions that activate for them.
  pub fn languages_changed(&self) {
    let mut outbox = Outbox::default();
    {
      let mut guard = self.shared.lock();
      let state = &mut *guard;
      if state.shutting_down {
        return;
      }
      for record in state.documents.values_mut() {
        record.info.language = record
          .path
          .as_deref()
          .and_then(|path| self.shared.embedder.language_of(path));
      }
      self.start_wanted(state, &mut outbox);
    }
    outbox.send(self.shared.embedder.as_ref());
  }

  /// Runs an extension command, starting the extension if needed.
  /// `document` is the active document, whose selections are passed along.
  /// Call without holding the editor lock.
  pub fn run_command(
    &self,
    command: &str,
    document: Option<DocumentId>,
    argument: Option<String>,
  ) -> Result<CommandTicket, String> {
    let selections = document
      .and_then(|id| {
        api::with_editor(&self.shared, |editor| {
          editor.document(id).ok().map(|document| {
            let buffer = document.buffer();
            let selections = document.selections();
            let primary = selections.primary();
            std::iter::once(primary)
              .chain(selections.iter().copied().filter(|s| *s != primary))
              .map(|selection| {
                let range = selection.range();
                WitRange {
                  start: to_position(buffer, range.start),
                  end: to_position(buffer, range.end),
                }
              })
              .collect::<Vec<_>>()
          })
        })
        .flatten()
      })
      .unwrap_or_default();
    let mut outbox = Outbox::default();
    let result = {
      let mut state = self.shared.lock();
      let index = state
        .extensions
        .iter()
        .position(|entry| entry.manifest.declares_command(command))
        .ok_or_else(|| format!("No extension provides the command {command}."))?;
      let entry = &state.extensions[index];
      let name = entry.manifest.name.clone();
      let decision = state.decisions.get(&entry.manifest.id);
      if decision.disabled {
        return Err(format!("{name} is disabled."));
      }
      if !decision.pending(&entry.manifest.permissions).is_empty() {
        return Err(format!(
          "{name} is waiting for you to allow or deny its permissions."
        ));
      }
      if let Run::Failed(reason) = &entry.run {
        return Err(format!(
          "{name} has stopped ({reason}). Restart it to try again."
        ));
      }
      self.shared.start(&mut state, index, &mut outbox);
      let command_time = state.options.limits.command_time;
      match &state.extensions[index].run {
        Run::Running { worker, .. } => {
          let (reply, receiver) = mpsc::channel();
          let context = CommandContext {
            document: document.map(DocumentId::get),
            selections,
          };
          worker
            .queue
            .push_command(Event::Command {
              command: command.to_owned(),
              context,
              argument,
              reply,
            })
            .map(|()| CommandTicket {
              receiver,
              timeout: command_time + state.options.limits.activation_time + COMMAND_SAFETY_MARGIN,
            })
        }
        _ => Err(format!("{name} could not be started.")),
      }
    };
    outbox.send(self.shared.embedder.as_ref());
    result
  }

  /// Records the user's permission decision for an extension: `granted`
  /// are allowed, its other requested permissions denied. A running
  /// extension is restarted with the new permissions.
  pub fn set_permissions(&self, id: &str, granted: &[Permission]) {
    self.update_decision(id, |decision, requested| {
      decision.granted = requested
        .iter()
        .copied()
        .filter(|p| granted.contains(p))
        .collect();
      decision.denied = requested
        .iter()
        .copied()
        .filter(|p| !granted.contains(p))
        .collect();
    });
  }

  pub fn set_enabled(&self, id: &str, enabled: bool) {
    self.update_decision(id, |decision, _| decision.disabled = !enabled);
  }

  fn update_decision(&self, id: &str, change: impl FnOnce(&mut Decision, &[Permission])) {
    let mut outbox = Outbox::default();
    {
      let mut state = self.shared.lock();
      let Some(index) = state
        .extensions
        .iter()
        .position(|entry| entry.manifest.id == id)
      else {
        return;
      };
      let mut decision = state.decisions.get(id);
      let before = decision.clone();
      change(&mut decision, &state.extensions[index].manifest.permissions);
      if decision == before {
        return;
      }
      state.decisions.set(id, decision);
      Shared::stop(&mut state, index, &mut outbox);
      self.start_wanted(&mut state, &mut outbox);
    }
    outbox.send(self.shared.embedder.as_ref());
  }

  /// The status bar items to show, from enabled extensions that haven't
  /// failed.
  pub fn status_items(&self) -> Vec<StatusItem> {
    let state = self.shared.lock();
    let mut items = Vec::new();
    for entry in &state.extensions {
      let manifest = &entry.manifest;
      if state.decisions.get(&manifest.id).disabled || matches!(entry.run, Run::Failed(_)) {
        continue;
      }
      for declared in &manifest.contributions.status_items {
        let content = match state
          .status
          .get(&(manifest.id.clone(), declared.id.clone()))
        {
          Some(content) => content.clone(),
          None => declared.text.clone().map(|text| StatusContent {
            text,
            tooltip: declared.tooltip.clone(),
            command: declared.command.clone(),
          }),
        };
        if let Some(content) = content {
          items.push(StatusItem {
            id: format!("{}.{}", manifest.id, declared.id),
            extension: manifest.name.clone(),
            alignment: declared.alignment,
            priority: declared.priority,
            content,
          });
        }
      }
    }
    items
  }

  /// The content of a view (by full id), as last set by its extension.
  pub fn view_content(&self, full_id: &str) -> ViewContent {
    self
      .shared
      .lock()
      .views
      .get(full_id)
      .cloned()
      .unwrap_or_default()
  }

  /// The open previews, in the order of their ids.
  pub fn previews(&self) -> Vec<PreviewInfo> {
    self
      .shared
      .lock()
      .previews
      .values()
      .map(|record| record.info.clone())
      .collect()
  }

  /// A preview's content, as last set by its extension; `None` if it isn't
  /// open.
  pub fn preview_content(&self, id: &str) -> Option<Arc<PreviewContent>> {
    self
      .shared
      .lock()
      .previews
      .get(id)
      .map(|record| record.content.clone())
  }

  /// The user closed a preview's tab.
  pub fn preview_closed(&self, id: &str) {
    self.shared.close_preview(id);
  }

  /// Whether a preview's content shows the picture file `path`, which the
  /// workbench may then load for it.
  pub fn preview_shows_image(&self, id: &str, path: &Path) -> bool {
    self
      .shared
      .lock()
      .previews
      .get(id)
      .is_some_and(|record| record.images.contains(path))
  }

  /// A picture a preview shows, as a `data:` URL. Only the files its
  /// content shows can be read. Reads the disk.
  pub fn preview_image(&self, id: &str, path: &Path) -> Result<String, String> {
    if !self.preview_shows_image(id, path) {
      return Err("the preview doesn't show this picture".into());
    }
    crate::preview::load_image(path)
  }

  /// Called when a view is shown: starts its extension if it activates for
  /// the view.
  pub fn view_opened(&self, full_id: &str) {
    let mut outbox = Outbox::default();
    {
      let mut state = self.shared.lock();
      let index = state.extensions.iter().position(|entry| {
        entry
          .manifest
          .contributions
          .views
          .iter()
          .any(|view| view.full_id == full_id && entry.manifest.activates_for_view(&view.id))
      });
      if let Some(index) = index
        && matches!(state.extensions[index].run, Run::Idle)
      {
        self.shared.start(&mut state, index, &mut outbox);
      }
    }
    outbox.send(self.shared.embedder.as_ref());
  }

  /// Extension decorations overlapping `bytes`, mapped to the current text.
  /// Call with the editor locked.
  pub fn decorations(&self, document: &Document, bytes: Range<usize>) -> Vec<ExtensionDecoration> {
    let mut state = self.shared.lock();
    let Some(record) = state.documents.get_mut(&document.id()) else {
      return Vec::new();
    };
    let mut found = Vec::new();
    for ((extension, name), set) in &mut record.decorations {
      set.catch_up(document.buffer());
      let key = format!("{extension}/{name}");
      found.extend(
        set
          .ranges
          .iter()
          .filter(|range| range.start <= bytes.end && range.end >= bytes.start)
          .map(|range| ExtensionDecoration {
            range: range.clone(),
            key: key.clone(),
          }),
      );
    }
    found
  }

  /// Restarts an extension (e.g. after it failed, or was rebuilt).
  pub fn restart(&self, id: &str) {
    let mut outbox = Outbox::default();
    {
      let mut state = self.shared.lock();
      let Some(index) = state
        .extensions
        .iter()
        .position(|entry| entry.manifest.id == id)
      else {
        return;
      };
      Shared::stop(&mut state, index, &mut outbox);
      self.shared.start(&mut state, index, &mut outbox);
    }
    outbox.send(self.shared.embedder.as_ref());
  }

  /// Brings the host's view of `document` up to date: announces new
  /// documents and sends changes to extensions. Call with the editor
  /// locked, whenever a document may have changed (e.g. on every view).
  pub fn sync(&self, document: &Document) {
    self.shared.sync(document);
  }

  /// Forgets a closed document, closes its previews, and tells extensions.
  pub fn closed(&self, id: DocumentId) {
    let previews_closed = {
      let mut state = self.shared.lock();
      let before = state.previews.len();
      state
        .previews
        .retain(|_, preview| preview.info.document != Some(id.get()));
      let previews_closed = state.previews.len() != before;
      if state.documents.remove(&id).is_some() {
        for entry in &state.extensions {
          if let Run::Running { worker, .. } = &entry.run
            && worker.reads_documents()
          {
            worker.queue.push_document(Event::Closed(id.get()));
          }
        }
      }
      previews_closed
    };
    if previews_closed {
      self.shared.embedder.notify(Notification::PreviewsChanged);
    }
  }

  /// Extension diagnostics overlapping `bytes`, mapped to the current text.
  /// Call with the editor locked.
  pub fn diagnostics(&self, document: &Document, bytes: Range<usize>) -> Vec<ExtensionDiagnostic> {
    self.collect(document, |range| {
      range.start <= bytes.end && range.end >= bytes.start
    })
  }

  /// Extension diagnostics covering `offset`.
  pub fn diagnostics_at(&self, document: &Document, offset: usize) -> Vec<ExtensionDiagnostic> {
    self.collect(document, |range| {
      range.start <= offset && offset <= range.end
    })
  }

  fn collect(
    &self,
    document: &Document,
    wanted: impl Fn(&Range<usize>) -> bool,
  ) -> Vec<ExtensionDiagnostic> {
    let mut state = self.shared.lock();
    let names: HashMap<String, String> = state
      .extensions
      .iter()
      .map(|entry| (entry.manifest.id.clone(), entry.manifest.name.clone()))
      .collect();
    let Some(record) = state.documents.get_mut(&document.id()) else {
      return Vec::new();
    };
    let mut found = Vec::new();
    for (id, set) in &mut record.diagnostics {
      set.catch_up(document.buffer());
      let source = names.get(id).cloned().unwrap_or_else(|| id.clone());
      found.extend(
        set
          .items
          .iter()
          .filter(|item| wanted(&item.range))
          .map(|item| ExtensionDiagnostic {
            range: item.range.clone(),
            severity: item.severity,
            message: item.message.clone(),
            code: item.code.clone(),
            source: source.clone(),
          }),
      );
    }
    found
  }

  /// Stops every extension, giving each a moment to deactivate.
  pub fn shutdown(&self) {
    let workers: Vec<Worker> = {
      let mut state = self.shared.lock();
      state.shutting_down = true;
      state
        .extensions
        .iter_mut()
        .filter_map(|entry| match std::mem::replace(&mut entry.run, Run::Idle) {
          Run::Running { worker, .. } => Some(worker),
          other => {
            entry.run = other;
            None
          }
        })
        .collect()
    };
    for worker in &workers {
      worker.stop(false);
    }
    let deadline = Instant::now() + SHUTDOWN_WAIT;
    for mut worker in workers {
      let Some(thread) = worker.thread.take() else {
        continue;
      };
      while !thread.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
      }
      if thread.is_finished() {
        let _ = thread.join();
      } else {
        worker.stop(true);
        tracing::warn!("an extension did not stop in time");
      }
    }
  }
}

/// Subfolders of `root` that contain a manifest, sorted by name.
fn installed_dirs(root: &Path) -> Vec<PathBuf> {
  let Ok(entries) = std::fs::read_dir(root) else {
    return Vec::new();
  };
  let mut dirs: Vec<PathBuf> = entries
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.join(MANIFEST_FILE).is_file())
    .collect();
  dirs.sort();
  dirs
}
