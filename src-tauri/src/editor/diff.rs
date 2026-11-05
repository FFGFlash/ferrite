//! Comparing two open documents for diff tabs.

use flareite_core::DocumentId;
use flareite_diff::{DiffError, DiffOptions, Point};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::EditorState;
use super::protocol::{Position, RangeDto};
use crate::error::CommandError;
use crate::perf::Perf;

/// Lines that changed: `original` lines (`[start, end)`) became `modified`
/// ones. An empty range is a place between lines.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineChangeDto {
  pub original: [usize; 2],
  pub modified: [usize; 2],
  /// The characters that changed within them, if they were compared.
  pub chars: Vec<CharChangeDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharChangeDto {
  pub original: RangeDto,
  pub modified: RangeDto,
}

impl CharChangeDto {
  pub fn of(chars: &flareite_diff::CharChange) -> Self {
    Self {
      original: range(chars.original.clone()),
      modified: range(chars.modified.clone()),
    }
  }
}

fn range(range: std::ops::Range<Point>) -> RangeDto {
  let position = |point: Point| Position {
    line: point.line,
    column: point.column,
  };
  RangeDto {
    start: position(range.start),
    end: position(range.end),
  }
}

/// The changes from document `original` to document `modified`. With
/// `ignore_trim_whitespace`, whitespace at the start and end of lines
/// doesn't count.
#[tauri::command]
pub async fn diff_compute(
  app: AppHandle,
  original: u64,
  modified: u64,
  ignore_trim_whitespace: bool,
) -> Result<Vec<LineChangeDto>, CommandError> {
  tauri::async_runtime::spawn_blocking(move || {
    let started = std::time::Instant::now();
    let (before, after) = {
      let state = app.state::<EditorState>();
      let editor = state.lock();
      let text = |id| {
        editor
          .document(DocumentId::from_raw(id))
          .map(|document| document.buffer().contents())
      };
      (text(original)?, text(modified)?)
    };
    let options = DiffOptions {
      ignore_trim_whitespace,
    };
    let changes = flareite_diff::diff(&before, &after, options).map_err(|error| match error {
      DiffError::TooLarge => CommandError::new(
        "tooLarge",
        format!(
          "Files with more than {} lines aren't compared.",
          flareite_diff::MAX_LINES
        ),
      ),
    })?;
    app
      .state::<Perf>()
      .record("diff.compute", started.elapsed());
    Ok(
      changes
        .into_iter()
        .map(|change| LineChangeDto {
          original: [change.original.start, change.original.end],
          modified: [change.modified.start, change.modified.end],
          chars: change
            .chars
            .into_iter()
            .map(|chars| CharChangeDto::of(&chars))
            .collect(),
        })
        .collect(),
    )
  })
  .await
  .map_err(CommandError::internal)?
}
