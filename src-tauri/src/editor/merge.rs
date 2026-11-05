//! Merge editors: the changes two versions of a file made to their common
//! base, where each is in the result (a document being edited), and
//! accepting a side's change into it. See `ferrite_diff::merge`.

use ferrite_core::{Document, DocumentId};
use ferrite_diff::DiffError;
use ferrite_diff::merge::{self, MergeRange, MergeTexts, RangeState};
use ferrite_text::Edit;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::EditorState;
use crate::error::CommandError;
use crate::events::{self, AppEvent};
use crate::extensions::ExtensionsState;
use crate::perf::Perf;

/// The documents a merge editor shows.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeDocuments {
  pub base: u64,
  pub incoming: u64,
  pub current: u64,
  pub result: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RangeStateDto {
  Base,
  Incoming,
  Current,
  CurrentIncoming,
  IncomingCurrent,
  Manual,
}

impl From<RangeState> for RangeStateDto {
  fn from(state: RangeState) -> Self {
    match state {
      RangeState::Base => Self::Base,
      RangeState::Incoming => Self::Incoming,
      RangeState::Current => Self::Current,
      RangeState::CurrentIncoming => Self::CurrentIncoming,
      RangeState::IncomingCurrent => Self::IncomingCurrent,
      RangeState::Manual => Self::Manual,
    }
  }
}

impl From<RangeStateDto> for RangeState {
  fn from(state: RangeStateDto) -> Self {
    match state {
      RangeStateDto::Base => Self::Base,
      RangeStateDto::Incoming => Self::Incoming,
      RangeStateDto::Current => Self::Current,
      RangeStateDto::CurrentIncoming => Self::CurrentIncoming,
      RangeStateDto::IncomingCurrent => Self::IncomingCurrent,
      RangeStateDto::Manual => Self::Manual,
    }
  }
}

/// A range the sides changed: its lines (`[start, end)`) in each document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRangeDto {
  pub base: [usize; 2],
  pub incoming: [usize; 2],
  pub current: [usize; 2],
  pub result: [usize; 2],
  pub conflicting: bool,
  /// Which sides changed it.
  pub incoming_changed: bool,
  pub current_changed: bool,
  pub state: RangeStateDto,
}

/// Where the result changed from the base away from the ranges.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeEditDto {
  pub base: [usize; 2],
  pub result: [usize; 2],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeDto {
  pub ranges: Vec<MergeRangeDto>,
  pub edits: Vec<MergeEditDto>,
  /// The result's version these are for.
  pub version: u64,
}

const fn pair(range: &std::ops::Range<usize>) -> [usize; 2] {
  [range.start, range.end]
}

fn too_large(_: DiffError) -> CommandError {
  CommandError::new(
    "tooLarge",
    format!(
      "Files with more than {} lines aren't merged.",
      ferrite_diff::MAX_LINES
    ),
  )
}

struct Texts {
  base: String,
  incoming: String,
  current: String,
  result: String,
  version: u64,
}

fn texts(state: &EditorState, documents: MergeDocuments) -> Result<Texts, CommandError> {
  let editor = state.lock();
  let text = |id| -> Result<String, CommandError> {
    Ok(
      editor
        .document(DocumentId::from_raw(id))?
        .buffer()
        .contents(),
    )
  };
  let result = editor.document(DocumentId::from_raw(documents.result))?;
  Ok(Texts {
    base: text(documents.base)?,
    incoming: text(documents.incoming)?,
    current: text(documents.current)?,
    result: result.buffer().contents(),
    version: result.buffer().version().get(),
  })
}

/// The changes each side made and what the result has of them.
#[tauri::command]
pub async fn merge_compute(
  app: AppHandle,
  documents: MergeDocuments,
) -> Result<MergeDto, CommandError> {
  tauri::async_runtime::spawn_blocking(move || {
    let started = std::time::Instant::now();
    let texts = texts(&app.state::<EditorState>(), documents)?;
    let lines = MergeTexts::new(&texts.base, &texts.incoming, &texts.current);
    let ranges = merge::ranges(&texts.base, &texts.incoming, &texts.current).map_err(too_large)?;
    let (shown, edits) =
      merge::in_result(&ranges, &lines, &texts.base, &texts.result).map_err(too_large)?;
    app
      .state::<Perf>()
      .record("merge.compute", started.elapsed());
    Ok(MergeDto {
      ranges: ranges
        .iter()
        .zip(shown)
        .map(|(range, shown)| MergeRangeDto {
          base: pair(&range.base),
          incoming: pair(&range.incoming),
          current: pair(&range.current),
          result: pair(&shown.lines),
          conflicting: range.conflicting,
          incoming_changed: range.changes(merge::Side::Incoming, &lines),
          current_changed: range.changes(merge::Side::Current, &lines),
          state: shown.state.into(),
        })
        .collect(),
      edits: edits
        .iter()
        .map(|edit| MergeEditDto {
          base: pair(&edit.base),
          result: pair(&edit.result),
        })
        .collect(),
      version: texts.version,
    })
  })
  .await
  .map_err(CommandError::internal)?
}

/// Replaces lines `lines` of the result with `with`, as one undoable edit.
fn replace_lines(
  app: &AppHandle,
  document: &mut Document,
  chosen: &[(std::ops::Range<usize>, Vec<&str>)],
) -> Result<bool, CommandError> {
  let text = document.buffer().contents();
  let mut source = String::new();
  let mut pairs = Vec::with_capacity(chosen.len());
  let mut at = 0;
  for (lines, with) in chosen {
    for line in with {
      source.push_str(line);
      source.push('\n');
    }
    pairs.push((lines.clone(), at..at + with.len()));
    at += with.len();
  }
  let source = source.strip_suffix('\n').unwrap_or(&source).to_owned();
  let edits = ferrite_diff::splices(&text, &source, &pairs)
    .into_iter()
    .map(|splice| Edit::new(splice.range, splice.text))
    .collect();
  let edited = document.apply_edits(edits)?;
  if edited {
    app.state::<ExtensionsState>().sync(document);
    events::emit(
      app,
      AppEvent::DocumentEdited {
        document_id: document.id().get(),
      },
    );
  }
  Ok(edited)
}

fn ranges_now(texts: &Texts) -> Result<(Vec<MergeRange>, Vec<merge::ResultRange>), CommandError> {
  let lines = MergeTexts::new(&texts.base, &texts.incoming, &texts.current);
  let ranges = merge::ranges(&texts.base, &texts.incoming, &texts.current).map_err(too_large)?;
  let (shown, _) =
    merge::in_result(&ranges, &lines, &texts.base, &texts.result).map_err(too_large)?;
  Ok((ranges, shown))
}

/// Makes range `index` of the result what `state` gives (accepting a
/// side, both, or neither), if the result is still at `version`. Returns
/// whether it changed.
#[tauri::command]
pub fn merge_set_range(
  app: AppHandle,
  state: State<'_, EditorState>,
  documents: MergeDocuments,
  version: u64,
  index: usize,
  to: RangeStateDto,
) -> Result<bool, CommandError> {
  let texts = texts(&state, documents)?;
  if texts.version != version {
    return Ok(false);
  }
  let (ranges, shown) = ranges_now(&texts)?;
  let (Some(range), Some(shown)) = (ranges.get(index), shown.get(index)) else {
    return Ok(false);
  };
  let lines = MergeTexts::new(&texts.base, &texts.incoming, &texts.current);
  let with = lines.lines_for(range, to.into());
  let mut editor = state.lock();
  let document = editor.document_mut(DocumentId::from_raw(documents.result))?;
  if document.buffer().version().get() != version {
    return Ok(false);
  }
  replace_lines(&app, document, &[(shown.lines.clone(), with)])
}

/// Starts the result again: the base with the changes that don't conflict
/// made, and the conflicts' lines as the base has them.
#[tauri::command]
pub fn merge_reset(
  app: AppHandle,
  state: State<'_, EditorState>,
  documents: MergeDocuments,
) -> Result<bool, CommandError> {
  let texts = texts(&state, documents)?;
  let lines = MergeTexts::new(&texts.base, &texts.incoming, &texts.current);
  let ranges = merge::ranges(&texts.base, &texts.incoming, &texts.current).map_err(too_large)?;
  let initial = merge::initial(&texts.base, &ranges, &lines);
  if initial == texts.result {
    return Ok(false);
  }
  let mut editor = state.lock();
  let document = editor.document_mut(DocumentId::from_raw(documents.result))?;
  let len = document.buffer().len();
  let edited = document.apply_edits(vec![Edit::new(0..len, initial)])?;
  if edited {
    app.state::<ExtensionsState>().sync(document);
    events::emit(
      &app,
      AppEvent::DocumentEdited {
        document_id: document.id().get(),
      },
    );
  }
  Ok(edited)
}
