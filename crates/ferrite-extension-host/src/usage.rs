//! Resource accounting: how much time, memory, and queue space each
//! extension uses, so users can see what an extension costs and which one
//! slows things down.
//!
//! Counters are atomics updated by the extension's worker (and its queue),
//! read without locks. They accumulate over the editor's session, across
//! restarts of the extension; memory is the current instance's.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Default)]
pub struct Usage {
  busy_nanos: AtomicU64,
  calls: AtomicU64,
  slowest_nanos: AtomicU64,
  memory_bytes: AtomicU64,
  peak_memory_bytes: AtomicU64,
  dropped_events: AtomicU64,
  compile_nanos: AtomicU64,
  activation_nanos: AtomicU64,
}

/// A snapshot of an extension's resource use.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUsage {
  /// Time spent running the extension's code, including the host calls it
  /// made.
  pub busy_ms: f64,
  /// Calls into the extension (activation, events, commands).
  pub calls: u64,
  pub slowest_call_ms: f64,
  /// Linear memory of the running instance; 0 when it isn't running.
  pub memory_bytes: u64,
  pub peak_memory_bytes: u64,
  /// Document events dropped because its queue was full.
  pub dropped_events: u64,
  /// Time spent compiling it, most recently.
  pub compile_ms: f64,
  /// How long it took to start, most recently: compiling, instantiating,
  /// and `activate`. `None` until it has started once.
  pub activation_ms: Option<f64>,
}

fn nanos(duration: Duration) -> u64 {
  u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn millis(nanos: u64) -> f64 {
  nanos as f64 / 1_000_000.0
}

impl Usage {
  pub fn record_call(&self, elapsed: Duration) {
    let elapsed = nanos(elapsed);
    self.busy_nanos.fetch_add(elapsed, Ordering::Relaxed);
    self.calls.fetch_add(1, Ordering::Relaxed);
    self.slowest_nanos.fetch_max(elapsed, Ordering::Relaxed);
  }

  pub fn record_compile(&self, elapsed: Duration) {
    self.compile_nanos.store(nanos(elapsed), Ordering::Relaxed);
  }

  pub fn record_activation(&self, elapsed: Duration) {
    // Never 0, which means "not yet".
    self
      .activation_nanos
      .store(nanos(elapsed).max(1), Ordering::Relaxed);
  }

  pub fn set_memory(&self, bytes: usize) {
    let bytes = bytes as u64;
    self.memory_bytes.store(bytes, Ordering::Relaxed);
    self.peak_memory_bytes.fetch_max(bytes, Ordering::Relaxed);
  }

  pub fn record_dropped_event(&self) {
    self.dropped_events.fetch_add(1, Ordering::Relaxed);
  }

  pub fn snapshot(&self) -> ResourceUsage {
    ResourceUsage {
      busy_ms: millis(self.busy_nanos.load(Ordering::Relaxed)),
      calls: self.calls.load(Ordering::Relaxed),
      slowest_call_ms: millis(self.slowest_nanos.load(Ordering::Relaxed)),
      memory_bytes: self.memory_bytes.load(Ordering::Relaxed),
      peak_memory_bytes: self.peak_memory_bytes.load(Ordering::Relaxed),
      dropped_events: self.dropped_events.load(Ordering::Relaxed),
      compile_ms: millis(self.compile_nanos.load(Ordering::Relaxed)),
      activation_ms: match self.activation_nanos.load(Ordering::Relaxed) {
        0 => None,
        nanos => Some(millis(nanos)),
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn accumulates_calls_and_keeps_peaks() {
    let usage = Usage::default();
    usage.record_call(Duration::from_millis(3));
    usage.record_call(Duration::from_millis(1));
    usage.set_memory(2 << 20);
    usage.set_memory(1 << 20);
    usage.record_dropped_event();
    assert_eq!(usage.snapshot().activation_ms, None);
    usage.record_activation(Duration::from_millis(12));
    let snapshot = usage.snapshot();
    assert_eq!(snapshot.activation_ms, Some(12.0));
    assert_eq!(snapshot.calls, 2);
    assert!((snapshot.busy_ms - 4.0).abs() < 1e-9);
    assert!((snapshot.slowest_call_ms - 3.0).abs() < 1e-9);
    assert_eq!(snapshot.memory_bytes, 1 << 20);
    assert_eq!(snapshot.peak_memory_bytes, 2 << 20);
    assert_eq!(snapshot.dropped_events, 1);
  }
}
