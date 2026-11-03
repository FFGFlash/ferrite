//! Starting programs: language servers, and programs extensions run.
//!
//! A [`ProcessGroup`] is a process together with every process it starts,
//! so stopping it stops them all (`cargo check` under rust-analyzer, `node`
//! under an npm `.cmd` shim, `ssh` under `git fetch`).
//!
//! On Windows the processes share a job object that is killed when its
//! handle closes, which also happens if Ferrite crashes, and no console
//! window opens for them. On Unix they share a process group, which is only
//! signalled while the process is unreaped (so its id can't have been
//! reused); after a crash, long-running programs rely on their input
//! closing.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

pub struct ProcessGroup {
  child: Child,
  #[cfg(windows)]
  job: Option<win32job::Job>,
}

impl ProcessGroup {
  pub fn spawn(command: &mut Command) -> io::Result<Self> {
    #[cfg(windows)]
    {
      use std::os::windows::process::CommandExt;
      // Do not open a console window for it.
      const CREATE_NO_WINDOW: u32 = 0x0800_0000;
      command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
      use std::os::unix::process::CommandExt;
      command.process_group(0);
    }
    let child = command.spawn()?;
    Ok(Self {
      #[cfg(windows)]
      job: job_for(&child),
      child,
    })
  }

  pub fn child(&mut self) -> &mut Child {
    &mut self.child
  }

  pub fn has_exited(&mut self) -> bool {
    !matches!(self.child.try_wait(), Ok(None))
  }

  /// Kills the process and everything it started, and waits for it.
  pub fn kill(&mut self) {
    // Closing the job kills every process in it.
    #[cfg(windows)]
    self.job.take();
    #[cfg(unix)]
    if matches!(self.child.try_wait(), Ok(None)) {
      let group = rustix::process::Pid::from_child(&self.child);
      let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

impl Drop for ProcessGroup {
  fn drop(&mut self) {
    if !self.has_exited() {
      self.kill();
    }
  }
}

/// A job that kills the process's group when it closes. The process is
/// added just after it starts, so anything it starts in that instant
/// escapes.
#[cfg(windows)]
fn job_for(child: &Child) -> Option<win32job::Job> {
  use std::os::windows::io::AsRawHandle;
  let mut limits = win32job::ExtendedLimitInfo::new();
  limits.limit_kill_on_job_close();
  let result = win32job::Job::create_with_limit_info(&limits).and_then(|job| {
    job
      .assign_process(child.as_raw_handle() as isize)
      .map(|()| job)
  });
  match result {
    Ok(job) => Some(job),
    Err(error) => {
      tracing::warn!(%error, "could not group a program's processes");
      None
    }
  }
}

/// Finds `program` on the `PATH`, trying the usual executable extensions on
/// Windows (where npm installs programs as `.cmd` scripts). A path (with a
/// folder) is returned as it is, and so is a program that isn't found, so
/// starting it fails with "not found".
pub fn resolve_program(program: &str) -> PathBuf {
  find_on_path(program).unwrap_or_else(|| PathBuf::from(program))
}

/// Finds `program` on the `PATH`; `None` if it isn't there, or `program` is
/// a path.
pub fn find_on_path(program: &str) -> Option<PathBuf> {
  let path = Path::new(program);
  if path.components().count() > 1 {
    return None;
  }
  let extensions: &[&str] = if cfg!(windows) {
    &[".exe", ".cmd", ".bat", ".com"]
  } else {
    &[""]
  };
  let dirs = std::env::var_os("PATH")?;
  std::env::split_paths(&dirs).find_map(|dir| {
    extensions
      .iter()
      .map(|extension| dir.join(format!("{program}{extension}")))
      .find(|candidate| candidate.is_file())
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn paths_are_not_looked_up() {
    assert_eq!(find_on_path("some/tool"), None);
    assert_eq!(resolve_program("some/tool"), PathBuf::from("some/tool"));
  }

  #[test]
  fn missing_programs_stay_as_named() {
    let name = "ferrite-no-such-program-anywhere";
    assert_eq!(find_on_path(name), None);
    assert_eq!(resolve_program(name), PathBuf::from(name));
  }
}
