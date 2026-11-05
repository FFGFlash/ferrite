//! Process-wide structured logging.
//!
//! Events are written to stderr and, when a log directory is available, to a
//! daily rolling file. The filter is read from `FLAREITE_LOG` using
//! `tracing_subscriber::EnvFilter` directive syntax, for example
//! `FLAREITE_LOG=debug,frontend=trace`.
//!
//! Recent events are also kept in memory for the Output view, in channels:
//! Flareite's own, one per extension (events with an `extension` field), and
//! one per language server (a `server` field). Those channels keep every
//! level, so an extension's debug messages and a server's stderr show there
//! even though the file only has `info` and above.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::State;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::Context;
use tracing_subscriber::{EnvFilter, Layer, fmt, prelude::*};

pub const FILTER_ENV: &str = "FLAREITE_LOG";
const DEFAULT_FILTER: &str = "info";
const FILE_PREFIX: &str = "flareite";
const RETAINED_FILES: usize = 7;

/// Flareite's own channel; others are `extension:<id>` and `server:<name>`.
pub const APP_CHANNEL: &str = "flareite";
/// Records kept per channel, so a chatty extension can't push out the rest.
const KEPT_PER_CHANNEL: usize = 2_000;
/// Channels kept; events for more go to Flareite's.
const MAX_CHANNELS: usize = 256;
/// Longest message kept in memory; the log file has all of it.
const MAX_MESSAGE_BYTES: usize = 8 * 1024;

/// Owns the background file writer and the records kept for the Output
/// view. Pending file output is written when the guard is flushed or
/// dropped.
pub struct LogGuard {
  file: Mutex<Option<WorkerGuard>>,
  dir: Option<PathBuf>,
  buffer: LogBuffer,
}

impl LogGuard {
  /// Flushes and closes the log file. Later events still reach stderr.
  pub fn flush(&self) {
    if let Ok(mut guard) = self.file.lock() {
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

  let buffer = LogBuffer::default();
  let installed = tracing_subscriber::registry()
    .with(
      fmt::layer()
        .with_writer(std::io::stderr)
        .and_then(file_layer)
        .with_filter(filter),
    )
    .with(BufferLayer(buffer.clone()).with_filter(filter_fn(kept_in_memory)))
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

  LogGuard {
    file: Mutex::new(file_guard),
    dir: log_dir.map(Path::to_path_buf),
    buffer,
  }
}

/// Flareite's events from `info` up, and every event of an extension or a
/// language server. Decided per call site, so other debug events cost
/// nothing.
fn kept_in_memory(metadata: &tracing::Metadata<'_>) -> bool {
  *metadata.level() <= Level::INFO
    || metadata.fields().field("extension").is_some()
    || metadata.fields().field("server").is_some()
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

/// A log event, as the Output view shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
  /// Increases with each record, across channels.
  pub seq: u64,
  /// Milliseconds since the Unix epoch.
  pub time: u64,
  /// `error`, `warn`, `info`, `debug`, or `trace`.
  pub level: &'static str,
  pub target: String,
  pub message: String,
  /// The event's other fields, as `name=value` pairs.
  pub fields: String,
}

#[derive(Default)]
struct Channels {
  next: u64,
  channels: BTreeMap<String, VecDeque<LogRecord>>,
}

/// Recent records by channel. Cheap to clone; clones share the records.
#[derive(Clone, Default)]
pub struct LogBuffer(Arc<Mutex<Channels>>);

impl LogBuffer {
  fn push(&self, channel: String, mut record: LogRecord) {
    let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    state.next += 1;
    record.seq = state.next;
    let channel = if state.channels.contains_key(&channel) || state.channels.len() < MAX_CHANNELS {
      channel
    } else {
      APP_CHANNEL.to_owned()
    };
    let records = state.channels.entry(channel).or_default();
    if records.len() == KEPT_PER_CHANNEL {
      records.pop_front();
    }
    records.push_back(record);
  }

  /// A channel's records after `after` (a `seq`), oldest first.
  pub fn read(&self, channel: &str, after: u64) -> Vec<LogRecord> {
    let state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    state
      .channels
      .get(channel)
      .map_or_else(Vec::new, |records| {
        let start = records.partition_point(|record| record.seq <= after);
        records.range(start..).cloned().collect()
      })
  }

  /// Every channel with records, Flareite's first, and its newest `seq`.
  pub fn channels(&self) -> Vec<LogChannel> {
    let state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    let mut channels: Vec<LogChannel> = state
      .channels
      .iter()
      .map(|(id, records)| LogChannel {
        id: id.clone(),
        last: records.back().map_or(0, |record| record.seq),
      })
      .collect();
    channels.sort_by_key(|channel| channel.id != APP_CHANNEL);
    channels
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogChannel {
  /// `flareite`, `extension:<id>`, or `server:<name>`.
  pub id: String,
  /// The newest record's `seq`.
  pub last: u64,
}

struct BufferLayer(LogBuffer);

impl<S: Subscriber> Layer<S> for BufferLayer {
  fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
    let mut fields = Fields::default();
    event.record(&mut fields);
    let channel = match (fields.extension, fields.server) {
      (Some(id), _) => format!("extension:{id}"),
      (None, Some(name)) => format!("server:{name}"),
      (None, None) => APP_CHANNEL.to_owned(),
    };
    let metadata = event.metadata();
    let mut message = fields.message;
    if message.len() > MAX_MESSAGE_BYTES {
      let mut end = MAX_MESSAGE_BYTES;
      while !message.is_char_boundary(end) {
        end -= 1;
      }
      message.truncate(end);
      message.push('…');
    }
    let time = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
      });
    self.0.push(
      channel,
      LogRecord {
        seq: 0,
        time,
        level: match *metadata.level() {
          Level::ERROR => "error",
          Level::WARN => "warn",
          Level::INFO => "info",
          Level::DEBUG => "debug",
          Level::TRACE => "trace",
        },
        target: metadata.target().to_owned(),
        message,
        fields: fields.rest,
      },
    );
  }
}

/// An event's fields: the message, the channel's fields, and the rest as
/// text.
#[derive(Default)]
struct Fields {
  message: String,
  extension: Option<String>,
  server: Option<String>,
  rest: String,
}

impl Fields {
  fn put(&mut self, field: &Field, value: String) {
    match field.name() {
      "message" => self.message = value,
      "extension" => self.extension = Some(value),
      "server" => self.server = Some(value),
      name => {
        if !self.rest.is_empty() {
          self.rest.push(' ');
        }
        let _ = write!(self.rest, "{name}={value}");
      }
    }
  }
}

impl Visit for Fields {
  fn record_str(&mut self, field: &Field, value: &str) {
    self.put(field, value.to_owned());
  }

  fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
    self.put(field, format!("{value:?}"));
  }
}

/// Recent log records for the Output view.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsDto {
  channels: Vec<LogChannel>,
  /// The channel's records after `after`.
  records: Vec<LogRecord>,
}

/// The channels, and a channel's records after `after` (a `seq`; 0 for
/// all that are kept).
#[tauri::command]
pub fn logs_read(state: State<'_, LogGuard>, channel: String, after: u64) -> LogsDto {
  LogsDto {
    channels: state.buffer.channels(),
    records: state.buffer.read(&channel, after),
  }
}

/// Today's log file (the newest), which has everything written since the
/// app started, unlike the Output view.
#[tauri::command]
pub fn logs_file(state: State<'_, LogGuard>) -> Option<String> {
  let entries = std::fs::read_dir(state.dir.as_ref()?).ok()?;
  entries
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| {
      path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(&format!("{FILE_PREFIX}.")) && name.ends_with(".log"))
    })
    // Named by date, so the newest sorts last.
    .max()
    .map(|path| path.display().to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn capture(f: impl FnOnce()) -> LogBuffer {
    let buffer = LogBuffer::default();
    let subscriber = tracing_subscriber::registry()
      .with(BufferLayer(buffer.clone()).with_filter(filter_fn(kept_in_memory)));
    tracing::subscriber::with_default(subscriber, f);
    buffer
  }

  #[test]
  fn keeps_records_by_channel_with_every_level_for_extensions_and_servers() {
    let buffer = capture(|| {
      tracing::info!(path = "a.rs", "saved file");
      tracing::debug!("hidden: Flareite's debug events aren't kept");
      tracing::debug!(extension = "example.tidy", "checking {} lines", 3);
      tracing::warn!(extension = %"example.tidy", dropped = 2, "too many messages");
      tracing::debug!(server = "rust-analyzer", "stderr line");
    });
    let ids: Vec<String> = buffer.channels().into_iter().map(|c| c.id).collect();
    assert_eq!(
      ids,
      ["flareite", "extension:example.tidy", "server:rust-analyzer"]
    );
    let app = buffer.read(APP_CHANNEL, 0);
    assert_eq!(app.len(), 1);
    assert_eq!(
      (
        app[0].level,
        app[0].message.as_str(),
        app[0].fields.as_str()
      ),
      ("info", "saved file", "path=a.rs")
    );
    let tidy = buffer.read("extension:example.tidy", 0);
    assert_eq!(tidy.len(), 2);
    assert_eq!(tidy[0].message, "checking 3 lines");
    assert_eq!(
      (tidy[1].level, tidy[1].fields.as_str()),
      ("warn", "dropped=2")
    );
    // Reading after a record returns only newer ones.
    assert_eq!(
      buffer.read("extension:example.tidy", tidy[0].seq),
      [tidy[1].clone()]
    );
    assert!(buffer.read("nothing", 0).is_empty());
  }

  #[test]
  fn keeps_the_newest_records_and_cuts_long_messages() {
    let long = "é".repeat(MAX_MESSAGE_BYTES);
    let buffer = capture(|| {
      for i in 0..KEPT_PER_CHANNEL + 5 {
        tracing::info!("record {i}");
      }
      tracing::info!(extension = "x", "{long}");
    });
    let app = buffer.read(APP_CHANNEL, 0);
    assert_eq!(app.len(), KEPT_PER_CHANNEL);
    assert_eq!(app[0].message, "record 5");
    let cut = &buffer.read("extension:x", 0)[0].message;
    assert!(cut.len() <= MAX_MESSAGE_BYTES + '…'.len_utf8());
    assert!(cut.ends_with('…'));
  }
}
