use std::ops::Range;

use ropey::Rope;
use ropey::iter::Chars;

use crate::change::{ChangeSet, Edit, EditError};
use crate::line_ending::LineEnding;

/// Content version of a buffer. Every applied change produces a new, larger
/// version; versions are never reused, so they identify stale requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Version(u64);

impl Version {
  pub const INITIAL: Self = Self(0);

  pub fn get(self) -> u64 {
    self.0
  }
}

/// A position whose column counts UTF-16 code units from the start of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Utf16Position {
  pub line: usize,
  pub column: usize,
}

/// Rope-backed, versioned text. Edits cost O(log n) regardless of size.
#[derive(Debug, Clone, Default)]
pub struct TextBuffer {
  pub(crate) rope: Rope,
  version: Version,
}

impl TextBuffer {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn from_text(text: &str) -> Self {
    Self {
      rope: Rope::from_str(text),
      version: Version::INITIAL,
    }
  }

  pub fn version(&self) -> Version {
    self.version
  }

  /// Length in bytes.
  pub fn len(&self) -> usize {
    self.rope.len_bytes()
  }

  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  /// Number of lines. Text ending in a line break has a final empty line,
  /// and empty text has one line.
  pub fn line_count(&self) -> usize {
    self.rope.len_lines()
  }

  /// The line containing `offset` (clamped to the text).
  pub fn line_of(&self, offset: usize) -> usize {
    self.rope.byte_to_line(offset.min(self.len()))
  }

  /// Offset of the first byte of `line` (clamped to the last line).
  pub fn line_start(&self, line: usize) -> usize {
    self.rope.line_to_byte(line.min(self.line_count() - 1))
  }

  /// Offset just before the line break that ends `line` (clamped to the last line).
  pub fn line_end(&self, line: usize) -> usize {
    let line = line.min(self.line_count() - 1);
    let slice = self.rope.line(line);
    let len = slice.len_bytes();
    let break_len = match (
      len.checked_sub(2).map(|i| slice.byte(i)),
      len.checked_sub(1).map(|i| slice.byte(i)),
    ) {
      (Some(b'\r'), Some(b'\n')) => 2,
      (_, Some(b'\n' | b'\r')) => 1,
      _ => 0,
    };
    self.rope.line_to_byte(line) + len - break_len
  }

  /// Text of `line` without its line break.
  pub fn line_text(&self, line: usize) -> String {
    self.text_in(self.line_start(line)..self.line_end(line))
  }

  /// Copies the text in `range`. Panics if the range is out of bounds or
  /// splits a character; callers pass ranges derived from this buffer.
  pub fn text_in(&self, range: Range<usize>) -> String {
    self.rope.byte_slice(range).to_string()
  }

  pub fn contents(&self) -> String {
    self.rope.to_string()
  }

  pub fn is_char_boundary(&self, offset: usize) -> bool {
    offset <= self.len() && self.rope.char_to_byte(self.rope.byte_to_char(offset)) == offset
  }

  /// Characters starting at `offset`; iterate backwards with `prev`.
  pub fn chars_at(&self, offset: usize) -> Chars<'_> {
    self
      .rope
      .chars_at(self.rope.byte_to_char(offset.min(self.len())))
  }

  /// The line ending used by most lines, if the text has any line breaks.
  pub fn detect_line_ending(&self) -> Option<LineEnding> {
    LineEnding::detect(self.rope.bytes())
  }

  /// An immutable copy of the current text. Cheap: ropes share structure.
  pub fn snapshot(&self) -> TextSnapshot {
    TextSnapshot {
      rope: self.rope.clone(),
      version: self.version,
    }
  }

  /// Applies `changes` atomically and returns the change set that reverts it.
  /// On error the buffer is unchanged. Empty change sets do not bump the version.
  pub fn apply(&mut self, changes: &ChangeSet) -> Result<ChangeSet, EditError> {
    let len = self.len();
    for edit in changes.edits() {
      for offset in [edit.range.start, edit.range.end] {
        if offset > len {
          return Err(EditError::OutOfBounds { offset, len });
        }
        if !self.is_char_boundary(offset) {
          return Err(EditError::NotCharBoundary { offset });
        }
      }
    }
    if changes.is_empty() {
      return Ok(ChangeSet::default());
    }

    let post_ranges = changes.post_ranges();
    let inverse = changes
      .edits()
      .iter()
      .zip(post_ranges)
      .map(|(edit, post)| Edit::new(post, self.text_in(edit.range.clone())))
      .collect();

    for edit in changes.edits().iter().rev() {
      let start = self.rope.byte_to_char(edit.range.start);
      let end = self.rope.byte_to_char(edit.range.end);
      if start < end {
        self.rope.remove(start..end);
      }
      if !edit.text.is_empty() {
        self.rope.insert(start, &edit.text);
      }
    }
    self.version = Version(self.version.0 + 1);
    Ok(ChangeSet::from_sorted(inverse))
  }

  pub fn offset_to_utf16(&self, offset: usize) -> Utf16Position {
    let offset = offset.min(self.len());
    let line = self.rope.byte_to_line(offset);
    let line_start = self.rope.char_to_utf16_cu(self.rope.line_to_char(line));
    let column = self.rope.char_to_utf16_cu(self.rope.byte_to_char(offset)) - line_start;
    Utf16Position { line, column }
  }

  /// Converts a UTF-16 position to a byte offset. Lines past the end clamp to
  /// the end of the text, columns past the line end clamp to the line end, and
  /// a column inside a surrogate pair moves to the start of that character.
  pub fn utf16_to_offset(&self, position: Utf16Position) -> usize {
    if position.line >= self.line_count() {
      return self.len();
    }
    let line_start_char = self.rope.line_to_char(position.line);
    let line_end_char = self.rope.byte_to_char(self.line_end(position.line));
    let base = self.rope.char_to_utf16_cu(line_start_char);
    let max_column = self.rope.char_to_utf16_cu(line_end_char) - base;
    let char_index = self
      .rope
      .utf16_cu_to_char(base + position.column.min(max_column));
    self.rope.char_to_byte(char_index)
  }
}

/// A read-only view of a buffer at one version, safe to send to another thread
/// (for example, to write it to disk while editing continues).
#[derive(Debug, Clone)]
pub struct TextSnapshot {
  rope: Rope,
  version: Version,
}

impl TextSnapshot {
  pub fn version(&self) -> Version {
    self.version
  }

  pub fn len(&self) -> usize {
    self.rope.len_bytes()
  }

  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  /// The text as a sequence of string chunks, in order.
  pub fn chunks(&self) -> impl Iterator<Item = &str> {
    self.rope.chunks()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn apply(buffer: &mut TextBuffer, edits: Vec<Edit>) -> ChangeSet {
    let changes = ChangeSet::new(edits).expect("valid change set");
    buffer.apply(&changes).expect("edit applies")
  }

  #[test]
  fn counts_lines_for_each_line_ending() {
    assert_eq!(TextBuffer::from_text("").line_count(), 1);
    assert_eq!(TextBuffer::from_text("a\nb\r\nc\rd").line_count(), 4);
    assert_eq!(TextBuffer::from_text("a\n").line_count(), 2);
    // Unicode separators are not line breaks.
    assert_eq!(TextBuffer::from_text("a\u{2028}b\u{85}c").line_count(), 1);
  }

  #[test]
  fn line_bounds_exclude_line_breaks() {
    let buffer = TextBuffer::from_text("ab\r\ncd\nef");
    assert_eq!((buffer.line_start(1), buffer.line_end(1)), (4, 6));
    assert_eq!(buffer.line_end(0), 2);
    assert_eq!(buffer.line_text(2), "ef");
    assert_eq!(buffer.line_of(5), 1);
    assert_eq!(buffer.line_of(100), 2);
  }

  #[test]
  fn insert_and_delete_update_text_and_version() {
    let mut buffer = TextBuffer::from_text("hello world");
    apply(&mut buffer, vec![Edit::new(0..5, "goodbye")]);
    assert_eq!(buffer.contents(), "goodbye world");
    apply(&mut buffer, vec![Edit::delete(7..13)]);
    assert_eq!(buffer.contents(), "goodbye");
    assert_eq!(buffer.version().get(), 2);
  }

  #[test]
  fn multiple_edits_use_pre_edit_coordinates() {
    let mut buffer = TextBuffer::from_text("a b c");
    apply(
      &mut buffer,
      vec![
        Edit::new(0..1, "AA"),
        Edit::new(2..3, ""),
        Edit::insert(5, "!"),
      ],
    );
    assert_eq!(buffer.contents(), "AA  c!");
  }

  #[test]
  fn inverse_restores_original_text() {
    let original = "fn main() {\n  é🦀\n}\n";
    let mut buffer = TextBuffer::from_text(original);
    let inverse = apply(
      &mut buffer,
      vec![
        Edit::new(3..7, "start"),
        Edit::insert(14, "x"),
        Edit::delete(20..21),
      ],
    );
    assert_ne!(buffer.contents(), original);
    buffer.apply(&inverse).expect("inverse applies");
    assert_eq!(buffer.contents(), original);
  }

  #[test]
  fn invalid_edits_are_rejected_without_changes() {
    let mut buffer = TextBuffer::from_text("é");
    let split = ChangeSet::single(Edit::insert(1, "x")).expect("valid");
    assert_eq!(
      buffer.apply(&split),
      Err(EditError::NotCharBoundary { offset: 1 })
    );
    let past_end = ChangeSet::single(Edit::delete(0..9)).expect("valid");
    assert_eq!(
      buffer.apply(&past_end),
      Err(EditError::OutOfBounds { offset: 9, len: 2 })
    );
    assert_eq!(buffer.contents(), "é");
    assert_eq!(buffer.version(), Version::INITIAL);
  }

  #[test]
  fn empty_change_sets_keep_the_version() {
    let mut buffer = TextBuffer::from_text("abc");
    apply(&mut buffer, vec![Edit::insert(1, "")]);
    assert_eq!(buffer.version(), Version::INITIAL);
  }

  #[test]
  fn converts_offsets_to_utf16_positions() {
    // "é" is 2 UTF-8 bytes / 1 UTF-16 unit; "🦀" is 4 bytes / 2 units.
    let buffer = TextBuffer::from_text("aé🦀b\r\nx");
    let at = |offset| buffer.offset_to_utf16(offset);
    assert_eq!(at(0), Utf16Position { line: 0, column: 0 });
    assert_eq!(at(3), Utf16Position { line: 0, column: 2 });
    assert_eq!(at(7), Utf16Position { line: 0, column: 4 });
    assert_eq!(at(8), Utf16Position { line: 0, column: 5 });
    assert_eq!(at(10), Utf16Position { line: 1, column: 0 });
    assert_eq!(at(11), Utf16Position { line: 1, column: 1 });
  }

  #[test]
  fn converts_utf16_positions_to_offsets_with_clamping() {
    let buffer = TextBuffer::from_text("aé🦀b\r\nx");
    let at = |line, column| buffer.utf16_to_offset(Utf16Position { line, column });
    assert_eq!(at(0, 2), 3);
    assert_eq!(at(0, 4), 7);
    // Inside the surrogate pair: start of the crab.
    assert_eq!(at(0, 3), 3);
    // Past the end of the line: before the CRLF.
    assert_eq!(at(0, 50), 8);
    assert_eq!(at(1, 1), 11);
    assert_eq!(at(9, 0), 11);
  }

  #[test]
  fn utf16_round_trips_on_character_boundaries() {
    let buffer = TextBuffer::from_text("x\u{301}y🦀\nzé");
    for offset in 0..=buffer.len() {
      if buffer.is_char_boundary(offset) {
        assert_eq!(
          buffer.utf16_to_offset(buffer.offset_to_utf16(offset)),
          offset
        );
      }
    }
  }

  #[test]
  fn snapshots_are_unaffected_by_later_edits() {
    let mut buffer = TextBuffer::from_text("before");
    let snapshot = buffer.snapshot();
    apply(&mut buffer, vec![Edit::new(0..6, "after")]);
    assert_eq!(snapshot.chunks().collect::<String>(), "before");
    assert_eq!(snapshot.version(), Version::INITIAL);
  }
}
