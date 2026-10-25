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
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, UpdateDeadline};

use crate::api::ApiContext;
use crate::bindings::Extension;

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

/// Caps linear memory, tables, and instance counts, and remembers when the
/// memory cap was hit so the failure can be reported as such.
pub struct Limiter {
  pub max_memory: usize,
  pub memory_exceeded: bool,
}

impl ResourceLimiter for Limiter {
  fn memory_growing(
    &mut self,
    _current: usize,
    desired: usize,
    _maximum: Option<usize>,
  ) -> wasmtime::Result<bool> {
    if desired > self.max_memory {
      self.memory_exceeded = true;
      return Ok(false);
    }
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
    config
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
        .map_err(|error| format!("{} is not a Ferrite extension: {error:#}", path.display()))?
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
  fn tells_components_from_modules() {
    assert!(is_component(b"\0asm\x0d\x00\x01\x00"));
    assert!(!is_component(b"\0asm\x01\x00\x00\x00"));
    assert!(!is_component(b"\0as"));
  }
}
