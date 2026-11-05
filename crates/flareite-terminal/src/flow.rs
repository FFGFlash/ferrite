//! Flow control between the shell and the frontend.
//!
//! A program can print far faster than a terminal emulator can draw. Output
//! sent to the frontend is counted until the frontend acknowledges it; when
//! too much is outstanding, the reader stops reading, the pseudoterminal's
//! buffer fills, and the program blocks on its writes until the frontend
//! catches up. Units are UTF-16 code units, which the frontend counts as
//! string length.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// Reading pauses when this much output is unacknowledged...
pub const HIGH_WATER: usize = 500_000;
/// ...and resumes when it drops below this. The frontend must acknowledge in
/// steps no larger than this, or reading could stay paused.
pub const LOW_WATER: usize = 100_000;

#[derive(Debug, Default)]
struct State {
  outstanding: usize,
  paused: bool,
  closed: bool,
}

#[derive(Debug, Default)]
pub struct Flow {
  state: Mutex<State>,
  changed: Condvar,
}

impl Flow {
  fn lock(&self) -> MutexGuard<'_, State> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Blocks while output is paused. Returns false once closed.
  pub fn wait_for_room(&self) -> bool {
    let mut state = self.lock();
    while state.paused && !state.closed {
      state = self
        .changed
        .wait(state)
        .unwrap_or_else(PoisonError::into_inner);
    }
    !state.closed
  }

  /// Counts output sent to the frontend.
  pub fn sent(&self, units: usize) {
    let mut state = self.lock();
    state.outstanding += units;
    if state.outstanding >= HIGH_WATER {
      state.paused = true;
    }
  }

  /// The frontend has drawn `units` of output.
  pub fn acknowledge(&self, units: usize) {
    let mut state = self.lock();
    state.outstanding = state.outstanding.saturating_sub(units);
    if state.paused && state.outstanding < LOW_WATER {
      state.paused = false;
      self.changed.notify_all();
    }
  }

  /// Forgets outstanding output, e.g. when a new frontend attaches.
  pub fn reset(&self) {
    let mut state = self.lock();
    state.outstanding = 0;
    state.paused = false;
    self.changed.notify_all();
  }

  /// Releases a waiting reader for good.
  pub fn close(&self) {
    self.lock().closed = true;
    self.changed.notify_all();
  }
}

/// The length of `text` in flow control units.
pub fn units(text: &str) -> usize {
  text.encode_utf16().count()
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::thread;
  use std::time::Duration;

  use super::*;

  #[test]
  fn pauses_at_the_high_water_mark_until_below_the_low_one() {
    let flow = Arc::new(Flow::default());
    assert!(flow.wait_for_room());
    flow.sent(HIGH_WATER);

    let waiter = {
      let flow = flow.clone();
      thread::spawn(move || flow.wait_for_room())
    };
    thread::sleep(Duration::from_millis(50));
    assert!(!waiter.is_finished());
    // Still above the low water mark: stays paused.
    flow.acknowledge(HIGH_WATER - LOW_WATER);
    thread::sleep(Duration::from_millis(50));
    assert!(!waiter.is_finished());
    flow.acknowledge(1);
    assert!(waiter.join().expect("waiter"));
  }

  #[test]
  fn reset_and_close_release_a_paused_reader() {
    let flow = Flow::default();
    flow.sent(HIGH_WATER * 2);
    flow.reset();
    assert!(flow.wait_for_room());

    flow.sent(HIGH_WATER);
    flow.close();
    assert!(!flow.wait_for_room());
  }

  #[test]
  fn counts_utf16_units() {
    assert_eq!(units("abc"), 3);
    assert_eq!(units("€"), 1);
    assert_eq!(units("😀"), 2);
  }
}
