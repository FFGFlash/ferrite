//! The WebAssembly runtime: one Wasmtime engine for all extensions, a
//! ticker that drives deadlines, compilation, and per-store limits.
//!
//! Deadlines use epoch interruption: a background thread advances the
//! engine's epoch every [`TICK`], and each store checks, once per tick
//! while WebAssembly runs, whether its current call is past its deadline or
//! was cancelled. That bounds wall-clock time per call (including infinite
//! loops) at a small cost, without the per-instruction overhead of fuel.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, UpdateDeadline};

use crate::api::ApiContext;
use crate::bindings::Extension;
use crate::usage::Usage;

/// How often deadlines are checked.
pub const TICK: Duration = Duration::from_millis(10);
/// WebAssembly stack available to an extension.
const MAX_WASM_STACK: usize = 1024 * 1024;
const MAX_TABLE_ELEMENTS: usize = 100_000;

/// Why a call was stopped from outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
  Deadline,
  Cancelled,
}

/// The call in progress, if any.
pub struct Call {
  pub deadline: Instant,
}

/// Data of an extension's store.
pub struct StoreState {
  pub api: ApiContext,
  pub limiter: Limiter,
  pub call: Option<Call>,
  /// Set from other threads to stop the extension at its next tick.
  pub cancel: Arc<AtomicBool>,
  /// Why the last call was interrupted.
  pub interrupt: Option<Interrupt>,
}

/// Memory shared by all extensions: each instance reserves what its linear
/// memories grow to, so together they stay within the total.
#[derive(Debug)]
pub struct MemoryPool {
  used: AtomicUsize,
  limit: usize,
}

impl MemoryPool {
  pub fn new(limit: usize) -> Self {
    Self {
      used: AtomicUsize::new(0),
      limit,
    }
  }

  pub fn limit(&self) -> usize {
    self.limit
  }

  fn reserve(&self, bytes: usize) -> bool {
    let mut used = self.used.load(Ordering::Acquire);
    loop {
      let Some(total) = used.checked_add(bytes).filter(|total| *total <= self.limit) else {
        return false;
      };
      match self
        .used
        .compare_exchange_weak(used, total, Ordering::AcqRel, Ordering::Acquire)
      {
        Ok(_) => return true,
        Err(actual) => used = actual,
      }
    }
  }

  fn release(&self, bytes: usize) {
    self.used.fetch_sub(bytes, Ordering::AcqRel);
  }

  #[cfg(test)]
  fn used(&self) -> usize {
    self.used.load(Ordering::Acquire)
  }
}

/// Which memory limit stopped a growth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryExceeded {
  /// The extension's own limit.
  Instance,
  /// The limit for all extensions together.
  Total,
}

/// Caps linear memory (per instance and in the shared pool), tables, and
/// instance counts, remembers which memory cap was hit so the failure can be
/// reported as such, and accounts for memory in the extension's usage.
pub struct Limiter {
  pub max_memory: usize,
  pub memory_exceeded: Option<MemoryExceeded>,
  pub usage: Arc<Usage>,
  pub pool: Arc<MemoryPool>,
  /// Bytes reserved in the pool by this instance's memories.
  reserved: usize,
}

impl Limiter {
  pub fn new(max_memory: usize, usage: Arc<Usage>, pool: Arc<MemoryPool>) -> Self {
    Self {
      max_memory,
      memory_exceeded: None,
      usage,
      pool,
      reserved: 0,
    }
  }
}

impl Drop for Limiter {
  fn drop(&mut self) {
    self.pool.release(self.reserved);
  }
}

impl ResourceLimiter for Limiter {
  fn memory_growing(
    &mut self,
    current: usize,
    desired: usize,
    _maximum: Option<usize>,
  ) -> wasmtime::Result<bool> {
    let growth = desired.saturating_sub(current);
    if self.reserved + growth > self.max_memory {
      self.memory_exceeded = Some(MemoryExceeded::Instance);
      return Ok(false);
    }
    if !self.pool.reserve(growth) {
      self.memory_exceeded = Some(MemoryExceeded::Total);
      return Ok(false);
    }
    self.reserved += growth;
    self.usage.set_memory(self.reserved);
    Ok(true)
  }

  fn table_growing(
    &mut self,
    _current: usize,
    desired: usize,
    _maximum: Option<usize>,
  ) -> wasmtime::Result<bool> {
    Ok(desired <= MAX_TABLE_ELEMENTS)
  }

  fn instances(&self) -> usize {
    16
  }

  fn tables(&self) -> usize {
    16
  }

  fn memories(&self) -> usize {
    4
  }
}

pub struct Runtime {
  engine: Engine,
  linker: Linker<StoreState>,
  stop: Arc<AtomicBool>,
  ticker: Option<JoinHandle<()>>,
}

impl Runtime {
  pub fn new() -> Result<Self, String> {
    let mut config = Config::new();
    // Tree-sitter's grammar loader builds wasmtime with GC support; the
    // extension API has no use for it.
    config
      .wasm_gc(false)
      .wasm_component_model(true)
      .epoch_interruption(true)
      .max_wasm_stack(MAX_WASM_STACK);
    let engine =
      Engine::new(&config).map_err(|error| format!("could not start WebAssembly: {error}"))?;
    let mut linker = Linker::new(&engine);
    Extension::add_to_linker::<StoreState, HasSelf<StoreState>>(&mut linker, |state| state)
      .map_err(|error| format!("could not set up the extension API: {error}"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let ticker = {
      let (engine, stop) = (engine.clone(), stop.clone());
      std::thread::Builder::new()
        .name("extension-ticker".into())
        .spawn(move || {
          while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(TICK);
            engine.increment_epoch();
          }
        })
        .map_err(|error| format!("could not start the extension timer: {error}"))?
    };
    Ok(Self {
      engine,
      linker,
      stop,
      ticker: Some(ticker),
    })
  }

  pub fn engine(&self) -> &Engine {
    &self.engine
  }

  pub fn linker(&self) -> &Linker<StoreState> {
    &self.linker
  }

  /// Reads and compiles an extension. A core module (as built for
  /// `wasm32-unknown-unknown`) is first turned into a component using the
  /// interface description wit-bindgen embeds in it.
  pub fn compile(&self, path: &Path) -> Result<Component, String> {
    let started = Instant::now();
    let bytes =
      std::fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let component_bytes = if is_component(&bytes) {
      bytes
    } else {
      wit_component::ComponentEncoder::default()
        .module(&bytes)
        .and_then(|encoder| encoder.validate(true).encode())
        .map_err(|error| format!("{} is not a Flareite extension: {error:#}", path.display()))?
    };
    let component = Component::new(&self.engine, &component_bytes)
      .map_err(|error| format!("{} could not be compiled: {error:#}", path.display()))?;
    tracing::info!(
      path = %path.display(),
      bytes = component_bytes.len(),
      elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
      "compiled extension"
    );
    Ok(component)
  }

  /// Installs the deadline check on a new store.
  pub fn watch(store: &mut wasmtime::Store<StoreState>) {
    store.epoch_deadline_callback(|mut state| {
      let data = state.data_mut();
      let interrupt = if data.cancel.load(Ordering::Relaxed) {
        Some(Interrupt::Cancelled)
      } else if data
        .call
        .as_ref()
        .is_some_and(|call| Instant::now() >= call.deadline)
      {
        Some(Interrupt::Deadline)
      } else {
        None
      };
      match interrupt {
        Some(interrupt) => {
          data.interrupt = Some(interrupt);
          Ok(UpdateDeadline::Interrupt)
        }
        None => Ok(UpdateDeadline::Continue(1)),
      }
    });
  }

  pub fn shutdown(&mut self) {
    self.stop.store(true, Ordering::Relaxed);
    if let Some(ticker) = self.ticker.take() {
      let _ = ticker.join();
    }
  }
}

impl Drop for Runtime {
  fn drop(&mut self) {
    self.shutdown();
  }
}

/// Whether `bytes` is a component rather than a core module: both start
/// with `\0asm`, followed by a version and a layer (1 for components).
fn is_component(bytes: &[u8]) -> bool {
  bytes.len() >= 8 && bytes[..4] == *b"\0asm" && bytes[6..8] == [1, 0]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn instances_share_the_memory_pool() {
    let pool = Arc::new(MemoryPool::new(100));
    let mut a = Limiter::new(80, Arc::default(), pool.clone());
    let mut b = Limiter::new(80, Arc::default(), pool.clone());
    assert!(a.memory_growing(0, 60, None).expect("grows"));
    assert!(!a.memory_growing(60, 90, None).expect("checked"));
    assert_eq!(a.memory_exceeded, Some(MemoryExceeded::Instance));
    assert!(!b.memory_growing(0, 50, None).expect("checked"));
    assert_eq!(b.memory_exceeded, Some(MemoryExceeded::Total));
    assert!(b.memory_growing(0, 40, None).expect("grows"));
    assert_eq!(pool.used(), 100);
    drop(a);
    assert_eq!(pool.used(), 40, "a stopped instance returns its memory");
    drop(b);
    assert_eq!(pool.used(), 0);
  }

  #[test]
  fn tells_components_from_modules() {
    assert!(is_component(b"\0asm\x0d\x00\x01\x00"));
    assert!(!is_component(b"\0asm\x01\x00\x00\x00"));
    assert!(!is_component(b"\0as"));
  }
}
