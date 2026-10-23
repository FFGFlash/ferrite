use std::fmt;
use std::path::{Path, PathBuf};

use ferrite_text::{LineEnding, TextBuffer, TextSnapshot, Version};
use ferrite_workspace::FileStamp;

use crate::history::{History, HistoryLimits, StateId};
use crate::selection::Selections;

/// Stable identifier of an open document. Never reused within a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(pub(crate) u64);

impl DocumentId {
  pub fn get(self) -> u64 {
    self.0
  }

  pub fn from_raw(raw: u64) -> Self {
    Self(raw)
  }
}

impl fmt::Display for DocumentId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "#{}", self.0)
  }
}

#[derive(Debug, Clone)]
pub struct EditorConfig {
  pub tab_size: usize,
  /// Whether Tab inserts spaces instead of a tab character.
  pub insert_spaces: bool,
  /// Line ending for documents that do not contain one yet.
  pub default_line_ending: LineEnding,
  pub history: HistoryLimits,
}

impl Default for EditorConfig {
  fn default() -> Self {
    Self {
      tab_size: 4,
      insert_spaces: true,
      default_line_ending: LineEnding::Lf,
      history: HistoryLimits::default(),
    }
  }
}

#[derive(Debug, Clone)]
struct FileBacking {
  path: PathBuf,
  has_bom: bool,
  /// Disk state when last loaded or saved; `None` if unknown.
  stamp: Option<FileStamp>,
}

/// What to write when saving, captured at one moment. Editing may continue
/// while the snapshot is written; pass the save point back to
/// [`Document::mark_saved`] afterwards.
#[derive(Debug, Clone)]
pub struct SavePoint {
  pub snapshot: TextSnapshot,
  pub has_bom: bool,
  /// Disk stamp the file is expected to still have, if known.
  pub expected_stamp: Option<FileStamp>,
  state: StateId,
}

#[derive(Debug)]
pub struct Document {
  id: DocumentId,
  pub(crate) buffer: TextBuffer,
  pub(crate) selections: Selections,
  pub(crate) history: History,
  pub(crate) line_ending: LineEnding,
  pub(crate) tab_size: usize,
  pub(crate) insert_spaces: bool,
  file: Option<FileBacking>,
  untitled_number: u32,
  saved_state: StateId,
}

impl Document {
  pub(crate) fn new(
    id: DocumentId,
    text: &str,
    file: Option<(PathBuf, bool, FileStamp)>,
    untitled_number: u32,
    config: &EditorConfig,
  ) -> Self {
    let buffer = TextBuffer::from_text(text);
    let line_ending = buffer
      .detect_line_ending()
      .unwrap_or(config.default_line_ending);
    let history = History::new(config.history);
    let saved_state = history.current_state();
    Self {
      id,
      buffer,
      selections: Selections::cursor(0),
      history,
      line_ending,
      tab_size: config.tab_size.max(1),
      insert_spaces: config.insert_spaces,
      file: file.map(|(path, has_bom, stamp)| FileBacking {
        path,
        has_bom,
        stamp: Some(stamp),
      }),
      untitled_number,
      saved_state,
    }
  }

  pub fn id(&self) -> DocumentId {
    self.id
  }

  pub fn buffer(&self) -> &TextBuffer {
    &self.buffer
  }

  pub fn selections(&self) -> &Selections {
    &self.selections
  }

  pub fn version(&self) -> Version {
    self.buffer.version()
  }

  pub fn line_ending(&self) -> LineEnding {
    self.line_ending
  }

  pub fn path(&self) -> Option<&Path> {
    self.file.as_ref().map(|file| file.path.as_path())
  }

  pub fn has_bom(&self) -> bool {
    self.file.as_ref().is_some_and(|file| file.has_bom)
  }

  /// File name for file-backed documents, `Untitled-N` otherwise.
  pub fn title(&self) -> String {
    match self.path().and_then(Path::file_name) {
      Some(name) => name.to_string_lossy().into_owned(),
      None => format!("Untitled-{}", self.untitled_number),
    }
  }

  /// Whether the text differs from what was last loaded or saved. Undoing
  /// back to the saved text makes the document clean again.
  pub fn is_dirty(&self) -> bool {
    self.history.current_state() != self.saved_state
  }

  pub fn can_undo(&self) -> bool {
    self.history.can_undo()
  }

  pub fn can_redo(&self) -> bool {
    self.history.can_redo()
  }

  /// Captures the current text for saving.
  pub fn save_point(&self) -> SavePoint {
    SavePoint {
      snapshot: self.buffer.snapshot(),
      has_bom: self.has_bom(),
      expected_stamp: self.file.as_ref().and_then(|file| file.stamp.clone()),
      state: self.history.current_state(),
    }
  }

  /// Records that `point` was written to `path`, which may differ from the
  /// current path (Save As). Edits made since the save point stay dirty.
  pub fn mark_saved(&mut self, point: &SavePoint, path: PathBuf, stamp: FileStamp) {
    self.saved_state = point.state;
    self.file = Some(FileBacking {
      path,
      has_bom: point.has_bom,
      stamp: Some(stamp),
    });
    self.history.seal();
  }

  /// The selected text of every non-empty selection, joined by the
  /// document's line ending, or `None` if nothing is selected.
  pub fn selected_text(&self) -> Option<String> {
    let parts: Vec<String> = self
      .selections
      .iter()
      .filter(|selection| !selection.is_empty())
      .map(|selection| self.buffer.text_in(selection.range()))
      .collect();
    (!parts.is_empty()).then(|| parts.join(self.line_ending.as_str()))
  }

  /// The visual column of `offset`: grapheme clusters since the start of the
  /// line, with tabs advancing to the next tab stop.
  pub fn visual_column(&self, offset: usize) -> usize {
    let mut position = self.buffer.line_start(self.buffer.line_of(offset));
    let offset = offset.min(self.buffer.len());
    let mut column = 0;
    while position < offset {
      column = self.advance_column(column, position);
      position = self.buffer.next_grapheme_boundary(position);
    }
    column
  }

  /// The offset in `line` closest to (not past) visual column `goal`.
  pub(crate) fn offset_at_visual_column(&self, line: usize, goal: usize) -> usize {
    let mut position = self.buffer.line_start(line);
    let end = self.buffer.line_end(line);
    let mut column = 0;
    while position < end {
      let next_column = self.advance_column(column, position);
      if next_column > goal {
        break;
      }
      column = next_column;
      position = self.buffer.next_grapheme_boundary(position);
    }
    position
  }

  fn advance_column(&self, column: usize, position: usize) -> usize {
    if self.buffer.chars_at(position).next() == Some('\t') {
      column + self.tab_size - column % self.tab_size
    } else {
      column + 1
    }
  }
}
