//! Ferrite desktop application.
//!
//! This crate owns process lifecycle and dependency wiring: it initializes
//! logging, registers the Tauri commands that form the frontend boundary, and
//! runs the window event loop. Editor behavior belongs in dedicated crates
//! (see `docs/architecture.md`), not here.

mod commands;
mod logging;

use std::time::Instant;

use tauri::{Manager, RunEvent};

/// Builds and runs the application. Returns only if startup fails; the process
/// exits when the event loop ends.
pub fn run() {
  let launched_at = Instant::now();

  let app = tauri::Builder::default()
    .setup(move |app| {
      let log_dir = match app.path().app_log_dir() {
        Ok(dir) => Some(dir),
        Err(error) => {
          eprintln!("Ferrite: log directory unavailable, logging to stderr only: {error}");
          None
        }
      };
      app.manage(logging::init(log_dir.as_deref()));
      logging::install_panic_hook();
      app.manage(commands::StartupState::new(launched_at));

      tracing::info!(
        version = %app.package_info().version,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        log_dir = ?log_dir,
        elapsed_ms = commands::millis(launched_at.elapsed()),
        "backend initialized"
      );
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::app_info,
      commands::frontend_log,
      commands::frontend_ready,
    ])
    .build(tauri::generate_context!());

  let app = match app {
    Ok(app) => app,
    Err(error) => {
      tracing::error!(%error, "failed to start");
      eprintln!("Ferrite failed to start: {error}");
      std::process::exit(1);
    }
  };

  app.run(|handle, event| {
    if let RunEvent::Exit = event {
      tracing::info!("exiting");
      if let Some(guard) = handle.try_state::<logging::LogGuard>() {
        guard.flush();
      }
    }
  });
}
