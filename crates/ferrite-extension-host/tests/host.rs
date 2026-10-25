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
    .run_command(&format!("test.misbehaving.{command}"), document)?
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
    .run_command("example.tidy.trimTrailingWhitespace", Some(id))
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
    .run_command("example.tidy.countWords", Some(id))
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
    .run_command("example.tidy.trimTrailingWhitespace", Some(id))
    .err()
    .expect("refused while undecided");
  assert!(refused.contains("allow or deny"), "{refused}");

  s.host
    .set_permissions("example.tidy", &[Permission::DocumentsRead]);
  let error = s
    .host
    .run_command("example.tidy.trimTrailingWhitespace", Some(id))
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
  let started = Instant::now();
  let error = run(&s, "loop", Some(id)).expect_err("stopped");
  assert!(error.contains("took longer than"), "{error}");
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
    .run_command("test.misbehaving.loop", None)
    .expect("queued");
  std::thread::sleep(Duration::from_millis(200));
  let started = Instant::now();
  s.host.shutdown();
  assert!(started.elapsed() < Duration::from_secs(3));
  assert!(ticket.wait().is_err());
}
