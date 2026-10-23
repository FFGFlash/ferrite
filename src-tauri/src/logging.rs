//! Process-wide structured logging.
//!
//! Events are written to stderr and, when a log directory is available, to a
//! daily rolling file. The filter is read from `FERRITE_LOG` using
//! `tracing_subscriber::EnvFilter` directive syntax, for example
//! `FERRITE_LOG=debug,frontend=trace`.

use std::path::Path;
use std::sync::Mutex;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub const FILTER_ENV: &str = "FERRITE_LOG";
const DEFAULT_FILTER: &str = "info";
const FILE_PREFIX: &str = "ferrite";
const RETAINED_FILES: usize = 7;

/// Owns the background file writer. Pending file output is written when the
/// guard is flushed or dropped.
pub struct LogGuard(Mutex<Option<WorkerGuard>>);

impl LogGuard {
    /// Flushes and closes the log file. Later events still reach stderr.
    pub fn flush(&self) {
        if let Ok(mut guard) = self.0.lock() {
            guard.take();
        }
    }
}

/// Installs the global subscriber. Never fails: problems with the filter or
/// the log file are reported through the subscriber once it is running.
pub fn init(log_dir: Option<&Path>) -> LogGuard {
    let (filter, filter_error) = match std::env::var(FILTER_ENV) {
        Ok(directives) => match EnvFilter::try_new(&directives) {
            Ok(filter) => (filter, None),
            Err(error) => (EnvFilter::new(DEFAULT_FILTER), Some(error)),
        },
        Err(_) => (EnvFilter::new(DEFAULT_FILTER), None),
    };

    let (file_layer, file_guard, file_error) = match log_dir.map(open_file_appender) {
        Some(Ok(appender)) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer().with_ansi(false).with_writer(writer);
            (Some(layer), Some(guard), None)
        }
        Some(Err(error)) => (None, None, Some(error)),
        None => (None, None, None),
    };

    let installed = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(file_layer)
        .try_init();

    if let Err(error) = installed {
        tracing::debug!(%error, "global subscriber already installed");
    }
    if let Some(error) = filter_error {
        tracing::warn!(%error, "invalid {FILTER_ENV} directives; using \"{DEFAULT_FILTER}\"");
    }
    if let Some(error) = file_error {
        tracing::warn!(%error, "file logging disabled");
    }

    LogGuard(Mutex::new(file_guard))
}

/// Routes panic messages through `tracing` so they reach the log file, then
/// defers to the previous hook.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(target: "panic", "{info}");
        previous(info);
    }));
}

fn open_file_appender(dir: &Path) -> Result<RollingFileAppender, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(RETAINED_FILES)
        .build(dir)?;
    Ok(appender)
}
