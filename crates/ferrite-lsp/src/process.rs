//! A server's process together with every process it starts, so stopping
//! the server stops them all (`cargo check` under rust-analyzer, `node`
//! under an npm `.cmd` shim).
//!
//! On Windows the processes share a job object that is killed when its
//! handle closes, which also happens if Ferrite crashes. On Unix they share
//! a process group, which is only signalled while the server is unreaped
//! (so its id can't have been reused); after a crash, servers rely on their
//! input closing and on watching the `processId` sent in `initialize`.

use std::io;
use std::process::{Child, Command};

pub(crate) struct ServerProcess {
  child: Child,
  #[cfg(windows)]
  job: Option<win32job::Job>,
}

impl ServerProcess {
  pub(crate) fn spawn(command: &mut Command) -> io::Result<Self> {
    #[cfg(windows)]
    {
      use std::os::windows::process::CommandExt;
      // Do not open a console window for the server.
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

  pub(crate) fn child(&mut self) -> &mut Child {
    &mut self.child
  }

  pub(crate) fn has_exited(&mut self) -> bool {
    !matches!(self.child.try_wait(), Ok(None))
  }

  /// Kills the server and everything it started, and waits for the server.
  pub(crate) fn kill(&mut self) {
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

impl Drop for ServerProcess {
  fn drop(&mut self) {
    if !self.has_exited() {
      self.kill();
    }
  }
}

/// A job that kills the server's processes when it closes. The server is
/// added just after it starts, so anything it starts in that instant
/// escapes; servers take far longer than that to get going.
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
      tracing::warn!(%error, "could not group the language server's processes");
      None
    }
  }
}
