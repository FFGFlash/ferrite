use std::ops::Range;

use ropey::Rope;
use ropey::iter::Chars;

use crate::change::{ChangeSet, Edit, EditError};
use crate::line_ending::LineEnding;
use crate::lines::{ByteEdit, LineLog, LineSplice, TextPoint};

/// Content version of a buffer. Every applied change produces a new, larger
/// version; versions are never reused, so they identify stale requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Version(u64);

impl Version {
  pub const INITIAL: Self = Self(0);

  pub fn get(self) -> u64 {
    self.0
  }

  /// A version received back from a client. Only meaningful for comparing
  /// with versions of the same buffer.
  pub fn from_raw(raw: u64) -> Self {
    Self(raw)
  }

  pub(crate) fn next(self) -> Self {
    Self(self.0 + 1)
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
  line_log: LineLog,
}

impl TextBuffer {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn from_text(text: &str) -> Self {
    Self {
      rope: Rope::from_str(text),
      version: Version::INITIAL,
      line_log: LineLog::default(),
    }
  }

  /// A buffer holding `snapshot`'s text, at its version but with no history
  /// of earlier edits. Cheap: the text is shared, not copied.
  pub fn from_snapshot(snapshot: &TextSnapshot) -> Self {
    Self {
      rope: snapshot.rope.clone(),
      version: snapshot.version,
      line_log: LineLog::default(),
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
    let old_lines: Vec<(usize, usize)> = changes
      .edits()
      .iter()
      .map(|edit| {
        (
          self.rope.byte_to_line(edit.range.start),
          self.rope.byte_to_line(edit.range.end),
        )
      })
      .collect();
    // Applied last to first, each edit's coordinates are still the original ones.
    let byte_edits: Vec<ByteEdit> = changes
      .edits()
      .iter()
      .rev()
      .filter(|edit| !edit.is_noop())
      .map(|edit| {
        let start_point = self.point(edit.range.start);
        let (breaks, after_last_break) = line_breaks(&edit.text);
        let new_end_point = match after_last_break {
          Some(column) => TextPoint {
            row: start_point.row + breaks,
            column,
          },
          None => TextPoint {
            row: start_point.row,
            column: start_point.column + edit.text.len(),
          },
        };
        ByteEdit {
          start: edit.range.start,
          old_end: edit.range.end,
          new_end: edit.range.start + edit.text.len(),
          start_point,
          old_end_point: self.point(edit.range.end),
          new_end_point,
          text: edit.text.clone(),
        }
      })
      .collect();
    let inverse = changes
      .edits()
      .iter()
      .zip(&post_ranges)
      .map(|(edit, post)| Edit::new(post.clone(), self.text_in(edit.range.clone())))
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
    self.version = self.version.next();
    let splices = self.line_splices(&old_lines, &post_ranges);
    self.line_log.push(self.version, splices, byte_edits);
    Ok(ChangeSet::from_sorted(inverse))
  }

  /// One splice per group of edits touching the same lines, in order.
  fn line_splices(
    &self,
    old_lines: &[(usize, usize)],
    post_ranges: &[Range<usize>],
  ) -> Vec<LineSplice> {
    // (old start, old end, new start, new end), inclusive line numbers.
    let mut groups: Vec<(usize, usize, usize, usize)> = Vec::new();
    for (&(old_start, old_end), post) in old_lines.iter().zip(post_ranges) {
      let new_start = self.rope.byte_to_line(post.start);
      let new_end = self.rope.byte_to_line(post.end);
      match groups.last_mut() {
        Some(last) if old_start <= last.1 => {
          last.1 = last.1.max(old_end);
          last.3 = last.3.max(new_end);
        }
        _ => groups.push((old_start, old_end, new_start, new_end)),
      }
    }
    groups
      .into_iter()
      .map(|(old_start, old_end, new_start, new_end)| LineSplice {
        start: new_start,
        removed: old_end - old_start + 1,
        inserted: new_end - new_start + 1,
      })
      .collect()
  }

  /// The byte edits between `since` and now, in application order. `None`
  /// if `since` is too old to reconstruct.
  pub fn edits_since(&self, since: Version) -> Option<Vec<ByteEdit>> {
    self.line_log.edits_since(since, self.version)
  }

  /// The line (counted like the rest of the buffer: LF, CRLF, or CR) and
  /// byte column of `offset`.
  pub fn point(&self, offset: usize) -> TextPoint {
    let offset = offset.min(self.len());
    let line = self.rope.byte_to_line(offset);
    TextPoint {
      row: line,
      column: offset - self.rope.line_to_byte(line),
    }
  }

  /// The text from `offset` to the end of its rope chunk (for parsers that
  /// read the text piece by piece). Empty at the end of the text.
  pub fn chunk_from(&self, offset: usize) -> &str {
    if offset >= self.len() {
      return "";
    }
    let (chunk, chunk_start, _, _) = self.rope.chunk_at_byte(offset);
    &chunk[offset - chunk_start..]
  }

  /// The text of `range` as a sequence of string pieces, without copying.
  pub fn chunks_in(&self, range: Range<usize>) -> impl Iterator<Item = &str> {
    self.rope.byte_slice(range).chunks()
  }

  /// How lines moved between `since` and now, for clients holding lines
  /// from that version. `None` if `since` is too old to reconstruct.
  pub fn line_changes_since(&self, since: Version) -> Option<Vec<LineSplice>> {
    self.line_log.since(since, self.version)
  }

  /// The start of `line`'s text, at most `max_bytes` long (cut at a character
  /// boundary), and whether it was cut.
  pub fn line_prefix(&self, line: usize, max_bytes: usize) -> (String, bool) {
    let start = self.line_start(line);
    let end = self.line_end(line);
    if end - start <= max_bytes {
      return (self.text_in(start..end), false);
    }
    let cut = self
      .rope
      .char_to_byte(self.rope.byte_to_char(start + max_bytes));
    (self.text_in(start..cut), true)
  }

  /// Length of `line` (without its line break) in UTF-16 code units.
  pub fn line_utf16_len(&self, line: usize) -> usize {
    let start = self.rope.byte_to_char(self.line_start(line));
    let end = self.rope.byte_to_char(self.line_end(line));
    self.rope.char_to_utf16_cu(end) - self.rope.char_to_utf16_cu(start)
  }

  /// Number of characters (Unicode scalar values) in `range`.
  pub fn char_count(&self, range: Range<usize>) -> usize {
    let len = self.len();
    self.rope.byte_to_char(range.end.min(len)) - self.rope.byte_to_char(range.start.min(len))
  }

  /// The offset `chars` characters after `offset`, clamped to `limit`.
  pub fn offset_after_chars(&self, offset: usize, chars: usize, limit: usize) -> usize {
    let start = self.rope.byte_to_char(offset.min(self.len()));
    let target = (start + chars).min(self.rope.len_chars());
    self.rope.char_to_byte(target).min(limit)
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
  pub(crate) rope: Rope,
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

  /// The text from `offset` to the end of its rope chunk; see
  /// [`TextBuffer::chunk_from`].
  pub fn chunk_from(&self, offset: usize) -> &str {
    if offset >= self.len() {
      return "";
    }
    let (chunk, chunk_start, _, _) = self.rope.chunk_at_byte(offset);
    &chunk[offset - chunk_start..]
  }

  /// The text of `range` as a sequence of string pieces, without copying.
  pub fn chunks_in(&self, range: Range<usize>) -> impl Iterator<Item = &str> {
    self.rope.byte_slice(range).chunks()
  }

  /// The whole text as one string.
  pub fn text(&self) -> String {
    self.rope.to_string()
  }
}

/// The number of line breaks (LF, CRLF, or CR) in `text`, and the byte
/// length after the last one, if any.
fn line_breaks(text: &str) -> (usize, Option<usize>) {
  let bytes = text.as_bytes();
  let (mut breaks, mut last_end) = (0, None);
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'\n' => {
        breaks += 1;
        last_end = Some(i + 1);
      }
      b'\r' => {
        breaks += 1;
        if bytes.get(i + 1) == Some(&b'\n') {
          i += 1;
        }
        last_end = Some(i + 1);
      }
      _ => {}
    }
    i += 1;
  }
  (breaks, last_end.map(|end| bytes.len() - end))
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
  fn changes_record_how_lines_moved() {
    let mut buffer = TextBuffer::from_text("a\nb\nc\nd\n");
    let start = buffer.version();
    // Split line 1 ("b") into two lines.
    apply(&mut buffer, vec![Edit::insert(3, "\nx")]);
    // Join lines 3 and 4 ("c" and "d") and edit line 0, in one change.
    apply(&mut buffer, vec![Edit::new(0..1, "A"), Edit::delete(7..8)]);
    assert_eq!(buffer.contents(), "A\nb\nx\ncd\n");
    let splice = |start, removed, inserted| LineSplice {
      start,
      removed,
      inserted,
    };
    assert_eq!(
      buffer.line_changes_since(start),
      Some(vec![splice(1, 1, 2), splice(0, 1, 1), splice(3, 2, 1)])
    );
    assert_eq!(buffer.line_changes_since(buffer.version()), Some(vec![]));
  }

  #[test]
  fn changes_record_byte_edits_in_application_order() {
    let mut buffer = TextBuffer::from_text("ab\ncd\n");
    let start = buffer.version();
    apply(
      &mut buffer,
      vec![Edit::new(0..1, "X"), Edit::insert(4, "1\n22")],
    );
    assert_eq!(buffer.contents(), "Xb\nc1\n22d\n");
    let edits = buffer.edits_since(start).expect("recorded");
    // The later edit comes first, so each applies in the original coordinates.
    assert_eq!(edits.len(), 2);
    assert_eq!(
      (edits[0].start, edits[0].old_end, edits[0].new_end),
      (4, 4, 8)
    );
    assert_eq!(edits[0].start_point, TextPoint { row: 1, column: 1 });
    assert_eq!(edits[0].new_end_point, TextPoint { row: 2, column: 2 });
    assert_eq!(
      (edits[1].start, edits[1].old_end, edits[1].new_end),
      (0, 1, 1)
    );
    assert_eq!(crate::lines::map_offset_through(&edits, 5, false), 9);
  }

  #[test]
  fn points_count_every_kind_of_line_break() {
    let buffer = TextBuffer::from_text("a\rb\nc\r\nd");
    assert_eq!(buffer.point(2), TextPoint { row: 1, column: 0 });
    assert_eq!(buffer.point(5), TextPoint { row: 2, column: 1 });
    assert_eq!(buffer.point(7), TextPoint { row: 3, column: 0 });
    assert_eq!(line_breaks("x\r\ny\rzz"), (2, Some(2)));
    assert_eq!(line_breaks("plain"), (0, None));
  }

  #[test]
  fn chunks_can_be_read_piece_by_piece() {
    let text = "x".repeat(10_000);
    let buffer = TextBuffer::from_text(&text);
    let mut read = String::new();
    while read.len() < buffer.len() {
      read.push_str(buffer.chunk_from(read.len()));
    }
    assert_eq!(read, text);
    assert_eq!(buffer.chunk_from(buffer.len()), "");
    assert_eq!(buffer.chunks_in(2..5).collect::<String>(), "xxx");
  }

  #[test]
  fn edits_on_the_same_line_merge_into_one_splice() {
    let mut buffer = TextBuffer::from_text("abc\ndef");
    let start = buffer.version();
    apply(
      &mut buffer,
      vec![Edit::insert(0, "1"), Edit::insert(2, "2")],
    );
    assert_eq!(
      buffer.line_changes_since(start),
      Some(vec![LineSplice {
        start: 0,
        removed: 1,
        inserted: 1
      }])
    );
  }

  #[test]
  fn long_lines_can_be_read_as_prefixes() {
    let buffer = TextBuffer::from_text("héllo world\nshort");
    assert_eq!(buffer.line_prefix(0, 3), ("hé".to_owned(), true));
    assert_eq!(buffer.line_prefix(1, 10), ("short".to_owned(), false));
    assert_eq!(buffer.line_utf16_len(0), 11);
    assert_eq!(buffer.char_count(0..6), 5);
    assert_eq!(buffer.offset_after_chars(0, 2, 100), 3);
    assert_eq!(buffer.offset_after_chars(0, 50, 7), 7);
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
