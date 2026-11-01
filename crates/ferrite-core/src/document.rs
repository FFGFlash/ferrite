use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};

use ferrite_text::{
  Bias, ChangeSet, Edit, LineEnding, TextBuffer, TextSnapshot, Version, normalize_line_endings,
};
use ferrite_workspace::FileStamp;

use crate::error::CoreError;
use crate::history::{EditKind, History, HistoryLimits, StateId};
use crate::selection::{Selection, Selections};

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

/// How far into a line visual columns are computed exactly (grapheme by
/// grapheme, with tab stops). Past this they count characters.
pub const EXACT_COLUMN_BYTES: usize = 4096;

/// Most earlier selections kept for cursor undo.
const MAX_CURSOR_HISTORY: usize = 100;

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
  /// Earlier selections, newest last, with the version they belong to.
  cursor_history: VecDeque<(Version, Selections)>,
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
      cursor_history: VecDeque::new(),
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

  /// The disk stamp recorded when the file was last loaded or saved.
  pub fn disk_stamp(&self) -> Option<&FileStamp> {
    self.file.as_ref().and_then(|file| file.stamp.as_ref())
  }

  /// Replaces the text with a newer version read from disk and marks the
  /// document saved. Only the differing middle section is replaced, so
  /// cursors before and after it stay put, and the reload can be undone.
  /// Returns whether the text changed.
  pub fn reload(&mut self, text: &str, has_bom: bool, stamp: FileStamp) -> Result<bool, CoreError> {
    let changed = self.replace_text(text)?;
    self.saved_state = self.history.current_state();
    if let Some(file) = &mut self.file {
      file.has_bom = has_bom;
      file.stamp = Some(stamp);
    }
    self.history.seal();
    Ok(changed)
  }

  /// Replaces the whole text with `text` (taken as is, line endings
  /// included) as one undoable change, e.g. to restore a backup. Only the
  /// differing middle section is replaced, so cursors before and after it
  /// stay put. Returns whether the text changed.
  pub fn replace_text(&mut self, text: &str) -> Result<bool, CoreError> {
    let current = self.buffer.contents();
    if current == text {
      return Ok(false);
    }
    let (prefix, suffix) = common_affixes(&current, text);
    let edit = Edit::new(
      prefix..current.len() - suffix,
      &text[prefix..text.len() - suffix],
    );
    let changes = ChangeSet::single(edit)?;
    let before = self.selections.clone();
    let inverse = self.buffer.apply(&changes)?;
    let after = before.map(|s| {
      Selection::new(
        changes.map_offset(s.anchor, Bias::Before),
        changes.map_offset(s.head, Bias::Before),
      )
    });
    self.selections = after.clone();
    self
      .history
      .record(changes, inverse, before, after, EditKind::Other, false);
    self.history.seal();
    Ok(true)
  }

  /// Applies edits made outside the editor (e.g. by an extension) as one
  /// undoable change. Line breaks in inserted text are converted to the
  /// document's line ending; selections move with the text. Returns whether
  /// the text changed.
  pub fn apply_edits(&mut self, edits: Vec<Edit>) -> Result<bool, CoreError> {
    let edits = edits
      .into_iter()
      .map(|edit| {
        let text = normalize_line_endings(&edit.text, self.line_ending).into_owned();
        Edit::new(edit.range, text)
      })
      .collect();
    let changes = ChangeSet::new(edits)?;
    if changes.edits().iter().all(Edit::is_noop) {
      return Ok(false);
    }
    let before = self.selections.clone();
    let inverse = self.buffer.apply(&changes)?;
    let after = before.map(|s| {
      Selection::new(
        changes.map_offset(s.anchor, Bias::After),
        changes.map_offset(s.head, Bias::After),
      )
    });
    self.selections = after.clone();
    self
      .history
      .record(changes, inverse, before, after, EditKind::Other, false);
    self.history.seal();
    Ok(true)
  }

  /// Records selections that were just replaced, for cursor undo.
  pub(crate) fn remember_selections(&mut self, previous: Selections) {
    if self.cursor_history.len() == MAX_CURSOR_HISTORY {
      self.cursor_history.pop_front();
    }
    self
      .cursor_history
      .push_back((self.buffer.version(), previous));
  }

  /// Restores the selections from before the last selection change. Only
  /// selections from the current text count: editing clears the history.
  pub(crate) fn cursor_undo(&mut self) {
    let version = self.buffer.version();
    while let Some((at, selections)) = self.cursor_history.pop_back() {
      if at != version {
        self.cursor_history.clear();
        return;
      }
      if selections != self.selections {
        self.history.seal();
        self.selections = selections;
        return;
      }
    }
  }

  pub(crate) fn set_indentation(&mut self, tab_size: usize, insert_spaces: bool) {
    self.tab_size = tab_size.max(1);
    self.insert_spaces = insert_spaces;
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

  /// Columns between tab stops.
  pub fn tab_size(&self) -> usize {
    self.tab_size
  }

  /// The visual column of `offset`: grapheme clusters since the start of the
  /// line, with tabs advancing to the next tab stop.
  ///
  /// Beyond [`EXACT_COLUMN_BYTES`] into a line, columns are approximated by
  /// counting characters, so cursor work on very long lines stays O(log n).
  pub fn visual_column(&self, offset: usize) -> usize {
    let mut position = self.buffer.line_start(self.buffer.line_of(offset));
    let offset = offset.min(self.buffer.len());
    if offset - position > EXACT_COLUMN_BYTES {
      return self.buffer.char_count(position..offset);
    }
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
    if goal > EXACT_COLUMN_BYTES {
      return self.buffer.offset_after_chars(position, goal, end);
    }
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

/// Lengths of the longest common prefix and suffix of `a` and `b`, on
/// character boundaries and not overlapping.
fn common_affixes(a: &str, b: &str) -> (usize, usize) {
  let mut prefix = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
  while !a.is_char_boundary(prefix) || !b.is_char_boundary(prefix) {
    prefix -= 1;
  }
  let max_suffix = a.len().min(b.len()) - prefix;
  let mut suffix = a
    .bytes()
    .rev()
    .zip(b.bytes().rev())
    .take(max_suffix)
    .take_while(|(x, y)| x == y)
    .count();
  while !a.is_char_boundary(a.len() - suffix) || !b.is_char_boundary(b.len() - suffix) {
    suffix -= 1;
  }
  (prefix, suffix)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn columns_on_very_long_lines_count_characters() {
    let long = format!("	{}", "x".repeat(EXACT_COLUMN_BYTES * 2));
    let document = Document::new(DocumentId(1), &long, None, 0, &EditorConfig::default());
    assert_eq!(
      document.visual_column(1),
      4,
      "short distances are exact (tab stop)"
    );
    let far = EXACT_COLUMN_BYTES + 100;
    assert_eq!(document.visual_column(far), far);
    assert_eq!(document.offset_at_visual_column(0, far), far);
  }

  #[test]
  fn common_affixes_respect_character_boundaries() {
    assert_eq!(common_affixes("abcXdef", "abcYYdef"), (3, 3));
    assert_eq!(common_affixes("same", "same"), (4, 0));
    // "é" (C3 A9) and "è" (C3 A8) share a first byte that must not be split.
    assert_eq!(common_affixes("aé", "aè"), (1, 0));
    assert_eq!(common_affixes("éa", "èa"), (0, 1));
    assert_eq!(common_affixes("aa", "aaa"), (2, 0));
  }

  #[test]
  fn reload_replaces_text_keeps_cursors_and_is_clean() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("f.txt");
    std::fs::write(&path, "x").expect("write");
    let stamp = FileStamp::read(&path).expect("stat").expect("exists");

    let mut document = Document::new(
      DocumentId(1),
      "head
old middle
tail",
      Some((path, false, stamp.clone())),
      0,
      &EditorConfig::default(),
    );
    document.selections = Selections::new(vec![Selection::cursor(2), Selection::cursor(19)], 0);
    assert!(
      document
        .reload(
          "head
new
tail",
          false,
          stamp.clone()
        )
        .expect("reloads")
    );
    assert_eq!(
      document.buffer().contents(),
      "head
new
tail"
    );
    let heads: Vec<_> = document.selections().iter().map(|s| s.head).collect();
    assert_eq!(
      heads,
      vec![2, 12],
      "cursors before and after the change stay on their text"
    );
    assert!(!document.is_dirty());

    document.apply(crate::ops::EditorOp::Undo).expect("undo");
    assert_eq!(
      document.buffer().contents(),
      "head
old middle
tail"
    );
    assert!(
      document.is_dirty(),
      "undoing a reload differs from the disk contents"
    );

    assert!(
      !document
        .reload(
          "head
old middle
tail",
          false,
          stamp
        )
        .expect("reloads")
    );
    assert!(!document.is_dirty());
  }

  #[test]
  fn replacing_the_text_is_one_undoable_dirty_change() {
    let config = EditorConfig::default();
    let mut document = Document::new(DocumentId(1), "a\r\nsaved\r\n", None, 0, &config);
    assert!(document.replace_text("a\r\nrestored\n").expect("replaces"));
    assert_eq!(document.buffer().contents(), "a\r\nrestored\n");
    assert!(document.is_dirty());
    assert!(!document.replace_text("a\r\nrestored\n").expect("replaces"));

    document.apply(crate::ops::EditorOp::Undo).expect("undo");
    assert_eq!(document.buffer().contents(), "a\r\nsaved\r\n");
    assert!(!document.is_dirty());
  }

  #[test]
  fn external_edits_are_one_undo_step_with_document_line_endings() {
    let config = EditorConfig::default();
    let mut document = Document::new(DocumentId(1), "one\r\ntwo\r\n", None, 0, &config);
    document.selections = Selections::single(Selection::cursor(5));
    let edits = vec![Edit::new(0..3, "ONE\nAND"), Edit::insert(10, "three")];
    assert!(document.apply_edits(edits).expect("applies"));
    assert_eq!(document.buffer().contents(), "ONE\r\nAND\r\ntwo\r\nthree");
    assert_eq!(
      document.selections().primary().head,
      10,
      "the cursor stays on its text"
    );
    document.apply(crate::EditorOp::Undo).expect("undoes");
    assert_eq!(document.buffer().contents(), "one\r\ntwo\r\n");

    let overlapping = vec![Edit::new(0..2, "a"), Edit::new(1..3, "b")];
    assert!(document.apply_edits(overlapping).is_err());
    assert!(
      !document
        .apply_edits(vec![Edit::new(1..1, "")])
        .expect("no-op")
    );
  }
}
