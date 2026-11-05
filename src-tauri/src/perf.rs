//! Background scheduling and performance instrumentation shared by the app:
//! one bounded task pool for all background work, timing histograms for
//! backend operations, and the `perf_stats` command behind the frontend's
//! performance overlay.

use std::time::Duration;

use flareite_tasks::{Metrics, Scheduler, Summary};
use serde::Serialize;
use tauri::State;

use crate::editor::EditorState;

#[derive(Default)]
pub struct Perf {
  pub scheduler: Scheduler,
  pub metrics: Metrics,
}

impl Perf {
  pub fn record(&self, name: &'static str, duration: Duration) {
    self.metrics.record(name, duration);
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingDto {
  pub name: &'static str,
  pub count: u64,
  pub mean_us: f64,
  pub p50_us: f64,
  pub p95_us: f64,
  pub p99_us: f64,
  pub max_us: u64,
}

impl TimingDto {
  fn of(name: &'static str, summary: Summary) -> Self {
    Self {
      name,
      count: summary.count,
      mean_us: summary.mean_us,
      p50_us: summary.p50_us,
      p95_us: summary.p95_us,
      p99_us: summary.p99_us,
      max_us: summary.max_us,
    }
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
  pub name: &'static str,
  pub completed: u64,
  pub cancelled: u64,
  pub dropped: u64,
  pub panicked: u64,
  pub wait_p95_us: f64,
  pub run_p50_us: f64,
  pub run_p95_us: f64,
  pub run_max_us: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfStats {
  pub timings: Vec<TimingDto>,
  pub tasks: Vec<TaskDto>,
  pub queued_tasks: usize,
  /// Resident memory of the backend process, if the platform reports it.
  /// WebView processes are separate and not included.
  pub backend_memory_bytes: Option<usize>,
  pub open_documents: usize,
  pub open_document_bytes: usize,
}

#[tauri::command]
pub fn perf_stats(perf: State<'_, Perf>, editor: State<'_, EditorState>) -> PerfStats {
  let (open_documents, open_document_bytes) = {
    let editor = editor.lock();
    editor.documents().fold((0, 0), |(count, bytes), document| {
      (count + 1, bytes + document.buffer().len())
    })
  };
  PerfStats {
    timings: perf
      .metrics
      .snapshot()
      .into_iter()
      .map(|(name, summary)| TimingDto::of(name, summary))
      .collect(),
    tasks: perf
      .scheduler
      .stats()
      .into_iter()
      .map(|(name, stats)| TaskDto {
        name,
        completed: stats.completed,
        cancelled: stats.cancelled,
        dropped: stats.dropped,
        panicked: stats.panicked,
        wait_p95_us: stats.wait.p95_us,
        run_p50_us: stats.run.p50_us,
        run_p95_us: stats.run.p95_us,
        run_max_us: stats.run.max_us,
      })
      .collect(),
    queued_tasks: perf.scheduler.queued(),
    backend_memory_bytes: memory_stats::memory_stats().map(|usage| usage.physical_mem),
    open_documents,
    open_document_bytes,
  }
}
