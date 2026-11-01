//! Ferrite desktop application.
//!
//! This crate owns process lifecycle and dependency wiring: it initializes
//! logging, registers the Tauri commands that form the frontend boundary, and
//! runs the window event loop. Editor behavior belongs in dedicated crates
//! (see `docs/architecture.md`), not here.

mod backups;
mod bench;
mod commands;
mod dialogs;
mod editor;
mod error;
mod events;
mod extensions;
mod languages;
mod logging;
mod lsp;
mod paths;
mod perf;
mod settings;
mod terminal;
mod ui_state;
mod workspace;

use std::time::Instant;

use tauri::{Manager, RunEvent, WindowEvent};

/// Builds and runs the application. Returns only if startup fails; the process
/// exits when the event loop ends.
pub fn run() {
  let launched_at = Instant::now();

  let app = tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_clipboard_manager::init())
    .manage(editor::EditorState::default())
    .manage(events::EventHub::default())
    .manage(workspace::WorkspaceState::default())
    .manage(perf::Perf::default())
    .manage(lsp::LspState::default())
    .manage(extensions::ExtensionsState::default())
    .manage(languages::LanguagesState::default())
    .manage(terminal::TerminalState::default())
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
      let settings = settings::SettingsState::load(app.path().app_config_dir().ok());
      app
        .state::<editor::EditorState>()
        .lock()
        .set_config(settings.get().editor_config());
      app.manage(settings);
      let saved = ui_state::UiStateStore::load(app.path().app_local_data_dir().ok());
      let previous_files = saved.get().open_files;
      app.manage(saved);

      let args: Vec<String> = std::env::args().skip(1).collect();
      let bench = bench::BenchState::from_args(&args, launched_at);
      let benchmarking = bench.active();
      let load_extensions =
        !benchmarking || bench.with_extensions() || !bench.languages().is_empty();
      app.manage(bench);
      if benchmarking {
        app.state::<ui_state::UiStateStore>().make_ephemeral();
      }
      // Benchmarks measure the editor alone unless asked otherwise.
      extensions::init(app.handle(), load_extensions);
      // Benchmark runs keep no backups and restore none.
      let backup_root = app
        .path()
        .app_local_data_dir()
        .ok()
        .filter(|_| !benchmarking)
        .map(|dir| dir.join("backups"));
      let crashed = backups::init(app.handle(), backup_root);

      // Benchmarks start from a clean session.
      let (previous_files, named) = if benchmarking {
        (Vec::new(), Vec::new())
      } else {
        workspace::restore(app.handle());
        let skip = bench::BenchState::consumed(&args);
        let named: Vec<std::path::PathBuf> = args
          .iter()
          .skip(skip)
          .filter(|arg| !arg.starts_with('-'))
          .map(std::path::PathBuf::from)
          .collect();
        (previous_files, named)
      };
      // A file named on the command line becomes the active tab.
      if let Some(last) = named.last() {
        let active = paths::normalize(last);
        app
          .state::<ui_state::UiStateStore>()
          .update(|state| state.active_file = Some(active));
      }
      editor::open_startup_files(
        app.handle(),
        previous_files.into_iter().chain(named).collect(),
        crashed,
      );
      Ok(())
    })
    .on_window_event(|window, event| {
      if let WindowEvent::CloseRequested { api, .. } = event
        && let Some(window) = window.app_handle().get_webview_window(window.label())
      {
        editor::on_close_requested(&window, api);
      }
    })
    .invoke_handler(tauri::generate_handler![
      commands::app_info,
      commands::frontend_log,
      commands::frontend_ready,
      commands::open_external,
      editor::document_list,
      editor::document_new,
      editor::document_open,
      editor::document_open_dialog,
      editor::document_save,
      editor::document_save_as,
      editor::document_close,
      editor::editor_view,
      editor::editor_minimap,
      editor::editor_apply,
      editor::editor_clipboard,
      events::events_subscribe,
      settings::settings_get,
      settings::settings_file,
      settings::settings_set_theme,
      settings::settings_set_icon_theme,
      settings::settings_set_default_profile,
      settings::settings_schema,
      settings::settings_set,
      settings::settings_problem,
      logging::logs_read,
      logging::logs_file,
      ui_state::ui_state_get,
      ui_state::ui_state_save,
      workspace::workspace_get,
      workspace::workspace_open,
      workspace::workspace_open_dialog,
      workspace::workspace_close,
      workspace::workspace_list_dir,
      workspace::workspace_create,
      workspace::workspace_find_files,
      workspace::workspace_search,
      workspace::workspace_search_cancel,
      lsp::lsp_hover,
      lsp::lsp_definition,
      lsp::lsp_completion,
      terminal::terminal_create,
      terminal::terminal_platform,
      terminal::terminal_profiles,
      terminal::terminal_list,
      terminal::terminal_attach,
      terminal::terminal_write,
      terminal::terminal_resize,
      terminal::terminal_acknowledge,
      terminal::terminal_kill,
      extensions::extensions_list,
      extensions::extensions_reload,
      extensions::extension_run_command,
      extensions::extension_status_items,
      extensions::extension_view,
      extensions::extensions_icon_theme,
      extensions::extension_view_opened,
      extensions::extension_previews,
      extensions::extension_preview_content,
      extensions::extension_preview_closed,
      extensions::extension_preview_image,
      extensions::extension_set_permissions,
      extensions::extension_set_enabled,
      extensions::extension_restart,
      extensions::extension_add_development,
      perf::perf_stats,
      bench::bench_config,
      bench::bench_generate,
      bench::bench_report,
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

  app.run(|handle, event| match event {
    RunEvent::ExitRequested { api, .. } => editor::on_exit_requested(handle, &api),
    RunEvent::Exit => {
      tracing::info!("exiting");
      // Removes the backups of unsaved changes, or keeps them if the exit
      // skipped asking about them.
      backups::shutdown(handle);
      // Stop terminals, extensions, language servers, and background work
      // (searches, indexing) before the process ends.
      if let Some(terminals) = handle.try_state::<terminal::TerminalState>() {
        terminals.shutdown();
      }
      extensions::shutdown(handle);
      if let Some(lsp) = handle.try_state::<lsp::LspState>() {
        lsp.shutdown_all();
      }
      if let Some(perf) = handle.try_state::<perf::Perf>() {
        perf.scheduler.shutdown();
      }
      if let Some(guard) = handle.try_state::<logging::LogGuard>() {
        guard.flush();
      }
    }
    _ => {}
  });
}
