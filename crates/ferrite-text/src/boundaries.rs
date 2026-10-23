//! Grapheme cluster and word boundaries.
//!
//! Cursor movement and deletion step over grapheme clusters (user-perceived
//! characters), so combining marks, emoji sequences, and CRLF move as one unit.

use std::ops::Range;

use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};

use crate::buffer::TextBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
  Word,
  Whitespace,
  LineBreak,
  Punctuation,
}

fn class_of(c: char) -> CharClass {
  if c == '\n' || c == '\r' {
    CharClass::LineBreak
  } else if c.is_whitespace() {
    CharClass::Whitespace
  } else if c.is_alphanumeric() || c == '_' {
    CharClass::Word
  } else {
    CharClass::Punctuation
  }
}

impl TextBuffer {
  /// The first grapheme boundary after `offset`, or the end of the text.
  /// Like the other boundary functions, accepts any offset: it is first
  /// clamped to the text and moved back to a character boundary.
  pub fn next_grapheme_boundary(&self, offset: usize) -> usize {
    let len = self.len();
    let offset = self.snap_to_char(offset);
    if offset >= len {
      return len;
    }
    let (mut chunk, mut chunk_start, _, _) = self.rope.chunk_at_byte(offset);
    let mut cursor = GraphemeCursor::new(offset, len, true);
    loop {
      match cursor.next_boundary(chunk, chunk_start) {
        Ok(Some(boundary)) => return boundary,
        Ok(None) => return len,
        Err(GraphemeIncomplete::NextChunk) => {
          chunk_start += chunk.len();
          chunk = self.rope.chunk_at_byte(chunk_start).0;
        }
        Err(GraphemeIncomplete::PreContext(at)) => {
          let (context, context_start, _, _) = self.rope.chunk_at_byte(at - 1);
          cursor.provide_context(context, context_start);
        }
        Err(_) => return self.next_char_boundary(offset),
      }
    }
  }

  /// The last grapheme boundary before `offset`, or zero.
  pub fn prev_grapheme_boundary(&self, offset: usize) -> usize {
    let offset = self.snap_to_char(offset);
    if offset == 0 {
      return 0;
    }
    let (mut chunk, mut chunk_start, _, _) = self.rope.chunk_at_byte(offset);
    let mut cursor = GraphemeCursor::new(offset, self.len(), true);
    loop {
      match cursor.prev_boundary(chunk, chunk_start) {
        Ok(Some(boundary)) => return boundary,
        Ok(None) => return 0,
        Err(GraphemeIncomplete::PrevChunk) => {
          let (previous, previous_start, _, _) = self.rope.chunk_at_byte(chunk_start - 1);
          chunk = previous;
          chunk_start = previous_start;
        }
        Err(GraphemeIncomplete::PreContext(at)) => {
          let (context, context_start, _, _) = self.rope.chunk_at_byte(at - 1);
          cursor.provide_context(context, context_start);
        }
        Err(_) => return self.prev_char_boundary(offset),
      }
    }
  }

  pub fn is_grapheme_boundary(&self, offset: usize) -> bool {
    if offset == 0 || offset == self.len() {
      return true;
    }
    if !self.is_char_boundary(offset) {
      return false;
    }
    let (chunk, chunk_start, _, _) = self.rope.chunk_at_byte(offset);
    let mut cursor = GraphemeCursor::new(offset, self.len(), true);
    loop {
      match cursor.is_boundary(chunk, chunk_start) {
        Ok(is_boundary) => return is_boundary,
        Err(GraphemeIncomplete::PreContext(at)) => {
          let (context, context_start, _, _) = self.rope.chunk_at_byte(at - 1);
          cursor.provide_context(context, context_start);
        }
        Err(_) => return true,
      }
    }
  }

  /// `offset` clamped to the text and moved back to a grapheme boundary.
  pub fn snap_to_grapheme(&self, offset: usize) -> usize {
    let offset = self.snap_to_char(offset);
    if self.is_grapheme_boundary(offset) {
      offset
    } else {
      self.prev_grapheme_boundary(offset)
    }
  }

  /// Where word-wise forward movement from `offset` stops: past any spaces,
  /// then past one run of word or punctuation characters. A line break is a
  /// stop of its own.
  pub fn next_word_boundary(&self, offset: usize) -> usize {
    let mut chars = self.chars_at(offset).peekable();
    let mut position = self.snap_to_char(offset);
    while let Some(&c) = chars.peek() {
      if class_of(c) != CharClass::Whitespace {
        break;
      }
      position += c.len_utf8();
      chars.next();
    }
    let Some(&first) = chars.peek() else {
      return position;
    };
    let class = class_of(first);
    if class == CharClass::LineBreak {
      return if position == self.snap_to_char(offset) {
        self.next_grapheme_boundary(position)
      } else {
        position
      };
    }
    while let Some(&c) = chars.peek() {
      if class_of(c) != class {
        break;
      }
      position += c.len_utf8();
      chars.next();
    }
    position
  }

  /// The mirror of [`Self::next_word_boundary`] for backward movement.
  pub fn prev_word_boundary(&self, offset: usize) -> usize {
    let start = self.snap_to_char(offset);
    let mut chars = self.chars_at(start);
    let mut position = start;
    let mut previous = chars.prev();
    while let Some(c) = previous {
      if class_of(c) != CharClass::Whitespace {
        break;
      }
      position -= c.len_utf8();
      previous = chars.prev();
    }
    let Some(first) = previous else {
      return position;
    };
    let class = class_of(first);
    if class == CharClass::LineBreak {
      return if position == start {
        self.prev_grapheme_boundary(position)
      } else {
        position
      };
    }
    while let Some(c) = previous {
      if class_of(c) != class {
        break;
      }
      position -= c.len_utf8();
      previous = chars.prev();
    }
    position
  }

  /// The run of same-class characters around `offset` (for double-click
  /// selection). Prefers the character after `offset`, falling back to the one
  /// before at the end of a line. Line breaks are never included.
  pub fn word_at(&self, offset: usize) -> Range<usize> {
    let offset = self.snap_to_char(offset);
    let after = self
      .chars_at(offset)
      .next()
      .filter(|&c| class_of(c) != CharClass::LineBreak);
    let before = self
      .chars_at(offset)
      .prev()
      .filter(|&c| class_of(c) != CharClass::LineBreak);
    let Some(class) = after.or(before).map(class_of) else {
      return offset..offset;
    };

    let mut start = offset;
    let mut backward = self.chars_at(offset);
    while let Some(c) = backward.prev() {
      if class_of(c) != class {
        break;
      }
      start -= c.len_utf8();
    }
    let mut end = offset;
    for c in self.chars_at(offset) {
      if class_of(c) != class {
        break;
      }
      end += c.len_utf8();
    }
    start..end
  }

  fn snap_to_char(&self, offset: usize) -> usize {
    let offset = offset.min(self.len());
    self.rope.char_to_byte(self.rope.byte_to_char(offset))
  }

  fn next_char_boundary(&self, offset: usize) -> usize {
    let next = self.rope.byte_to_char(offset) + 1;
    self.rope.char_to_byte(next.min(self.rope.len_chars()))
  }

  fn prev_char_boundary(&self, offset: usize) -> usize {
    let current = self.rope.byte_to_char(offset);
    let start = self.rope.char_to_byte(current);
    if start < offset {
      start
    } else {
      self.rope.char_to_byte(current.saturating_sub(1))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn graphemes_keep_combining_marks_and_emoji_together() {
    // "e" + combining acute, a family emoji (ZWJ sequence), then "x".
    let text = "e\u{301}👨‍👩‍👧x";
    let buffer = TextBuffer::from_text(text);
    let family_start = "e\u{301}".len();
    let family_end = text.len() - 1;
    assert_eq!(buffer.next_grapheme_boundary(0), family_start);
    assert_eq!(buffer.next_grapheme_boundary(family_start), family_end);
    assert_eq!(buffer.prev_grapheme_boundary(family_end), family_start);
    assert_eq!(buffer.prev_grapheme_boundary(family_start), 0);
    assert!(!buffer.is_grapheme_boundary(1));
    assert_eq!(buffer.snap_to_grapheme(family_start + 4), family_start);
    // Offsets inside a multi-byte character are tolerated.
    assert_eq!(buffer.snap_to_grapheme(2), 0);
    assert_eq!(buffer.prev_grapheme_boundary(2), 0);
    assert_eq!(buffer.next_grapheme_boundary(2), family_start);
  }

  #[test]
  fn crlf_is_a_single_grapheme() {
    let buffer = TextBuffer::from_text("a\r\nb");
    assert_eq!(buffer.next_grapheme_boundary(1), 3);
    assert_eq!(buffer.prev_grapheme_boundary(3), 1);
    assert!(!buffer.is_grapheme_boundary(2));
  }

  #[test]
  fn grapheme_steps_clamp_at_the_ends() {
    let buffer = TextBuffer::from_text("ab");
    assert_eq!(buffer.prev_grapheme_boundary(0), 0);
    assert_eq!(buffer.next_grapheme_boundary(2), 2);
    assert_eq!(TextBuffer::new().next_grapheme_boundary(0), 0);
  }

  #[test]
  fn graphemes_work_across_rope_chunks() {
    let text = format!("{}e\u{301}{}", "a".repeat(5000), "b".repeat(5000));
    let buffer = TextBuffer::from_text(&text);
    let mut offset = 0;
    let mut steps = 0;
    while offset < buffer.len() {
      offset = buffer.next_grapheme_boundary(offset);
      steps += 1;
    }
    assert_eq!(steps, 10_001);
  }

  #[test]
  fn word_movement_skips_spaces_then_a_run() {
    let buffer = TextBuffer::from_text("let foo_bar = a.b;");
    assert_eq!(buffer.next_word_boundary(0), 3);
    assert_eq!(buffer.next_word_boundary(3), 11);
    assert_eq!(buffer.next_word_boundary(11), 13);
    assert_eq!(buffer.prev_word_boundary(11), 4);
    assert_eq!(buffer.prev_word_boundary(4), 0);
    assert_eq!(buffer.prev_word_boundary(18), 17);
  }

  #[test]
  fn word_movement_stops_at_line_breaks() {
    let buffer = TextBuffer::from_text("ab  \r\ncd");
    assert_eq!(buffer.next_word_boundary(2), 4);
    assert_eq!(buffer.next_word_boundary(4), 6);
    assert_eq!(buffer.prev_word_boundary(6), 4);
    assert_eq!(buffer.prev_word_boundary(4), 0);
  }

  #[test]
  fn word_at_selects_the_surrounding_run() {
    let buffer = TextBuffer::from_text("call(foo_bar)\nx");
    assert_eq!(buffer.word_at(7), 5..12);
    assert_eq!(buffer.word_at(5), 5..12);
    assert_eq!(buffer.word_at(4), 4..5);
    // At the end of a line, the word before the cursor.
    assert_eq!(buffer.word_at(13), 12..13);
    assert_eq!(TextBuffer::from_text("\n").word_at(0), 0..0);
  }
}
