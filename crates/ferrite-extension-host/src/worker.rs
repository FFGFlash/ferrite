//! Each running extension has a worker thread that owns its WebAssembly
//! instance and processes its event queue one event at a time.
//!
//! The editor never calls into an extension directly: it pushes events
//! (document changes, commands) onto the extension's queue and moves on.
//! The queue is bounded, and consecutive changes to a document are merged
//! while they wait, so a slow extension sees fewer, larger changes rather
//! than an ever-growing backlog. Every call runs under a deadline; a trap,
//! an exceeded limit, or a missed deadline ends the instance, and the host
//! marks the extension failed.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use wasmtime::Store;

use crate::api::ApiContext;
use crate::bindings::Extension;
use crate::bindings::ferrite::extension::types::{
  Activation, CommandContext, DocumentChange, DocumentInfo, Permission as WitPermission, TextChange,
};
use crate::host::Shared;
use crate::manifest::Manifest;
use crate::permissions::Permission;
use crate::runtime::{Call, Interrupt, Limiter, Runtime, StoreState};

/// Changed text kept in one pending notification; beyond this the extension
/// is told to re-read the document instead.
pub const MAX_PENDING_CHANGE_BYTES: usize = 1024 * 1024;
/// Error messages from extensions are cut to this length.
const MAX_ERROR_CHARS: usize = 1_000;

pub type Reply = Sender<Result<(), String>>;

/// Why a worker stopped running its extension.
pub struct Failure {
  pub reason: String,
  /// The command that caused it, answered once the host has recorded the
  /// failure.
  pub reply: Option<Reply>,
}

impl From<String> for Failure {
  fn from(reason: String) -> Self {
    Self {
      reason,
      reply: None,
    }
  }
}

impl From<&str> for Failure {
  fn from(reason: &str) -> Self {
    reason.to_owned().into()
  }
}

pub enum Event {
  Opened(DocumentInfo),
  Changed {
    document: u64,
    version: u64,
    changes: Option<Vec<TextChange>>,
  },
  Closed(u64),
  Command {
    command: String,
    context: CommandContext,
    reply: Reply,
  },
}

impl Event {
  fn document(&self) -> Option<u64> {
    match self {
      Self::Opened(info) => Some(info.id),
      Self::Changed { document, .. } | Self::Closed(document) => Some(*document),
      Self::Command { .. } => None,
    }
  }
}

fn change_bytes(changes: &[TextChange]) -> usize {
  changes.iter().map(|change| change.text.len()).sum()
}

struct QueueState {
  events: VecDeque<Event>,
  /// Documents whose events were dropped because the queue was full: their
  /// next change asks the extension to re-read them.
  dropped: HashSet<u64>,
  stopped: bool,
}

/// A bounded queue of events for one extension.
pub struct Queue {
  state: Mutex<QueueState>,
  ready: Condvar,
  capacity: usize,
}

impl Queue {
  pub fn new(capacity: usize) -> Self {
    Self {
      state: Mutex::new(QueueState {
        events: VecDeque::new(),
        dropped: HashSet::new(),
        stopped: false,
      }),
      ready: Condvar::new(),
      capacity: capacity.max(1),
    }
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Queues a document event, merging a change into a change of the same
  /// document that is still waiting.
  pub fn push_document(&self, event: Event) {
    let mut state = self.lock();
    if state.stopped {
      return;
    }
    let Some(document) = event.document() else {
      return;
    };
    if let Event::Changed {
      version, changes, ..
    } = event
    {
      let resync = state.dropped.remove(&document);
      let waiting = state
        .events
        .iter_mut()
        .rev()
        .find(|pending| pending.document() == Some(document));
      if let Some(Event::Changed {
        version: pending_version,
        changes: pending_changes,
        ..
      }) = waiting
      {
        *pending_version = version;
        *pending_changes = match (pending_changes.take(), changes) {
          (Some(mut earlier), Some(later))
            if !resync
              && change_bytes(&earlier) + change_bytes(&later) <= MAX_PENDING_CHANGE_BYTES =>
          {
            earlier.extend(later);
            Some(earlier)
          }
          _ => None,
        };
        return;
      }
      let event = Event::Changed {
        document,
        version,
        changes: if resync { None } else { changes },
      };
      self.push_locked(state, event, document);
      return;
    }
    self.push_locked(state, event, document);
  }

  fn push_locked(
    &self,
    mut state: std::sync::MutexGuard<'_, QueueState>,
    event: Event,
    document: u64,
  ) {
    if state.events.len() >= self.capacity {
      tracing::warn!(document, "extension event queue is full; dropping an event");
      state.dropped.insert(document);
      return;
    }
    state.events.push_back(event);
    self.ready.notify_one();
  }

  /// Queues a command. Fails if the queue is full or stopped.
  pub fn push_command(&self, event: Event) -> Result<(), String> {
    let mut state = self.lock();
    if state.stopped {
      return Err("The extension is stopping.".into());
    }
    if state.events.len() >= self.capacity {
      return Err("The extension is busy; try again.".into());
    }
    state.events.push_back(event);
    self.ready.notify_one();
    Ok(())
  }

  /// Waits for the next event. `None` once the queue is stopped.
  fn pop(&self) -> Option<Event> {
    let mut state = self.lock();
    loop {
      if state.stopped {
        return None;
      }
      if let Some(event) = state.events.pop_front() {
        return Some(event);
      }
      state = self
        .ready
        .wait(state)
        .unwrap_or_else(PoisonError::into_inner);
    }
  }

  /// Stops the queue; waiting commands fail with `reason`.
  pub fn stop(&self, reason: &str) {
    let mut state = self.lock();
    state.stopped = true;
    for event in state.events.drain(..) {
      if let Event::Command { reply, .. } = event {
        let _ = reply.send(Err(reason.to_owned()));
      }
    }
    self.ready.notify_all();
  }

  #[cfg(test)]
  fn len(&self) -> usize {
    self.lock().events.len()
  }
}

/// What a worker needs to start.
pub struct Setup {
  pub manifest: Arc<Manifest>,
  pub generation: u64,
  pub granted: BTreeSet<Permission>,
  pub config:
    Arc<std::sync::RwLock<std::collections::BTreeMap<String, crate::config::ConfigValue>>>,
  pub queue: Arc<Queue>,
  pub cancel: Arc<AtomicBool>,
}

/// The host's handle on a running worker.
pub struct Worker {
  pub generation: u64,
  pub granted: BTreeSet<Permission>,
  pub queue: Arc<Queue>,
  pub cancel: Arc<AtomicBool>,
  pub thread: Option<JoinHandle<()>>,
}

impl Worker {
  pub fn reads_documents(&self) -> bool {
    self.granted.contains(&Permission::DocumentsRead)
  }

  /// Asks the worker to finish: it deactivates the extension after the
  /// event it is processing, which is cancelled if `cancel` is set.
  pub fn stop(&self, cancel: bool) {
    self.queue.stop("The extension was stopped.");
    if cancel {
      self.cancel.store(true, Ordering::Relaxed);
    }
  }
}

pub fn spawn(shared: Weak<Shared>, setup: Setup) -> Worker {
  let worker = Worker {
    generation: setup.generation,
    granted: setup.granted.clone(),
    queue: setup.queue.clone(),
    cancel: setup.cancel.clone(),
    thread: None,
  };
  let id = setup.manifest.id.clone();
  let generation = setup.generation;
  let thread = std::thread::Builder::new()
    .name(format!("extension-{id}"))
    .spawn(move || {
      let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| run(&shared, &setup)))
        .unwrap_or_else(|_| Err("stopped because of an internal error".into()));
      if let Some(shared) = shared.upgrade() {
        shared.worker_finished(&setup.manifest.id, setup.generation, outcome);
      }
    });
  match thread {
    Ok(thread) => Worker {
      thread: Some(thread),
      ..worker
    },
    Err(error) => {
      tracing::error!(%error, extension = %id, generation, "could not start an extension thread");
      worker.queue.stop("The extension could not be started.");
      worker
    }
  }
}

/// Deadlines for a worker's calls.
#[derive(Debug, Clone, Copy)]
pub struct Budgets {
  pub activation: Duration,
  pub command: Duration,
  pub event: Duration,
  pub memory: usize,
}

fn wit_permission(permission: Permission) -> WitPermission {
  match permission {
    Permission::DocumentsRead => WitPermission::DocumentsRead,
    Permission::DocumentsWrite => WitPermission::DocumentsWrite,
  }
}

/// Runs an extension until its queue is stopped (`Ok`) or it fails.
fn run(shared: &Weak<Shared>, setup: &Setup) -> Result<(), Failure> {
  // Holding the runtime keeps the deadline ticker alive while this runs.
  let (runtime, budgets) = {
    let shared = shared.upgrade().ok_or("the editor is closing")?;
    (shared.runtime()?, shared.budgets())
  };
  let component = runtime.compile(&setup.manifest.entrypoint)?;
  let mut store = Store::new(
    runtime.engine(),
    StoreState {
      api: ApiContext::new(shared.clone(), setup),
      limiter: Limiter {
        max_memory: budgets.memory,
        memory_exceeded: false,
      },
      call: None,
      cancel: setup.cancel.clone(),
      interrupt: None,
    },
  );
  store.limiter(|state| &mut state.limiter);
  Runtime::watch(&mut store);

  let extension = guarded(&mut store, budgets.activation, "Starting", |store| {
    Extension::instantiate(&mut *store, &component, runtime.linker())
  })
  .map_err(|error| {
    if error.contains("import") || error.contains("export") {
      format!(
        "does not implement the Ferrite extension API {}: {error}",
        setup.manifest.api_version
      )
    } else {
      error
    }
  })?;

  let activation = Activation {
    granted: setup.granted.iter().copied().map(wit_permission).collect(),
  };
  guarded(&mut store, budgets.activation, "Activation", |store| {
    extension.call_activate(&mut *store, &activation)
  })?
  .map_err(|message| format!("could not start: {}", truncate(&message)))?;
  if let Some(shared) = shared.upgrade() {
    shared.worker_active(&setup.manifest.id, setup.generation);
  }

  while let Some(event) = setup.queue.pop() {
    match event {
      Event::Opened(info) => guarded(
        &mut store,
        budgets.event,
        "Handling an opened document",
        |store| extension.call_document_opened(&mut *store, &info),
      )?,
      Event::Changed {
        document,
        version,
        changes,
      } => {
        let change = DocumentChange {
          document,
          version,
          changes,
        };
        guarded(
          &mut store,
          budgets.event,
          "Handling a document change",
          |store| extension.call_document_changed(&mut *store, &change),
        )?;
      }
      Event::Closed(document) => guarded(
        &mut store,
        budgets.event,
        "Handling a closed document",
        |store| extension.call_document_closed(&mut *store, document),
      )?,
      Event::Command {
        command,
        context,
        reply,
      } => {
        let result = guarded(&mut store, budgets.command, "The command", |store| {
          extension.call_run_command(&mut *store, &command, &context)
        });
        match result {
          Ok(outcome) => {
            let _ = reply.send(outcome.map_err(|message| truncate(&message)));
          }
          Err(reason) => {
            return Err(Failure {
              reason,
              reply: Some(reply),
            });
          }
        }
      }
    }
  }

  // Stopped: let the extension clean up, briefly, unless it was cancelled.
  if !setup.cancel.load(Ordering::Relaxed) {
    let _ = guarded(&mut store, budgets.event, "Deactivation", |store| {
      extension.call_deactivate(&mut *store)
    });
  }
  Ok(())
}

/// Runs `f` with a deadline of `budget`. Failures are described for the
/// user, starting with `what`.
fn guarded<R>(
  store: &mut Store<StoreState>,
  budget: Duration,
  what: &str,
  f: impl FnOnce(&mut Store<StoreState>) -> wasmtime::Result<R>,
) -> Result<R, String> {
  store.data_mut().call = Some(Call {
    deadline: Instant::now() + budget,
  });
  store.data_mut().interrupt = None;
  store.set_epoch_deadline(1);
  let result = f(store);
  store.data_mut().call = None;
  result.map_err(|error| describe_failure(store.data(), &error, budget, what))
}

fn describe_failure(
  state: &StoreState,
  error: &wasmtime::Error,
  budget: Duration,
  what: &str,
) -> String {
  match state.interrupt {
    Some(Interrupt::Deadline) => {
      return format!(
        "{what} took longer than {} and was stopped",
        seconds(budget)
      );
    }
    Some(Interrupt::Cancelled) => return format!("{what} was cancelled"),
    None => {}
  }
  if state.limiter.memory_exceeded {
    return format!(
      "exceeded its memory limit of {} MiB",
      state.limiter.max_memory / 1024 / 1024
    );
  }
  match error.downcast_ref::<wasmtime::Trap>() {
    Some(wasmtime::Trap::StackOverflow) => "ran out of stack space (too deep recursion)".into(),
    Some(wasmtime::Trap::UnreachableCodeReached) => "crashed (it panicked or aborted)".into(),
    Some(trap) => format!("crashed: {trap}"),
    None => truncate(&format!("failed: {error:#}")),
  }
}

fn seconds(duration: Duration) -> String {
  let secs = duration.as_secs_f64();
  if secs >= 1.0 && duration.subsec_millis() == 0 {
    format!("{secs:.0} s")
  } else {
    format!("{} ms", duration.as_millis())
  }
}

pub fn truncate(message: &str) -> String {
  if message.chars().count() <= MAX_ERROR_CHARS {
    message.to_owned()
  } else {
    let mut cut: String = message.chars().take(MAX_ERROR_CHARS).collect();
    cut.push('…');
    cut
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::bindings::ferrite::extension::types::{Position, Range};

  fn change(text: &str) -> TextChange {
    let at = Position { line: 0, column: 0 };
    TextChange {
      range: Range { start: at, end: at },
      text: text.into(),
    }
  }

  fn changed(document: u64, version: u64, text: &str) -> Event {
    Event::Changed {
      document,
      version,
      changes: Some(vec![change(text)]),
    }
  }

  fn pending_changes(queue: &Queue) -> Vec<(u64, u64, Option<usize>)> {
    queue
      .lock()
      .events
      .iter()
      .filter_map(|event| match event {
        Event::Changed {
          document,
          version,
          changes,
        } => Some((*document, *version, changes.as_ref().map(Vec::len))),
        _ => None,
      })
      .collect()
  }

  #[test]
  fn merges_waiting_changes_to_the_same_document() {
    let queue = Queue::new(10);
    queue.push_document(changed(1, 2, "a"));
    queue.push_document(changed(2, 5, "x"));
    queue.push_document(changed(1, 3, "b"));
    assert_eq!(
      pending_changes(&queue),
      vec![(1, 3, Some(2)), (2, 5, Some(1))]
    );
  }

  #[test]
  fn large_merged_changes_become_a_reread() {
    let queue = Queue::new(10);
    let big = "x".repeat(MAX_PENDING_CHANGE_BYTES);
    queue.push_document(changed(1, 2, &big));
    queue.push_document(changed(1, 3, "more"));
    assert_eq!(pending_changes(&queue), vec![(1, 3, None)]);
  }

  #[test]
  fn a_full_queue_drops_events_and_asks_for_a_reread_later() {
    let queue = Queue::new(1);
    queue.push_document(Event::Closed(9));
    queue.push_document(changed(1, 2, "a"));
    assert_eq!(queue.len(), 1);
    let (reply, _receiver) = std::sync::mpsc::channel();
    let command = Event::Command {
      command: "x".into(),
      context: CommandContext {
        document: None,
        selections: Vec::new(),
      },
      reply,
    };
    assert!(
      queue.push_command(command).is_err(),
      "commands are refused when full"
    );
    assert!(matches!(queue.pop(), Some(Event::Closed(9))));
    queue.push_document(changed(1, 3, "b"));
    assert_eq!(pending_changes(&queue), vec![(1, 3, None)]);
  }

  #[test]
  fn stopping_fails_waiting_commands() {
    let queue = Queue::new(4);
    let (reply, receiver) = std::sync::mpsc::channel();
    queue
      .push_command(Event::Command {
        command: "x".into(),
        context: CommandContext {
          document: None,
          selections: Vec::new(),
        },
        reply,
      })
      .expect("queued");
    queue.stop("stopping");
    assert_eq!(receiver.recv().expect("reply"), Err("stopping".into()));
    assert!(queue.pop().is_none());
  }
}
