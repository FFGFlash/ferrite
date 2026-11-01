//! Integration tests: real extensions, built from `extensions/` for
//! `wasm32-unknown-unknown`, running in the host against an editor core.
//!
//! `example.tidy` is the example extension; `test.misbehaving` breaks rules
//! and exceeds limits on request, and every test checks that the host (and
//! the document) survive it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use ferrite_core::{DocumentId, Editor, EditorOp};
use ferrite_extension_host::{
  Embedder, ExtensionHost, ExtensionState, HostOptions, Limits, Notification, Permission, Severity,
};

/// Builds the extensions workspace once and returns its output folder.
fn wasm_dir() -> &'static Path {
  static BUILT: OnceLock<PathBuf> = OnceLock::new();
  BUILT.get_or_init(|| {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
      .args(["build", "--release", "--quiet"])
      .current_dir(&root)
      .env_remove("CARGO_TARGET_DIR")
      .env_remove("CARGO_BUILD_TARGET")
      .env_remove("RUSTFLAGS")
      .env_remove("CARGO_ENCODED_RUSTFLAGS")
      .status()
      .expect("cargo runs");
    assert!(
      status.success(),
      "building the test extensions failed; is the target installed? \
       (rustup target add wasm32-unknown-unknown)"
    );
    root.join("target/wasm32-unknown-unknown/release")
  })
}

/// The application side: an editor core, and the notifications received.
#[derive(Default)]
struct Fake {
  editor: Mutex<Editor>,
  notifications: Mutex<Vec<Notification>>,
  arrived: Condvar,
}

impl Embedder for Fake {
  fn with_editor(&self, f: &mut dyn FnMut(&mut Editor)) {
    f(&mut self.editor());
  }

  fn language_of(&self, path: &Path) -> Option<String> {
    path
      .extension()
      .and_then(|extension| extension.to_str())
      .map(|extension| if extension == "rs" { "rust" } else { extension }.to_owned())
  }

  fn notify(&self, notification: Notification) {
    self
      .notifications
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .push(notification);
    self.arrived.notify_all();
  }
}

impl Fake {
  fn editor(&self) -> MutexGuard<'_, Editor> {
    self.editor.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Waits for a notification matching `wanted` (which may have arrived
  /// already) and removes it and everything before it.
  fn wait_for(&self, what: &str, wanted: impl Fn(&Notification) -> bool) -> Notification {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut notifications = self
      .notifications
      .lock()
      .unwrap_or_else(PoisonError::into_inner);
    loop {
      if let Some(index) = notifications.iter().position(&wanted) {
        let found = notifications[index].clone();
        notifications.drain(..=index);
        return found;
      }
      let now = Instant::now();
      assert!(
        now < deadline,
        "timed out waiting for {what}; got {notifications:#?}"
      );
      notifications = self
        .arrived
        .wait_timeout(notifications, deadline - now)
        .unwrap_or_else(PoisonError::into_inner)
        .0;
    }
  }

  fn wait_for_message(&self, starting: &str) -> String {
    match self.wait_for(
      starting,
      |n| matches!(n, Notification::Message { message, .. } if message.starts_with(starting)),
    ) {
      Notification::Message { message, .. } => message,
      _ => unreachable!(),
    }
  }

  /// Opens an untitled document with `text` and tells the host.
  fn open(&self, host: &ExtensionHost, text: &str) -> DocumentId {
    let mut editor = self.editor();
    let id = editor.create_untitled();
    let document = editor.document_mut(id).expect("open");
    document
      .apply(EditorOp::Insert(text.into()))
      .expect("insert");
    document
      .apply(EditorOp::SetCursor {
        offset: 0,
        extend: false,
        add: false,
      })
      .expect("cursor");
    host.sync(document);
    id
  }

  fn type_text(&self, host: &ExtensionHost, id: DocumentId, text: &str) {
    let mut editor = self.editor();
    let document = editor.document_mut(id).expect("open");
    document
      .apply(EditorOp::Insert(text.into()))
      .expect("insert");
    host.sync(document);
  }

  fn text(&self, id: DocumentId) -> String {
    self
      .editor()
      .document(id)
      .expect("open")
      .buffer()
      .contents()
  }
}

/// Copies a built extension into `dir` with its manifest, edited by `edit`.
fn install(
  dir: &Path,
  crate_name: &str,
  manifest_dir: &str,
  edit: impl FnOnce(String) -> String,
) -> PathBuf {
  let source = Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("../../extensions")
    .join(manifest_dir);
  let target = dir.join(crate_name);
  std::fs::create_dir_all(&target).expect("mkdir");
  let wasm = format!("{crate_name}.wasm");
  std::fs::copy(wasm_dir().join(&wasm), target.join(&wasm)).expect("copy wasm");
  let manifest = std::fs::read_to_string(source.join("extension.toml")).expect("manifest");
  let manifest = manifest
    .lines()
    .map(|line| {
      if line.starts_with("entrypoint") {
        format!("entrypoint = \"{wasm}\"")
      } else {
        line.to_owned()
      }
    })
    .collect::<Vec<_>>()
    .join("\n");
  std::fs::write(target.join("extension.toml"), edit(manifest)).expect("write manifest");
  target
}

struct Setup {
  _dir: tempfile::TempDir,
  fake: Arc<Fake>,
  host: ExtensionHost,
}

fn limits() -> Limits {
  Limits {
    memory_bytes: 32 * 1024 * 1024,
    total_memory_bytes: 48 * 1024 * 1024,
    activation_time: Duration::from_secs(10),
    command_time: Duration::from_millis(1500),
    event_time: Duration::from_millis(1500),
    queue_capacity: 64,
  }
}

/// A host with one extension installed as a development extension.
/// `grant` lists the permissions granted up front (`None`: undecided).
fn setup(
  crate_name: &str,
  manifest_dir: &str,
  grant: Option<&[Permission]>,
  settings: &str,
  edit: impl FnOnce(String) -> String,
) -> Setup {
  let dir = tempfile::tempdir().expect("temp dir");
  let extension = install(dir.path(), crate_name, manifest_dir, edit);
  let permissions_file = dir.path().join("permissions.json");
  let fake = Arc::new(Fake::default());
  let settings: BTreeMap<String, toml::Table> = toml::from_str(settings).expect("settings");
  let host = ExtensionHost::new(
    fake.clone(),
    HostOptions {
      editor_version: semver::Version::new(0, 1, 0),
      extensions_dir: None,
      development: vec![extension],
      permissions_file: Some(permissions_file),
      settings,
      limits: limits(),
    },
  );
  if let Some(granted) = grant {
    // Decisions are made per loaded extension; load once to record them.
    host.load();
    let id = host.statuses()[0].id.clone();
    host.set_permissions(&id, granted);
  }
  Setup {
    _dir: dir,
    fake,
    host,
  }
}

const ALL: &[Permission] = &[Permission::DocumentsRead, Permission::DocumentsWrite];

fn misbehaving(settings: &str) -> Setup {
  setup(
    "misbehaving",
    "test-fixtures/misbehaving",
    Some(ALL),
    settings,
    |m| m,
  )
}

fn state_of(host: &ExtensionHost) -> (ExtensionState, Option<String>) {
  let status = &host.statuses()[0];
  (status.state, status.error.clone())
}

fn run(setup: &Setup, command: &str, document: Option<DocumentId>) -> Result<(), String> {
  setup
    .host
    .run_command(&format!("test.misbehaving.{command}"), document, None)?
    .wait()
}

#[test]
fn tidy_flags_and_trims_trailing_whitespace() {
  let s = setup("tidy", "tidy", Some(ALL), "", |m| m);
  let id = s.fake.open(&s.host, "fn main() {   \n  ok();\n}\n");
  s.host.load();
  s.fake.wait_for("diagnostics", |n| {
    *n == Notification::DiagnosticsChanged(id)
  });
  let diagnostics = {
    let editor = s.fake.editor();
    let document = editor.document(id).expect("open");
    s.host.diagnostics(document, 0..document.buffer().len())
  };
  assert_eq!(diagnostics.len(), 1);
  assert_eq!(diagnostics[0].range, 11..14);
  assert_eq!(diagnostics[0].severity, Severity::Warning);
  assert_eq!(diagnostics[0].source, "Tidy");
  assert_eq!(diagnostics[0].code.as_deref(), Some("trailing-whitespace"));

  s.host
    .run_command("example.tidy.trimTrailingWhitespace", Some(id), None)
    .expect("queued")
    .wait()
    .expect("trimmed");
  assert_eq!(s.fake.text(id), "fn main() {\n  ok();\n}\n");
  s.fake
    .wait_for("edit", |n| *n == Notification::DocumentEdited(id));
  s.fake
    .wait_for_message("Trimmed trailing whitespace from 1 line.");

  // The extension hears about its own edit and clears the diagnostic.
  s.fake.wait_for("cleared diagnostics", |n| {
    *n == Notification::DiagnosticsChanged(id)
  });
  let editor = s.fake.editor();
  let document = editor.document(id).expect("open");
  assert!(
    s.host
      .diagnostics(document, 0..document.buffer().len())
      .is_empty()
  );
}

#[test]
fn tidy_rechecks_only_changed_lines_and_counts_words() {
  let s = setup(
    "tidy",
    "tidy",
    Some(ALL),
    r#"[ "example.tidy" ]
max_line_length = 10
"#,
    |m| m,
  );
  let id = s.fake.open(&s.host, "short\n");
  s.host.load();
  s.fake.wait_for("diagnostics", |n| {
    *n == Notification::DiagnosticsChanged(id)
  });
  // Typing at the start of the first line makes it too long.
  s.fake.type_text(&s.host, id, "much longer ");
  s.fake.wait_for("new diagnostics", |n| {
    *n == Notification::DiagnosticsChanged(id)
  });
  let diagnostics = {
    let editor = s.fake.editor();
    let document = editor.document(id).expect("open");
    s.host.diagnostics(document, 0..document.buffer().len())
  };
  assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
  assert!(
    diagnostics[0]
      .message
      .contains("17 characters long (limit 10)")
  );

  s.host
    .run_command("example.tidy.countWords", Some(id), None)
    .expect("queued")
    .wait()
    .expect("counted");
  s.fake.wait_for_message("3 words in 2 lines.");
}

#[test]
fn extensions_wait_for_permission_and_denied_permissions_are_enforced() {
  let s = setup("tidy", "tidy", None, "", |m| m);
  let id = s.fake.open(&s.host, "x  \n");
  s.host.load();
  assert_eq!(state_of(&s.host).0, ExtensionState::AwaitingPermission);
  let refused = s
    .host
    .run_command("example.tidy.trimTrailingWhitespace", Some(id), None)
    .err()
    .expect("refused while undecided");
  assert!(refused.contains("allow or deny"), "{refused}");

  s.host
    .set_permissions("example.tidy", &[Permission::DocumentsRead]);
  let error = s
    .host
    .run_command("example.tidy.trimTrailingWhitespace", Some(id), None)
    .expect("queued")
    .wait()
    .expect_err("editing is not allowed");
  assert!(error.contains("not allowed to edit documents"), "{error}");
  assert_eq!(s.fake.text(id), "x  \n", "the document is unchanged");
  assert_eq!(state_of(&s.host).0, ExtensionState::Active);
}

#[test]
fn document_changes_are_sent_as_incremental_utf8_changes() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "héllo\nworld");
  s.host.load();
  // It activates for its commands; open documents are announced first.
  run(&s, "read", Some(id)).expect("ran");
  s.fake.wait_for_message(&format!("opened {}", id.get()));
  {
    let mut editor = s.fake.editor();
    let document = editor.document_mut(id).expect("open");
    document
      .apply(EditorOp::SetCursor {
        offset: 7,
        extend: false,
        add: false,
      })
      .expect("cursor");
    document
      .apply(EditorOp::Insert("big ".into()))
      .expect("insert");
    s.host.sync(document);
  }
  let message = s.fake.wait_for_message(&format!("changed {}", id.get()));
  assert!(message.ends_with("[1:0-1:0=\"big \"]"), "{message}");

  s.host.closed(id);
  s.fake.wait_for_message(&format!("closed {}", id.get()));
}

#[test]
fn edits_are_validated_and_applied_atomically() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "abcdef");
  s.host.load();

  run(&s, "editStale", Some(id)).expect("ran");
  assert!(s.fake.wait_for_message("edit:").contains("StaleVersion"));
  run(&s, "editBadRange", Some(id)).expect("ran");
  assert!(s.fake.wait_for_message("edit:").contains("InvalidRange"));
  run(&s, "editOverlapping", Some(id)).expect("ran");
  assert!(s.fake.wait_for_message("edit:").contains("edits overlap"));
  assert_eq!(s.fake.text(id), "abcdef", "rejected edits change nothing");

  run(&s, "edit", Some(id)).expect("ran");
  assert!(s.fake.wait_for_message("edit:").contains("Ok("));
  assert_eq!(s.fake.text(id), "Xabcdef");
  // One undo step.
  s.fake
    .editor()
    .document_mut(id)
    .expect("open")
    .apply(EditorOp::Undo)
    .expect("undo");
  assert_eq!(s.fake.text(id), "abcdef");
}

#[test]
fn extensions_have_no_filesystem_environment_or_arguments() {
  let s = misbehaving("");
  s.host.load();
  run(&s, "escape", None).expect("ran");
  let message = s.fake.wait_for_message("file:");
  assert!(message.starts_with("file: Err("), "{message}");
  assert!(message.contains("env: Err(NotPresent)"), "{message}");
  assert!(message.ends_with("args: 0"), "{message}");
}

#[test]
fn an_infinite_loop_is_stopped_and_the_extension_can_be_restarted() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "text");
  s.host.load();
  // Start (and compile) the extension first, so only the loop is timed.
  run(&s, "read", Some(id)).expect("started");
  let started = Instant::now();
  let error = run(&s, "loop", Some(id)).expect_err("stopped");
  assert!(error.contains("took longer than"), "{error}");
  // The 1.5 s command deadline, with room for a loaded test machine.
  assert!(started.elapsed() < Duration::from_secs(10));
  let failed = s.fake.wait_for("failure", |n| {
    matches!(n, Notification::ExtensionFailed { .. })
  });
  assert!(
    matches!(failed, Notification::ExtensionFailed { reason, .. } if reason.contains("took longer"))
  );
  assert_eq!(state_of(&s.host).0, ExtensionState::Failed);
  let refused = run(&s, "read", Some(id)).expect_err("failed extensions don't run");
  assert!(refused.contains("Restart"), "{refused}");

  s.host.restart("test.misbehaving");
  run(&s, "read", Some(id)).expect("works again");
  assert!(s.fake.wait_for_message("read:").contains("[\"text\"]"));
  assert_eq!(s.fake.text(id), "text");
}

#[test]
fn resource_use_is_accounted_per_extension() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "text");
  s.host.load();
  run(&s, "read", Some(id)).expect("ran");
  let usage = s.host.statuses()[0].usage.clone();
  // Activation, the opened document, and the command.
  assert!(usage.calls >= 3, "{usage:?}");
  assert!(usage.busy_ms > 0.0 && usage.slowest_call_ms <= usage.busy_ms);
  assert!(usage.compile_ms > 0.0);
  assert!(usage.memory_bytes > 0 && usage.peak_memory_bytes >= usage.memory_bytes);

  run(&s, "alloc", None).expect_err("exceeds its memory");
  let after = s.host.statuses()[0].usage.clone();
  assert_eq!(after.memory_bytes, 0, "a stopped instance holds no memory");
  assert!(after.peak_memory_bytes >= usage.peak_memory_bytes);
  assert!(after.calls > usage.calls);

  // Totals survive a restart.
  s.host.restart("test.misbehaving");
  run(&s, "read", Some(id)).expect("works again");
  assert!(s.host.statuses()[0].usage.calls > after.calls);
}

#[test]
fn memory_stack_and_panics_end_the_instance_with_a_clear_reason() {
  for (command, expected) in [
    ("alloc", "memory limit of 32 MiB"),
    ("recurse", "stack space"),
    ("panic", "panicked"),
  ] {
    let s = misbehaving("");
    s.host.load();
    let error = run(&s, command, None).expect_err("fails");
    assert!(error.contains(expected), "{command}: {error}");
    let (state, reason) = state_of(&s.host);
    assert_eq!(state, ExtensionState::Failed);
    assert!(reason.is_some_and(|reason| reason.contains(expected)));
  }
}

#[test]
fn output_floods_are_limited() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "x");
  s.host.load();
  run(&s, "flood", Some(id)).expect("ran");
  assert!(s.fake.wait_for_message("diagnostics:").contains("TooLarge"));
  assert_eq!(state_of(&s.host).0, ExtensionState::Active);
}

#[test]
fn diagnostics_are_removed_when_an_extension_fails() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "abc");
  s.host.load();
  run(&s, "diagnose", Some(id)).expect("ran");
  assert!(s.fake.wait_for_message("diagnostics:").contains("Ok(())"));
  let count = || {
    let editor = s.fake.editor();
    let document = editor.document(id).expect("open");
    s.host.diagnostics_at(document, 0).len()
  };
  assert_eq!(count(), 1);
  run(&s, "panic", Some(id)).expect_err("fails");
  assert_eq!(count(), 0);
}

#[test]
fn activation_failures_and_slow_handlers_are_reported() {
  let refused = misbehaving("[\"test.misbehaving\"]\non_activate = \"error\"\n");
  refused.host.load();
  run(&refused, "read", None).expect_err("activation fails");
  let (state, reason) = state_of(&refused.host);
  assert_eq!(state, ExtensionState::Failed);
  assert!(reason.is_some_and(|reason| reason.contains("activation refused")));

  let slow = misbehaving("[\"test.misbehaving\"]\non_change = \"loop\"\n");
  let id = slow.fake.open(&slow.host, "a");
  slow.host.load();
  run(&slow, "read", Some(id)).expect("starts");
  slow.fake.type_text(&slow.host, id, "b");
  let failed = slow.fake.wait_for("failure", |n| {
    matches!(n, Notification::ExtensionFailed { .. })
  });
  assert!(
    matches!(&failed, Notification::ExtensionFailed { reason, .. } if reason.contains("Handling a document change took longer")),
    "{failed:?}"
  );
}

#[test]
fn configuration_comes_from_settings_with_declared_defaults() {
  let s = misbehaving("[\"test.misbehaving\"]\nlimit = 7\non_change = \"sideways\"\n");
  s.host.load();
  run(&s, "config", None).expect("ran");
  let message = s.fake.wait_for_message("on_change:");
  assert_eq!(
    message,
    "on_change: report; limit: Some(ConfigValue::Integer(7)); unknown: None"
  );
  let errors = &s.host.statuses()[0].config_errors;
  assert_eq!(errors.len(), 1);
  assert!(errors[0].contains("on_change"), "{errors:?}");
}

#[test]
fn invalid_manifests_and_foreign_components_are_reported() {
  let bad = setup("misbehaving", "test-fixtures/misbehaving", None, "", |m| {
    m.replace("api_version = \"1\"", "api_version = \"7\"")
  });
  bad.host.load();
  let (state, reason) = state_of(&bad.host);
  assert_eq!(state, ExtensionState::Invalid);
  assert!(reason.is_some_and(|reason| reason.contains("API version 7")));

  // A component that imports an API version this host doesn't provide.
  let foreign = setup(
    "misbehaving",
    "test-fixtures/misbehaving",
    Some(ALL),
    "",
    |m| m,
  );
  let component =
    wat::parse_str("(component (import \"ferrite:extension/host@9.0.0\" (instance)))")
      .expect("wat");
  let dir = PathBuf::from(&foreign.host.statuses()[0].path);
  std::fs::write(dir.join("misbehaving.wasm"), component).expect("write");
  foreign.host.load();
  let error = run(&foreign, "read", None).expect_err("cannot start");
  assert!(
    error.contains("does not implement the Ferrite extension API"),
    "{error}"
  );
}

#[test]
fn shutdown_stops_busy_extensions_promptly() {
  let s = misbehaving("");
  s.host.load();
  let ticket = s
    .host
    .run_command("test.misbehaving.loop", None, None)
    .expect("queued");
  std::thread::sleep(Duration::from_millis(200));
  let started = Instant::now();
  s.host.shutdown();
  assert!(started.elapsed() < Duration::from_secs(3));
  assert!(ticket.wait().is_err());
}

#[test]
fn extensions_fill_status_items_views_and_decorations() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "abcdef");
  s.host.load();
  // The manifest's text shows before the extension runs.
  let items = s.host.status_items();
  assert_eq!(items.len(), 1);
  assert_eq!(items[0].id, "test.misbehaving.state");
  assert_eq!(items[0].content.text, "idle");

  run(&s, "ui", Some(id)).expect("ran");
  // In the order the extension made the calls.
  s.fake
    .wait_for("status", |n| *n == Notification::StatusItemsChanged);
  s.fake.wait_for("decorations", |n| {
    *n == Notification::DecorationsChanged(id)
  });
  let reveal = s
    .fake
    .wait_for("reveal", |n| matches!(n, Notification::Reveal { .. }));
  assert!(
    matches!(reveal, Notification::Reveal { document, end, .. } if document == id && end.column == 2)
  );
  let view = Notification::ShowView("test.misbehaving.items".into());
  s.fake.wait_for("show view", |n| *n == view);
  let report = s.fake.wait_for_message("ui:");
  assert_eq!(report, "ui: Ok(()) Ok(()) Ok(()) Ok(()) Ok(()) Ok(())");
  let items = s.host.status_items();
  assert_eq!(items[0].content.text, "busy");
  assert_eq!(
    items[0].content.command.as_deref(),
    Some("test.misbehaving.argument")
  );
  let view = s.host.view_content("test.misbehaving.items");
  assert_eq!(view.message.as_deref(), Some("Two items"));
  assert_eq!(view.items.len(), 2);
  assert_eq!(view.items[1].parent.as_deref(), Some("a"));
  let decorations = {
    let editor = s.fake.editor();
    s.host.decorations(editor.document(id).expect("open"), 0..6)
  };
  assert_eq!(decorations.len(), 1);
  assert_eq!(decorations[0].range, 0..2);
  assert_eq!(decorations[0].key, "test.misbehaving/mark");

  // Decorations move with the text.
  s.fake.type_text(&s.host, id, "12");
  let editor = s.fake.editor();
  let moved = s.host.decorations(editor.document(id).expect("open"), 0..8);
  assert_eq!(moved[0].range, 2..4);
}

#[test]
fn commands_receive_their_argument() {
  let s = misbehaving("");
  s.host.load();
  s.host
    .run_command("test.misbehaving.argument", None, Some("item b".into()))
    .expect("queued")
    .wait()
    .expect("ran");
  assert_eq!(
    s.fake.wait_for_message("argument:"),
    "argument: Some(\"item b\")"
  );
  run(&s, "argument", None).expect("ran");
  assert_eq!(s.fake.wait_for_message("argument:"), "argument: None");
}

#[test]
fn workbench_misuse_is_rejected_and_output_is_removed_on_failure() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "abc");
  s.host.load();
  run(&s, "uiErrors", Some(id)).expect("ran");
  let report = s.fake.wait_for_message("uiErrors:");
  assert_eq!(report.matches("InvalidArgument").count(), 5, "{report}");

  run(&s, "ui", Some(id)).expect("ran");
  s.fake.wait_for_message("ui:");
  run(&s, "panic", Some(id)).expect_err("fails");
  assert!(
    s.host.status_items().is_empty(),
    "a failed extension's items are hidden"
  );
  assert!(
    s.host
      .view_content("test.misbehaving.items")
      .items
      .is_empty()
  );
  let editor = s.fake.editor();
  assert!(
    s.host
      .decorations(editor.document(id).expect("open"), 0..3)
      .is_empty()
  );
}

#[test]
fn decorations_need_the_read_permission() {
  let s = misbehaving("");
  let id = s.fake.open(&s.host, "abc");
  s.host.load();
  s.host.set_permissions("test.misbehaving", &[]);
  run(&s, "ui", Some(id)).expect("ran");
  let report = s.fake.wait_for_message("ui:");
  assert!(report.contains("PermissionDenied"), "{report}");
  // Status items and views need no permission.
  assert!(report.starts_with("ui: Ok(()) Ok(()) Ok(())"), "{report}");
}

#[test]
fn showing_a_view_activates_its_extension() {
  let s = misbehaving("");
  s.host.load();
  assert_eq!(state_of(&s.host).0, ExtensionState::Inactive);
  s.host.view_opened("test.misbehaving.items");
  s.fake.wait_for_message("activated with");
}

#[test]
fn extensions_built_against_api_1_0_still_run() {
  let s = setup("tidy", "tidy", Some(ALL), "", |m| m);
  let dir = PathBuf::from(&s.host.statuses()[0].path);
  let old = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tidy-api-1.0.wasm");
  std::fs::copy(old, dir.join("tidy.wasm")).expect("copy");
  let id = s.fake.open(&s.host, "x  \n");
  s.host.load();
  s.fake.wait_for("diagnostics", |n| {
    *n == Notification::DiagnosticsChanged(id)
  });
  assert_eq!(state_of(&s.host).0, ExtensionState::Active);
}

#[test]
fn theme_only_extensions_need_no_code() {
  let dir = tempfile::tempdir().expect("temp dir");
  let extension = dir.path().join("themes");
  std::fs::create_dir_all(extension.join("themes")).expect("mkdir");
  std::fs::write(
    extension.join("extension.toml"),
    "id = \"example.themes\"\nname = \"Themes\"\nversion = \"1.0.0\"\napi_version = \"1\"\n\
     [[themes]]\nid = \"night\"\nlabel = \"Night\"\nkind = \"dark\"\npath = \"themes/night.toml\"\n",
  )
  .expect("manifest");
  let theme = extension.join("themes/night.toml");
  std::fs::write(&theme, "[colors]\neditor-background = \"#101020\"\n").expect("theme");
  let fake = Arc::new(Fake::default());
  let options = HostOptions {
    editor_version: semver::Version::new(0, 1, 0),
    extensions_dir: Some(dir.path().to_path_buf()),
    development: Vec::new(),
    permissions_file: None,
    settings: BTreeMap::new(),
    limits: limits(),
  };
  let host = ExtensionHost::new(fake, options);
  host.load();
  let status = &host.statuses()[0];
  assert_eq!(status.state, ExtensionState::Ready);
  let themes = &status.contributions.themes;
  assert_eq!(themes[0].id, "example.themes.night");
  assert_eq!(themes[0].colors["editor-background"], "#101020");

  std::fs::write(&theme, "[colors]\neditor-background = \"url(evil)\"\n").expect("theme");
  host.load();
  let status = &host.statuses()[0];
  assert_eq!(status.state, ExtensionState::Invalid);
  assert!(
    status
      .error
      .as_deref()
      .is_some_and(|e| e.contains("hex color"))
  );
}

#[test]
fn the_icons_example_contributes_an_icon_theme() {
  let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/icons");
  let fake = Arc::new(Fake::default());
  let options = HostOptions {
    editor_version: semver::Version::new(0, 1, 0),
    extensions_dir: None,
    development: vec![example],
    permissions_file: None,
    settings: BTreeMap::new(),
    limits: limits(),
  };
  let host = ExtensionHost::new(fake, options);
  host.load();
  let status = &host.statuses()[0];
  assert_eq!(status.state, ExtensionState::Ready, "{:?}", status.error);
  assert_eq!(
    status.contributions.icon_themes[0].id,
    "example.icons.colorful"
  );

  let theme = host
    .icon_theme("example.icons.colorful")
    .expect("an icon theme");
  assert_eq!(theme.associations.file_extensions["rs"], "rust");
  assert_eq!(theme.associations.folder_names["src"], "folder-src");
  assert_eq!(theme.light.folder.as_deref(), Some("folder-light"));
  assert!(theme.icons["rust"].starts_with("data:image/svg+xml;base64,"));
  assert!(host.icon_theme("example.icons.missing").is_none());

  host.set_enabled("example.icons", false);
  assert!(host.icon_theme("example.icons.colorful").is_none());
}

#[test]
fn the_language_examples_load_and_declare_their_languages_and_servers() {
  let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions");
  let folders = ["rust", "typescript", "javascript", "json", "toml", "python"];
  let options = HostOptions {
    editor_version: semver::Version::new(0, 1, 0),
    extensions_dir: None,
    development: folders.iter().map(|folder| examples.join(folder)).collect(),
    permissions_file: None,
    settings: BTreeMap::new(),
    limits: limits(),
  };
  let host = ExtensionHost::new(Arc::new(Fake::default()), options);
  host.load();
  for status in host.statuses() {
    assert_ne!(
      status.state,
      ExtensionState::Invalid,
      "{}: {:?}",
      status.name,
      status.error
    );
  }
  let mut languages: Vec<String> = host.languages().into_iter().map(|l| l.id).collect();
  languages.sort();
  assert_eq!(
    languages,
    [
      "javascript",
      "json",
      "python",
      "rust",
      "toml",
      "typescript",
      "typescriptreact"
    ]
  );
  for language in host.languages() {
    assert!(language.grammar.is_file(), "{}", language.grammar.display());
  }
  // Servers wait for the user's permission.
  assert!(host.language_servers().is_empty());
  for id in [
    "example.rust",
    "example.typescript",
    "example.javascript",
    "example.python",
  ] {
    host.set_permissions(id, &[Permission::LanguageServers]);
  }
  let mut servers: Vec<String> = host
    .language_servers()
    .into_iter()
    .map(|s| s.name)
    .collect();
  servers.sort();
  assert_eq!(
    servers,
    [
      "pyright",
      "rust-analyzer",
      "typescript-language-server",
      "typescript-language-server"
    ],
    "JavaScript and TypeScript declare the same server, which settings merge"
  );
}

#[test]
fn the_html_css_example_contributes_languages_and_servers_once_allowed() {
  let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/html-css");
  let dir = tempfile::tempdir().expect("temp dir");
  let fake = Arc::new(Fake::default());
  let options = HostOptions {
    editor_version: semver::Version::new(0, 1, 0),
    extensions_dir: None,
    development: vec![example.clone()],
    permissions_file: Some(dir.path().join("permissions.json")),
    settings: BTreeMap::new(),
    limits: limits(),
  };
  let host = ExtensionHost::new(fake.clone(), options);
  host.load();
  let status = &host.statuses()[0];
  assert_eq!(status.id, "example.html-css");
  assert_eq!(
    status.state,
    ExtensionState::AwaitingPermission,
    "{:?}",
    status.error
  );
  let permission = &status.permissions[0];
  assert_eq!(permission.permission, Permission::LanguageServers);
  assert!(
    permission
      .description
      .contains("vscode-html-language-server --stdio; vscode-css-language-server --stdio"),
    "{}",
    permission.description
  );

  // Languages don't wait for the permission; servers do.
  let languages = host.languages();
  let ids: Vec<&str> = languages.iter().map(|l| l.id.as_str()).collect();
  assert_eq!(ids, ["html", "css"]);
  assert_eq!(
    languages[0].grammar,
    example.join("grammars/tree-sitter-html.wasm")
  );
  assert!(languages[0].grammar.is_file());
  assert!(host.language_servers().is_empty());

  host.set_permissions("example.html-css", &[Permission::LanguageServers]);
  fake.wait_for("a state change", |n| {
    matches!(n, Notification::ExtensionsChanged)
  });
  assert_eq!(host.statuses()[0].state, ExtensionState::Ready);
  let servers = host.language_servers();
  assert_eq!(servers.len(), 2);
  assert_eq!(servers[0].command, "vscode-html-language-server");
  assert_eq!(servers[1].languages, ["css"]);

  host.set_permissions("example.html-css", &[]);
  assert!(host.language_servers().is_empty());
  assert_eq!(host.statuses()[0].state, ExtensionState::Ready, "decided");
  host.set_enabled("example.html-css", false);
  assert!(host.languages().is_empty());
}

#[test]
fn todos_example_highlights_lists_and_counts_markers() {
  let s = setup("todos", "todos", Some(ALL), "", |m| m);
  let id = s.fake.open(
    &s.host,
    "fn main() {\n  // TODO: tidy up\n}\n// FIXME later\n",
  );
  s.host.load();
  s.fake.wait_for("decorations", |n| {
    *n == Notification::DecorationsChanged(id)
  });
  s.fake.wait_for("view", |n| {
    *n == Notification::ViewChanged("example.todos.list".into())
  });
  let decorations = {
    let editor = s.fake.editor();
    let document = editor.document(id).expect("open");
    s.host.decorations(document, 0..document.buffer().len())
  };
  let keys: Vec<_> = decorations
    .iter()
    .map(|d| (d.range.clone(), d.key.as_str()))
    .collect();
  assert_eq!(
    keys,
    [
      (17..21, "example.todos/keyword"),
      (36..41, "example.todos/keyword")
    ]
  );
  let view = s.host.view_content("example.todos.list");
  assert_eq!(view.items.len(), 3, "{view:?}");
  assert_eq!(view.items[1].label, "TODO: tidy up");
  let reveal = view.items[1].argument.clone();
  s.fake
    .wait_for("count", |n| *n == Notification::StatusItemsChanged);
  assert_eq!(s.host.status_items()[0].content.text, "☐ 2 TODOs");

  // Selecting the item reveals the marker.
  s.host
    .run_command("example.todos.reveal", None, reveal)
    .expect("queued")
    .wait()
    .expect("revealed");
  let revealed = s
    .fake
    .wait_for("reveal", |n| matches!(n, Notification::Reveal { .. }));
  assert!(
    matches!(revealed, Notification::Reveal { start, .. } if start.line == 1 && start.column == 5)
  );
}
