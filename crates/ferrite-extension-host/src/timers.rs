//! The host functions of the `timers` interface (API 1.10): running one of
//! the extension's commands later, on a thread of its own that waits.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::bindings::ferrite::extension::timers::Host;
use crate::bindings::ferrite::extension::types::ApiError;
use crate::runtime::StoreState;
use crate::worker::Payload;

/// Most timers an extension has waiting.
pub const MAX_TIMERS: usize = 16;
const MIN_DELAY_MS: u32 = 1_000;
const MAX_DELAY_MS: u32 = 24 * 60 * 60 * 1_000;

/// A timer waiting to run its command.
pub struct Timer {
  pub id: u64,
  pub extension: String,
  /// Set to cancel it; its thread waits on the condition variable.
  cancelled: Arc<(Mutex<bool>, Condvar)>,
}

impl Timer {
  pub fn new(id: u64, extension: String) -> Self {
    Self {
      id,
      extension,
      cancelled: Arc::default(),
    }
  }

  pub fn cancel(&self) {
    let (flag, wake) = &*self.cancelled;
    *flag.lock().unwrap_or_else(PoisonError::into_inner) = true;
    wake.notify_all();
  }

  /// Waits `delay`; returns false if the timer was cancelled meanwhile.
  pub fn waiter(&self, delay: Duration) -> impl FnOnce() -> bool + Send + 'static {
    let cancelled = self.cancelled.clone();
    move || {
      let due = Instant::now() + delay;
      let (flag, wake) = &*cancelled;
      let mut stopped = flag.lock().unwrap_or_else(PoisonError::into_inner);
      while !*stopped {
        let now = Instant::now();
        if now >= due {
          return true;
        }
        stopped = wake
          .wait_timeout(stopped, due - now)
          .unwrap_or_else(PoisonError::into_inner)
          .0;
      }
      false
    }
  }
}

impl Host for StoreState {
  fn set_timer(&mut self, delay_ms: u32, command: String) -> Result<u64, ApiError> {
    if !(MIN_DELAY_MS..=MAX_DELAY_MS).contains(&delay_ms) {
      return Err(ApiError::InvalidArgument(
        "a timer waits 1 second to 1 day".into(),
      ));
    }
    if !self.api.manifest().declares_command(&command) {
      return Err(ApiError::InvalidArgument(format!(
        "{command:?} is not one of this extension's commands"
      )));
    }
    let shared = self.api.shared()?;
    shared
      .set_timer(
        self.api.extension_id(),
        self.api.generation(),
        Duration::from_millis(u64::from(delay_ms)),
        command,
      )
      .ok_or_else(|| ApiError::TooLarge(format!("at most {MAX_TIMERS} timers can wait at once")))
  }

  fn clear_timer(&mut self, id: u64) -> bool {
    self
      .api
      .shared()
      .is_ok_and(|shared| shared.clear_timer(self.api.extension_id(), id))
  }

  fn fired(&mut self) -> Option<u64> {
    match &self.api.payload {
      Payload::Timer(id) => Some(*id),
      _ => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn timers_wait_unless_cancelled() {
    let timer = Timer::new(1, "ferrite.git".into());
    let started = Instant::now();
    assert!(timer.waiter(Duration::from_millis(50))());
    assert!(started.elapsed() >= Duration::from_millis(50));

    let timer = Timer::new(2, "ferrite.git".into());
    let wait = timer.waiter(Duration::from_secs(60));
    let waiting = std::thread::spawn(wait);
    timer.cancel();
    assert!(!waiting.join().expect("waits"), "cancelled at once");
  }
}
