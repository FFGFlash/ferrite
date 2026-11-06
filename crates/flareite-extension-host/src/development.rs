//! Building development extensions: the `[build]` in their manifests.
//!
//! Each development folder with a build gets a [`Builder`]: a thread that
//! runs the build when the folder starts being watched, and again whenever
//! a file in it changes, then loads the extension again. Changes made
//! while it builds start one more build after it, not one each, and don't
//! stop the running one. The build's output goes to the extension's log.
//!
//! Changes in `.git`, to files the folder's `.gitignore` files (its own and
//! its parents', up to the repository's) ignore, and to the entry point
//! don't start a build, so a build's output doesn't start another.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant, SystemTime};

use flareite_process::ProcessGroup;
use flareite_workspace::{ExcludeFilter, WorkspaceWatcher};
use ignore::Match;
use ignore::gitignore::Gitignore;

use crate::host::Shared;
use crate::manifest::BuildPlan;
use crate::{BuildState, BuildStatus, ExtensionHost, MessageLevel, Notification};

/// How long the output of a finished build may keep coming (from programs
/// it started that still hold its output open) before it's left behind.
const OUTPUT_GRACE: Duration = Duration::from_secs(2);
/// How often a running build is checked for having finished.
const POLL: Duration = Duration::from_millis(25);

/// The builders of the development folders that have a build.
#[derive(Default)]
pub(crate) struct Builders(Mutex<BTreeMap<PathBuf, Builder>>);

/// What a development folder's manifest says about building it now.
pub(crate) enum Planned {
  Plan(BuildPlan),
  /// It has no build.
  None,
  /// Its manifest can't be read now (e.g. it's half edited): its builder,
  /// if it has one, keeps watching for it to be fixed.
  Unreadable,
}

impl Builders {
  fn lock(&self) -> MutexGuard<'_, BTreeMap<PathBuf, Builder>> {
    self.0.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Makes the builders those of `folders`: starts one for each new plan
  /// (which builds straight away), replaces those whose plan changed, and
  /// stops those of folders that are gone or no longer have a build.
  pub fn sync(&self, host: &Weak<Shared>, folders: Vec<(PathBuf, Planned)>) {
    let mut builders = self.lock();
    builders.retain(|dir, builder| {
      folders.iter().any(|(folder, planned)| {
        folder == dir
          && match planned {
            Planned::Plan(plan) => *plan == builder.inner.plan,
            Planned::None => false,
            Planned::Unreadable => true,
          }
      })
    });
    for (dir, planned) in folders {
      if let Planned::Plan(plan) = planned
        && !builders.contains_key(&dir)
      {
        builders.insert(dir, Builder::start(plan, host.clone()));
      }
    }
  }

  /// Every builder's status, by folder; those that haven't built yet are
  /// left out.
  pub fn statuses(&self) -> BTreeMap<PathBuf, BuildStatus> {
    self
      .lock()
      .iter()
      .filter_map(|(dir, builder)| Some((dir.clone(), builder.inner.lock().status.clone()?)))
      .collect()
  }

  /// Builds `dir` (again, after the build running now, if one is), and
  /// loads it after. Returns whether it has a build.
  pub fn build(&self, dir: &Path) -> bool {
    self.lock().get(dir).is_some_and(|builder| {
      builder.inner.trigger(Trigger::Requested);
      true
    })
  }

  /// Stops every builder, and their builds.
  pub fn stop_all(&self) {
    let builders = std::mem::take(&mut *self.lock());
    drop(builders);
  }
}

/// Why a build runs. A later reason replaces an earlier pending one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Trigger {
  /// The folder just started being watched: its build output may be up to
  /// date already, so it's only loaded again if the build changed it.
  Started,
  /// A file changed, or the user asked: it's always loaded again.
  Requested,
}

struct Builder {
  inner: Arc<Inner>,
  /// `None` if the folder couldn't be watched: it builds only when asked.
  _watcher: Option<WorkspaceWatcher>,
}

struct Inner {
  plan: BuildPlan,
  /// The extension id the build's output is logged under.
  log: String,
  control: Mutex<Control>,
  wake: Condvar,
}

#[derive(Default)]
struct Control {
  pending: Option<Trigger>,
  stopped: bool,
  /// The running build.
  child: Option<ProcessGroup>,
  status: Option<BuildStatus>,
  /// Whether the last finished build failed: a failure is announced once,
  /// not on every build that fails after it.
  failing: bool,
}

enum Outcome {
  Succeeded,
  Failed(String),
  /// The builder stopped while it ran.
  Stopped,
}

impl Builder {
  fn start(plan: BuildPlan, host: Weak<Shared>) -> Self {
    let log = plan
      .id
      .clone()
      .unwrap_or_else(|| plan.dir.to_string_lossy().into_owned());
    let inner = Arc::new(Inner {
      plan,
      log,
      control: Mutex::new(Control {
        pending: Some(Trigger::Started),
        ..Control::default()
      }),
      wake: Condvar::new(),
    });
    let changes = ChangeFilter::new(&inner.plan);
    let watched = Arc::downgrade(&inner);
    let watcher = WorkspaceWatcher::new(&inner.plan.dir, move |paths| {
      if let Some(inner) = watched.upgrade()
        && paths.iter().any(|path| changes.starts_build(path))
      {
        inner.trigger(Trigger::Requested);
      }
    });
    let watcher = match watcher {
      Ok(watcher) => Some(watcher),
      Err(error) => {
        tracing::warn!(extension = %inner.log, %error, "can't watch the extension's folder; it builds only when asked");
        None
      }
    };
    let worker = inner.clone();
    let spawned = std::thread::Builder::new()
      .name("extension-build".into())
      .spawn(move || run_builds(&worker, &host));
    if let Err(error) = spawned {
      tracing::error!(extension = %inner.log, %error, "could not start building the extension");
    }
    Self {
      inner,
      _watcher: watcher,
    }
  }
}

impl Drop for Builder {
  fn drop(&mut self) {
    let mut control = self.inner.lock();
    control.stopped = true;
    if let Some(mut child) = control.child.take() {
      tracing::info!(extension = %self.inner.log, "build stopped");
      child.kill();
    }
    self.inner.wake.notify_all();
  }
}

impl Inner {
  fn lock(&self) -> MutexGuard<'_, Control> {
    self.control.lock().unwrap_or_else(PoisonError::into_inner)
  }

  fn trigger(&self, trigger: Trigger) {
    let mut control = self.lock();
    if !control.stopped {
      control.pending = control.pending.max(Some(trigger));
      self.wake.notify_all();
    }
  }

  fn name(&self) -> String {
    self.plan.name.clone().unwrap_or_else(|| {
      self.plan.dir.file_name().map_or_else(
        || self.log.clone(),
        |name| name.to_string_lossy().into_owned(),
      )
    })
  }
}

/// The builder's thread: runs builds as they're asked for, until stopped.
fn run_builds(inner: &Inner, host: &Weak<Shared>) {
  let notify = |notification| {
    if let Some(shared) = host.upgrade() {
      shared.embedder.notify(notification);
    }
  };
  loop {
    let trigger = {
      let mut control = inner.lock();
      loop {
        if control.stopped {
          return;
        }
        if let Some(trigger) = control.pending.take() {
          break trigger;
        }
        control = inner
          .wake
          .wait(control)
          .unwrap_or_else(PoisonError::into_inner);
      }
    };
    // Its `[build]` was edited: loading the manifest again starts a builder
    // for the new one in place of this one, which builds straight away.
    let current = crate::manifest::read_build_plan(&inner.plan.dir);
    if current.is_ok_and(|plan| plan.as_ref() != Some(&inner.plan)) {
      if let Some(shared) = host.upgrade() {
        ExtensionHost::from_shared(shared).reload_development(&inner.plan.dir, true);
      }
      if inner.lock().stopped {
        return;
      }
    }
    inner.lock().status = Some(BuildStatus {
      state: BuildState::Building,
      error: None,
      duration_ms: None,
      log: inner.log.clone(),
    });
    notify(Notification::BuildChanged);

    let build = &inner.plan.build;
    tracing::info!(extension = %inner.log, "building: {}", command_line(&build.command, &build.args));
    let before = stamp(inner.plan.entrypoint.as_deref());
    let started = Instant::now();
    let outcome = run(inner);
    let elapsed = started.elapsed();
    let announce = {
      let mut control = inner.lock();
      if control.stopped {
        return;
      }
      let (state, error) = match &outcome {
        Outcome::Succeeded => (BuildState::Succeeded, None),
        Outcome::Failed(error) => (BuildState::Failed, Some(error.clone())),
        Outcome::Stopped => return,
      };
      let announce = state == BuildState::Failed && !control.failing;
      control.failing = state == BuildState::Failed;
      control.status = Some(BuildStatus {
        state,
        error,
        duration_ms: Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
        log: inner.log.clone(),
      });
      announce
    };
    match outcome {
      Outcome::Succeeded => {
        tracing::info!(extension = %inner.log, "built in {elapsed:.1?}");
        let changed =
          trigger == Trigger::Requested || stamp(inner.plan.entrypoint.as_deref()) != before;
        if let Some(shared) = host.upgrade() {
          ExtensionHost::from_shared(shared).reload_development(&inner.plan.dir, changed);
        }
      }
      Outcome::Failed(error) => {
        tracing::warn!(extension = %inner.log, "build failed: {error}");
        if announce {
          notify(Notification::Message {
            id: inner.log.clone(),
            extension: inner.name(),
            level: MessageLevel::Error,
            message: format!("The build failed: {error}. Its log has the output."),
          });
        }
      }
      Outcome::Stopped => {}
    }
    notify(Notification::BuildChanged);
  }
}

/// Runs the build and waits for it, logging its output.
fn run(inner: &Inner) -> Outcome {
  let plan = &inner.plan;
  let build = &plan.build;
  let program = if build.command.contains(['/', '\\']) {
    plan.dir.join(&build.command)
  } else {
    flareite_process::resolve_program(&build.command)
  };
  let mut command = Command::new(&program);
  command
    .args(&build.args)
    .current_dir(&plan.dir)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut group = match ProcessGroup::spawn(&mut command) {
    Ok(group) => group,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
      return Outcome::Failed(format!(
        "{} was not found (is it installed, and on the PATH?)",
        build.command
      ));
    }
    Err(error) => return Outcome::Failed(format!("{} could not start: {error}", build.command)),
  };
  let (done, finished) = mpsc::channel();
  let mut readers = 0;
  let stdout = group
    .child()
    .stdout
    .take()
    .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>);
  let stderr = group
    .child()
    .stderr
    .take()
    .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>);
  for pipe in [stdout, stderr].into_iter().flatten() {
    let log = inner.log.clone();
    let done = done.clone();
    let spawned = std::thread::Builder::new()
      .name("extension-build-output".into())
      .spawn(move || {
        forward(&log, pipe);
        let _ = done.send(());
      });
    readers += usize::from(spawned.is_ok());
  }
  {
    let mut control = inner.lock();
    if control.stopped {
      group.kill();
      return Outcome::Stopped;
    }
    control.child = Some(group);
  }
  let status = loop {
    {
      let mut control = inner.lock();
      let Some(group) = control.child.as_mut() else {
        return Outcome::Stopped;
      };
      match group.child().try_wait() {
        Ok(Some(status)) => {
          control.child = None;
          break Ok(status);
        }
        Ok(None) => {}
        Err(error) => {
          control.child = None;
          break Err(error);
        }
      }
    }
    std::thread::sleep(POLL);
  };
  // The rest of its output, so the log has it before the outcome.
  let deadline = Instant::now() + OUTPUT_GRACE;
  for _ in 0..readers {
    let left = deadline.saturating_duration_since(Instant::now());
    if finished.recv_timeout(left).is_err() {
      break;
    }
  }
  match status {
    Ok(status) if status.success() => Outcome::Succeeded,
    Ok(status) => Outcome::Failed(describe_exit(&build.command, status)),
    Err(error) => Outcome::Failed(format!(
      "{} could not be waited for: {error}",
      build.command
    )),
  }
}

/// Logs each line of `pipe` until it closes.
fn forward(log: &str, pipe: Box<dyn Read + Send>) {
  let mut reader = BufReader::new(pipe);
  let mut line = Vec::new();
  loop {
    line.clear();
    match reader.read_until(b'\n', &mut line) {
      Ok(0) | Err(_) => return,
      Ok(_) => {
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        if !text.is_empty() {
          tracing::debug!(extension = %log, "{text}");
        }
      }
    }
  }
}

fn describe_exit(command: &str, status: ExitStatus) -> String {
  match status.code() {
    Some(code) => format!("{command} exited with code {code}"),
    None => format!("{command} was stopped ({status})"),
  }
}

fn command_line(command: &str, args: &[String]) -> String {
  std::iter::once(command)
    .chain(args.iter().map(String::as_str))
    .map(|part| {
      if part.is_empty() || part.contains(char::is_whitespace) {
        format!("\"{part}\"")
      } else {
        part.to_owned()
      }
    })
    .collect::<Vec<_>>()
    .join(" ")
}

/// When the entry point was last written, and its size.
fn stamp(path: Option<&Path>) -> Option<(SystemTime, u64)> {
  let metadata = std::fs::metadata(path?).ok()?;
  Some((metadata.modified().ok()?, metadata.len()))
}

/// Which changed paths start a build.
struct ChangeFilter {
  dir: PathBuf,
  /// The folder's real path, as some systems report changes in it.
  canonical: Option<PathBuf>,
  /// The `.gitignore` files that apply, the folder's own first.
  ignores: Vec<Gitignore>,
  /// The manifest's `watch`: if set, only paths it matches count.
  watch: Option<ExcludeFilter>,
  entrypoint: Option<PathBuf>,
}

impl ChangeFilter {
  fn new(plan: &BuildPlan) -> Self {
    let mut ignores = Vec::new();
    for folder in plan.dir.ancestors() {
      let file = folder.join(".gitignore");
      if file.is_file() {
        let (gitignore, error) = Gitignore::new(&file);
        if let Some(error) = error {
          tracing::debug!(file = %file.display(), %error, "part of a .gitignore was not understood");
        }
        ignores.push(gitignore);
      }
      // The repository's root: the ones above it aren't its.
      if folder.join(".git").exists() {
        break;
      }
    }
    let watch = (!plan.build.watch.is_empty())
      .then(|| ExcludeFilter::new(&plan.build.watch).ok())
      .flatten();
    Self {
      dir: plan.dir.clone(),
      canonical: std::fs::canonicalize(&plan.dir).ok(),
      ignores,
      watch,
      entrypoint: plan.entrypoint.clone(),
    }
  }

  fn starts_build(&self, changed: &Path) -> bool {
    let relative = changed
      .strip_prefix(&self.dir)
      .ok()
      .or_else(|| changed.strip_prefix(self.canonical.as_deref()?).ok());
    let Some(relative) = relative.filter(|relative| !relative.as_os_str().is_empty()) else {
      return false;
    };
    // Spelled as the folder is, which the rest is.
    let path = self.dir.join(relative);
    // A folder's own change says nothing about which file in it changed
    // (each file is reported), and may be a build writing its output.
    if path.is_dir()
      || relative.components().any(|part| part.as_os_str() == ".git")
      || self.entrypoint.as_deref() == Some(path.as_path())
    {
      return false;
    }
    // Each relative to its own folder: `ignore` asserts the path is under
    // it, comparing bytes.
    let ignored = self
      .ignores
      .iter()
      .filter_map(|gitignore| {
        let relative = path.strip_prefix(gitignore.path()).ok()?;
        Some(gitignore.matched_path_or_any_parents(relative, false))
      })
      .find(|matched| !matched.is_none());
    if matches!(ignored, Some(Match::Ignore(_))) {
      return false;
    }
    self.watch.as_ref().is_none_or(|watch| {
      path
        .ancestors()
        .take_while(|ancestor| *ancestor != self.dir.as_path())
        .any(|ancestor| watch.is_excluded(&self.dir, ancestor))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Build;

  fn plan(dir: &Path, watch: &[&str], entrypoint: Option<&str>) -> BuildPlan {
    BuildPlan {
      dir: dir.to_path_buf(),
      id: Some("example.tidy".into()),
      name: Some("Tidy".into()),
      entrypoint: entrypoint.map(|entrypoint| dir.join(entrypoint)),
      build: Build {
        command: "cargo".into(),
        args: vec!["build".into()],
        watch: watch.iter().map(|&pattern| pattern.to_owned()).collect(),
      },
    }
  }

  #[test]
  fn ignored_files_and_the_entry_point_start_no_build() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo = root.path();
    std::fs::create_dir(repo.join(".git")).expect("mkdir");
    std::fs::write(repo.join(".gitignore"), "/out/\n").expect("write");
    let dir = repo.join("ext");
    std::fs::create_dir_all(dir.join("src")).expect("mkdir");
    std::fs::write(dir.join(".gitignore"), "*.log\n").expect("write");
    let filter = ChangeFilter::new(&plan(&dir, &[], Some("dist/ext.wasm")));

    assert!(filter.starts_build(&dir.join("src/lib.rs")));
    assert!(filter.starts_build(&dir.join("extension.toml")));
    assert!(
      !filter.starts_build(&dir.join("build.log")),
      "its own .gitignore"
    );
    assert!(
      filter.starts_build(&dir.join("out/x")),
      "the parent's /out/ is the repository root's"
    );
    assert!(
      !filter.starts_build(&repo.join("out/x")),
      "outside the folder"
    );
    assert!(
      !filter.starts_build(&dir.join("dist/ext.wasm")),
      "the entry point"
    );
    assert!(!filter.starts_build(&dir.join(".git/index")));
    assert!(!filter.starts_build(&dir.join("src")), "a folder");
  }

  #[test]
  fn watch_patterns_narrow_the_changes() {
    let root = tempfile::tempdir().expect("temp dir");
    let dir = root.path().to_path_buf();
    std::fs::create_dir(dir.join(".git")).expect("mkdir");
    let filter = ChangeFilter::new(&plan(&dir, &["src", "Cargo.toml", "assets/*.svg"], None));
    assert!(filter.starts_build(&dir.join("src/lib.rs")));
    assert!(filter.starts_build(&dir.join("src/deep/mod.rs")));
    assert!(filter.starts_build(&dir.join("Cargo.toml")));
    assert!(filter.starts_build(&dir.join("assets/icon.svg")));
    assert!(!filter.starts_build(&dir.join("assets/icon.png")));
    assert!(!filter.starts_build(&dir.join("README.md")));
  }

  #[test]
  fn command_lines_quote_spaces() {
    assert_eq!(
      command_line("cargo", &["build".into(), "a b".into(), String::new()]),
      "cargo build \"a b\" \"\""
    );
  }
}
