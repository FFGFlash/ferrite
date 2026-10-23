//! End-to-end benchmark support, active only when Ferrite is started with
//! `--bench <name> [file]` (see `scripts/bench.mjs`).
//!
//! The frontend reads the requested benchmark at startup, runs it against the
//! real editor (IPC, rendering, and all), and reports its measurements. The
//! report is written as one JSON object to the file named by the
//! `FERRITE_BENCH_OUT` environment variable (or stdout), and the app exits
//! shortly after, leaving the harness time to measure process memory.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::StartupState;
use crate::editor::EditorState;
use crate::editor::protocol::DocumentSummary;
use crate::error::CommandError;

/// How long the app stays up after reporting, so memory can be sampled.
const EXIT_DELAY: Duration = Duration::from_millis(1500);

pub struct BenchState {
  name: Option<String>,
  file: Option<PathBuf>,
  launched_at: Instant,
}

impl BenchState {
  /// Reads `--bench <name> [file]` from the command line.
  pub fn from_args(args: &[String], launched_at: Instant) -> Self {
    let position = args.iter().position(|arg| arg == "--bench");
    let name = position.and_then(|index| args.get(index + 1)).cloned();
    let file = position
      .and_then(|index| args.get(index + 2))
      .filter(|arg| !arg.starts_with('-'))
      .map(PathBuf::from);
    Self {
      name,
      file,
      launched_at,
    }
  }

  pub fn active(&self) -> bool {
    self.name.is_some()
  }

  /// Arguments consumed by `--bench`, so they are not opened as files.
  pub fn consumed(args: &[String]) -> usize {
    args
      .iter()
      .position(|arg| arg == "--bench")
      .map_or(0, |_| 3)
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchConfig {
  pub name: String,
  pub file: Option<String>,
}

#[tauri::command]
pub fn bench_config(state: State<'_, BenchState>) -> Option<BenchConfig> {
  Some(BenchConfig {
    name: state.name.clone()?,
    file: state.file.as_ref().map(|path| path.display().to_string()),
  })
}

/// Creates an untitled document with `lines` generated lines of about
/// `width` characters, for benchmarks that need a large document.
#[tauri::command]
pub fn bench_generate(
  bench: State<'_, BenchState>,
  editor: State<'_, EditorState>,
  lines: usize,
  width: usize,
) -> Result<DocumentSummary, CommandError> {
  if !bench.active() {
    return Err(CommandError::new(
      "notBenchmarking",
      "Only available in benchmark mode.",
    ));
  }
  let width = width.clamp(8, 1000);
  let mut text = String::with_capacity(lines * (width + 1));
  for line in 0..lines.min(10_000_000) {
    let prefix = format!("{line:>8} ");
    text.push_str(&prefix);
    for i in 0..width.saturating_sub(prefix.len()) {
      text.push(if i % 6 == 5 {
        ' '
      } else {
        (b'a' + (line + i) as u8 % 26) as char
      });
    }
    text.push('\n');
  }
  let mut editor = editor.lock();
  let id = editor.create_untitled();
  let document = editor.document_mut(id)?;
  document.apply(ferrite_core::EditorOp::Insert(text))?;
  document.apply(ferrite_core::EditorOp::Move {
    motion: ferrite_core::Motion::DocumentStart,
    extend: false,
  })?;
  Ok(DocumentSummary::of(document))
}

/// Records a benchmark result (plus backend timing and memory) and exits.
#[tauri::command]
pub fn bench_report(
  app: AppHandle,
  bench: State<'_, BenchState>,
  startup: State<'_, StartupState>,
  result: serde_json::Value,
) {
  let report = serde_json::json!({
    "benchmark": bench.name,
    "result": result,
    "backend": {
      "uptimeMs": bench.launched_at.elapsed().as_secs_f64() * 1000.0,
      "launchToFrontendReadyMs": startup.ready_ms(),
      "residentBytes": memory_stats::memory_stats().map(|usage| usage.physical_mem),
      "pid": std::process::id(),
    },
  });
  let line = report.to_string();
  match std::env::var_os("FERRITE_BENCH_OUT") {
    Some(path) => {
      if let Err(error) = std::fs::write(&path, &line) {
        tracing::error!(%error, "could not write benchmark report");
      }
    }
    None => println!("{line}"),
  }
  tracing::info!(report = %line, "benchmark finished");
  std::thread::spawn(move || {
    std::thread::sleep(EXIT_DELAY);
    app.exit(0);
  });
}
