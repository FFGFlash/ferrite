//! Background work for Ferrite.
//!
//! - [`Scheduler`]: a fixed pool of worker threads with priorities,
//!   cooperative cancellation, keyed replacement (a new task with the same
//!   key cancels the old one), and a bounded queue.
//! - [`Metrics`]: named timing histograms for instrumentation.
//!
//! Nothing here depends on the editor; tasks are plain closures.

mod metrics;
mod scheduler;

pub use metrics::{Histogram, Metrics, Summary, Timer};
pub use scheduler::{CancelToken, Priority, Scheduler, SchedulerConfig, TaskHandle, TaskStats};
