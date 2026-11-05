use std::collections::{BTreeMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::metrics::{Histogram, Summary};

/// Order in which queued tasks start. Tasks of the same priority start in
/// submission order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
  /// Someone is waiting for the result (find in files, match counts).
  High = 0,
  /// Background maintenance (indexing).
  Normal = 1,
  /// Speculative work that may be dropped under load.
  Low = 2,
}

const PRIORITIES: usize = 3;

/// Cooperative cancellation. Long-running tasks check
/// [`CancelToken::is_cancelled`] and return early.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn cancel(&self) {
    self.0.store(true, Ordering::Relaxed);
  }

  pub fn is_cancelled(&self) -> bool {
    self.0.load(Ordering::Relaxed)
  }

  /// The underlying flag, for APIs that take an `&AtomicBool`.
  pub fn as_atomic(&self) -> &AtomicBool {
    &self.0
  }

  fn same(&self, other: &Self) -> bool {
    Arc::ptr_eq(&self.0, &other.0)
  }
}

/// Handle to a submitted task; cancelling it stops it if it has not started
/// and asks it to stop if it is running.
#[derive(Debug, Clone)]
pub struct TaskHandle {
  token: CancelToken,
}

impl TaskHandle {
  pub fn cancel(&self) {
    self.token.cancel();
  }

  pub fn is_cancelled(&self) -> bool {
    self.token.is_cancelled()
  }
}

#[derive(Debug, Clone, Copy)]
pub struct SchedulerConfig {
  pub workers: usize,
  /// Tasks waiting beyond this are dropped, lowest priority and oldest first.
  pub max_queued: usize,
}

impl Default for SchedulerConfig {
  /// One worker per core minus one (for the UI), between one and four.
  fn default() -> Self {
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    Self {
      workers: cores.saturating_sub(1).clamp(1, 4),
      max_queued: 256,
    }
  }
}

/// Per-task-name statistics.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TaskStats {
  pub completed: u64,
  pub cancelled: u64,
  pub dropped: u64,
  pub panicked: u64,
  /// Time spent queued before starting.
  pub wait: Summary,
  /// Time spent running.
  pub run: Summary,
}

type Work = Box<dyn FnOnce(&CancelToken) + Send + 'static>;

struct Job {
  name: &'static str,
  key: Option<String>,
  token: CancelToken,
  queued_at: Instant,
  work: Work,
}

#[derive(Default)]
struct NameStats {
  completed: u64,
  cancelled: u64,
  dropped: u64,
  panicked: u64,
  wait: Histogram,
  run: Histogram,
}

#[derive(Default)]
struct State {
  queues: [VecDeque<Job>; PRIORITIES],
  /// Every running task's key and token, for keyed replacement and
  /// shutdown.
  running: Vec<(Option<String>, CancelToken)>,
  stats: BTreeMap<&'static str, NameStats>,
  shutdown: bool,
}

impl State {
  fn queued(&self) -> usize {
    self.queues.iter().map(VecDeque::len).sum()
  }
}

struct Shared {
  state: Mutex<State>,
  ready: Condvar,
  max_queued: usize,
}

impl Shared {
  fn lock(&self) -> MutexGuard<'_, State> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }
}

/// A fixed pool of worker threads running submitted tasks.
pub struct Scheduler {
  shared: Arc<Shared>,
  workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Default for Scheduler {
  fn default() -> Self {
    Self::new(SchedulerConfig::default())
  }
}

impl Scheduler {
  pub fn new(config: SchedulerConfig) -> Self {
    let shared = Arc::new(Shared {
      state: Mutex::new(State::default()),
      ready: Condvar::new(),
      max_queued: config.max_queued.max(1),
    });
    let workers = (0..config.workers.max(1))
      .filter_map(|index| {
        let shared = shared.clone();
        std::thread::Builder::new()
          .name(format!("flareite-worker-{index}"))
          .spawn(move || worker(&shared))
          .inspect_err(|error| tracing::error!(%error, "could not start worker thread"))
          .ok()
      })
      .collect();
    Self {
      shared,
      workers: Mutex::new(workers),
    }
  }

  /// Queues `work`. With a `key`, any queued or running task with the same
  /// key is cancelled first, so only the latest request for a resource runs.
  pub fn spawn(
    &self,
    name: &'static str,
    priority: Priority,
    key: Option<String>,
    work: impl FnOnce(&CancelToken) + Send + 'static,
  ) -> TaskHandle {
    let token = CancelToken::new();
    let handle = TaskHandle {
      token: token.clone(),
    };
    let mut state = self.shared.lock();
    if state.shutdown {
      token.cancel();
      return handle;
    }

    if let Some(key) = &key {
      for queue in &mut state.queues {
        queue.retain(|job| {
          let replaced = job.key.as_ref() == Some(key);
          if replaced {
            job.token.cancel();
          }
          !replaced
        });
      }
      for (running_key, running_token) in &state.running {
        if running_key.as_ref() == Some(key) {
          running_token.cancel();
        }
      }
    }

    if state.queued() >= self.shared.max_queued {
      // Drop the oldest task of the lowest priority that is not more
      // important than this one; otherwise drop this one.
      let victim = (priority as usize..PRIORITIES)
        .rev()
        .find(|&level| !state.queues[level].is_empty());
      match victim.and_then(|level| state.queues[level].pop_front()) {
        Some(dropped) => {
          tracing::warn!(
            task = dropped.name,
            "task queue full; dropped a queued task"
          );
          dropped.token.cancel();
          state.stats.entry(dropped.name).or_default().dropped += 1;
        }
        None => {
          tracing::warn!(task = name, "task queue full; dropped new task");
          token.cancel();
          state.stats.entry(name).or_default().dropped += 1;
          return handle;
        }
      }
    }

    state.queues[priority as usize].push_back(Job {
      name,
      key,
      token,
      queued_at: Instant::now(),
      work: Box::new(work),
    });
    drop(state);
    self.shared.ready.notify_one();
    handle
  }

  /// Statistics per task name.
  pub fn stats(&self) -> Vec<(&'static str, TaskStats)> {
    self
      .shared
      .lock()
      .stats
      .iter()
      .map(|(&name, stats)| {
        (
          name,
          TaskStats {
            completed: stats.completed,
            cancelled: stats.cancelled,
            dropped: stats.dropped,
            panicked: stats.panicked,
            wait: stats.wait.summary(),
            run: stats.run.summary(),
          },
        )
      })
      .collect()
  }

  pub fn queued(&self) -> usize {
    self.shared.lock().queued()
  }

  /// Cancels all tasks, waits for running ones to return, and stops the
  /// workers. Later submissions are cancelled immediately.
  pub fn shutdown(&self) {
    {
      let mut state = self.shared.lock();
      state.shutdown = true;
      for queue in &mut state.queues {
        for job in queue.drain(..) {
          job.token.cancel();
        }
      }
      for (_, token) in &state.running {
        token.cancel();
      }
    }
    self.shared.ready.notify_all();
    let workers = std::mem::take(&mut *self.workers.lock().unwrap_or_else(PoisonError::into_inner));
    for worker in workers {
      let _ = worker.join();
    }
  }
}

impl Drop for Scheduler {
  fn drop(&mut self) {
    self.shutdown();
  }
}

fn worker(shared: &Shared) {
  loop {
    let job = {
      let mut state = shared.lock();
      loop {
        if state.shutdown {
          return;
        }
        if let Some(job) = state.queues.iter_mut().find_map(VecDeque::pop_front) {
          break job;
        }
        state = shared
          .ready
          .wait(state)
          .unwrap_or_else(PoisonError::into_inner);
      }
    };
    run(shared, job);
  }
}

fn run(shared: &Shared, job: Job) {
  let waited = job.queued_at.elapsed();
  if job.token.is_cancelled() {
    shared.lock().stats.entry(job.name).or_default().cancelled += 1;
    return;
  }
  shared
    .lock()
    .running
    .push((job.key.clone(), job.token.clone()));

  let started = Instant::now();
  let token = job.token.clone();
  let outcome = catch_unwind(AssertUnwindSafe(|| (job.work)(&token)));
  let elapsed = started.elapsed();
  if elapsed > Duration::from_secs(1) {
    tracing::debug!(
      task = job.name,
      elapsed_ms = elapsed.as_millis() as u64,
      "slow task"
    );
  }

  let mut state = shared.lock();
  state.running.retain(|(_, running)| !running.same(&token));
  let stats = state.stats.entry(job.name).or_default();
  stats.wait.record(waited);
  stats.run.record(elapsed);
  match outcome {
    Err(_) => {
      tracing::error!(task = job.name, "task panicked");
      stats.panicked += 1;
    }
    Ok(()) if token.is_cancelled() => stats.cancelled += 1,
    Ok(()) => stats.completed += 1,
  }
}

#[cfg(test)]
mod tests {
  use std::sync::mpsc;

  use super::*;

  fn scheduler(workers: usize, max_queued: usize) -> Scheduler {
    Scheduler::new(SchedulerConfig {
      workers,
      max_queued,
    })
  }

  /// Occupies the only worker until the returned sender is used.
  fn block_worker(scheduler: &Scheduler) -> mpsc::Sender<()> {
    let (release, wait) = mpsc::channel::<()>();
    let (started, has_started) = mpsc::channel();
    scheduler.spawn("blocker", Priority::High, None, move |_| {
      let _ = started.send(());
      let _ = wait.recv();
    });
    has_started.recv().expect("blocker started");
    release
  }

  #[test]
  fn runs_tasks_in_priority_order() {
    let scheduler = scheduler(1, 16);
    let release = block_worker(&scheduler);
    let (sender, results) = mpsc::channel();
    for (label, priority) in [
      ("low", Priority::Low),
      ("normal", Priority::Normal),
      ("high", Priority::High),
    ] {
      let sender = sender.clone();
      scheduler.spawn("ordered", priority, None, move |_| {
        let _ = sender.send(label);
      });
    }
    release.send(()).expect("release");
    let order: Vec<_> = (0..3).map(|_| results.recv().expect("result")).collect();
    assert_eq!(order, vec!["high", "normal", "low"]);
  }

  #[test]
  fn keyed_tasks_replace_queued_and_cancel_running_ones() {
    let scheduler = scheduler(1, 16);
    let (started, has_started) = mpsc::channel();
    let (finished, has_finished) = mpsc::channel();
    let first = scheduler.spawn(
      "search",
      Priority::High,
      Some("search".into()),
      move |token| {
        let _ = started.send(());
        while !token.is_cancelled() {
          std::thread::sleep(Duration::from_millis(1));
        }
        let _ = finished.send("first stopped");
      },
    );
    has_started.recv().expect("first started");

    let (sender, results) = mpsc::channel();
    let queued_sender = sender.clone();
    let second = scheduler.spawn("search", Priority::High, Some("search".into()), move |_| {
      let _ = queued_sender.send("second");
    });
    let _third = scheduler.spawn("search", Priority::High, Some("search".into()), move |_| {
      let _ = sender.send("third");
    });

    assert_eq!(
      has_finished.recv().expect("first finished"),
      "first stopped"
    );
    assert!(first.is_cancelled());
    assert!(second.is_cancelled());
    assert_eq!(results.recv().expect("result"), "third");
    assert!(results.recv_timeout(Duration::from_millis(50)).is_err());
  }

  #[test]
  fn cancelled_tasks_do_not_start() {
    let scheduler = scheduler(1, 16);
    let release = block_worker(&scheduler);
    let (sender, results) = mpsc::channel();
    let handle = scheduler.spawn("cancelled", Priority::Normal, None, move |_| {
      let _ = sender.send(());
    });
    handle.cancel();
    release.send(()).expect("release");
    assert!(results.recv_timeout(Duration::from_millis(100)).is_err());
  }

  #[test]
  fn a_full_queue_drops_the_least_important_task() {
    let scheduler = scheduler(1, 2);
    let release = block_worker(&scheduler);
    let low = scheduler.spawn("low", Priority::Low, None, |_| {});
    let _normal = scheduler.spawn("normal", Priority::Normal, None, |_| {});
    let _high = scheduler.spawn("high", Priority::High, None, |_| {});
    assert!(low.is_cancelled(), "the low-priority task makes room");
    let rejected = scheduler.spawn("low", Priority::Low, None, |_| {});
    assert!(rejected.is_cancelled(), "nothing less important to drop");
    assert_eq!(scheduler.queued(), 2);
    release.send(()).expect("release");
  }

  #[test]
  fn panicking_tasks_do_not_take_down_workers() {
    let scheduler = scheduler(1, 16);
    scheduler.spawn("boom", Priority::High, None, |_| panic!("task failure"));
    let (sender, results) = mpsc::channel();
    scheduler.spawn("after", Priority::High, None, move |_| {
      let _ = sender.send(());
    });
    results
      .recv_timeout(Duration::from_secs(5))
      .expect("the worker survived");
    let stats = scheduler.stats();
    let boom = stats
      .iter()
      .find(|(name, _)| *name == "boom")
      .expect("stats");
    assert_eq!(boom.1.panicked, 1);
  }

  #[test]
  fn shutdown_cancels_and_joins() {
    let scheduler = scheduler(2, 16);
    let (started, has_started) = mpsc::channel();
    let running = scheduler.spawn("long", Priority::Normal, None, move |token| {
      let _ = started.send(());
      while !token.is_cancelled() {
        std::thread::sleep(Duration::from_millis(1));
      }
    });
    has_started.recv().expect("started");
    scheduler.shutdown();
    assert!(running.is_cancelled());
    assert!(
      scheduler
        .spawn("late", Priority::High, None, |_| {})
        .is_cancelled()
    );
  }
}
