//! Conversions between Ferrite's byte offsets and LSP positions, and the
//! document changes sent to servers.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ferrite_text::{ByteEdit, ChangeSet, Edit, TextBuffer, TextSnapshot, Utf16Position};
use lsp_types::{Position, TextDocumentContentChangeEvent, Uri};

/// How a server counts columns, as negotiated at initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionEncoding {
  /// Bytes: Ferrite's native unit, so no conversion is needed.
  Utf8,
  /// UTF-16 code units: the LSP default.
  Utf16,
}

impl PositionEncoding {
  pub fn label(self) -> &'static str {
    match self {
      Self::Utf8 => "utf-8",
      Self::Utf16 => "utf-16",
    }
  }
}

fn to_u32(value: usize) -> u32 {
  u32::try_from(value).unwrap_or(u32::MAX)
}

/// The LSP position of `offset` in `buffer`.
pub fn to_position(buffer: &TextBuffer, offset: usize, encoding: PositionEncoding) -> Position {
  match encoding {
    PositionEncoding::Utf8 => {
      let point = buffer.point(offset);
      Position::new(to_u32(point.row), to_u32(point.column))
    }
    PositionEncoding::Utf16 => {
      let position = buffer.offset_to_utf16(offset);
      Position::new(to_u32(position.line), to_u32(position.column))
    }
  }
}

/// The offset of an LSP position in `buffer`, clamped to the text.
pub fn to_offset(buffer: &TextBuffer, position: Position, encoding: PositionEncoding) -> usize {
  let line = position.line as usize;
  if line >= buffer.line_count() {
    return buffer.len();
  }
  match encoding {
    PositionEncoding::Utf8 => {
      let start = buffer.line_start(line);
      let mut offset = (start + position.character as usize).min(buffer.line_end(line));
      while !buffer.is_char_boundary(offset) {
        offset -= 1;
      }
      offset
    }
    PositionEncoding::Utf16 => buffer.utf16_to_offset(Utf16Position {
      line,
      column: position.character as usize,
    }),
  }
}

pub fn to_range(
  buffer: &TextBuffer,
  range: Range<usize>,
  encoding: PositionEncoding,
) -> lsp_types::Range {
  lsp_types::Range::new(
    to_position(buffer, range.start, encoding),
    to_position(buffer, range.end, encoding),
  )
}

pub fn to_offsets(
  buffer: &TextBuffer,
  range: lsp_types::Range,
  encoding: PositionEncoding,
) -> Range<usize> {
  let start = to_offset(buffer, range.start, encoding);
  start..to_offset(buffer, range.end, encoding).max(start)
}

/// Incremental changes taking a server from `synced` to `synced` plus
/// `edits` (in application order, as from `TextBuffer::edits_since`).
pub fn content_changes(
  synced: &TextSnapshot,
  edits: &[ByteEdit],
  encoding: PositionEncoding,
) -> Vec<TextDocumentContentChangeEvent> {
  let change = |range, text: &str| TextDocumentContentChangeEvent {
    range: Some(range),
    range_length: None,
    text: text.to_owned(),
  };
  match encoding {
    // Each edit's points are already in the text it applies to.
    PositionEncoding::Utf8 => edits
      .iter()
      .map(|edit| {
        let range = lsp_types::Range::new(
          Position::new(
            to_u32(edit.start_point.row),
            to_u32(edit.start_point.column),
          ),
          Position::new(
            to_u32(edit.old_end_point.row),
            to_u32(edit.old_end_point.column),
          ),
        );
        change(range, &edit.text)
      })
      .collect(),
    // UTF-16 columns depend on the text before each edit, so replay them.
    PositionEncoding::Utf16 => {
      let mut scratch = TextBuffer::from_snapshot(synced);
      let mut changes = Vec::with_capacity(edits.len());
      for edit in edits {
        let range = to_range(&scratch, edit.start..edit.old_end, encoding);
        changes.push(change(range, &edit.text));
        let applied = ChangeSet::single(Edit::new(edit.start..edit.old_end, edit.text.as_str()))
          .and_then(|changes| scratch.apply(&changes).map(|_| ()));
        if applied.is_err() {
          tracing::error!("edit log does not match the synced text");
          break;
        }
      }
      changes
    }
  }
}

/// A change replacing the whole document.
pub fn full_change(text: String) -> TextDocumentContentChangeEvent {
  TextDocumentContentChangeEvent {
    range: None,
    range_length: None,
    text,
  }
}

fn is_unreserved(byte: u8) -> bool {
  byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':')
}

/// The `file:` URI of an absolute path.
pub fn path_to_uri(path: &Path) -> Option<Uri> {
  let path = path.to_str()?;
  let path = path
    .strip_prefix(r"\\?\")
    .unwrap_or(path)
    .replace('\\', "/");
  let mut encoded = String::with_capacity(path.len() + 8);
  if !path.starts_with('/') {
    encoded.push('/');
  }
  for byte in path.bytes() {
    if is_unreserved(byte) {
      encoded.push(char::from(byte));
    } else {
      encoded.push_str(&format!("%{byte:02X}"));
    }
  }
  Uri::from_str(&format!("file://{encoded}")).ok()
}

/// The path of a `file:` URI.
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
  let rest = uri.as_str().strip_prefix("file://")?;
  // Skip an authority such as `localhost`.
  let path = &rest[rest.find('/')?..];
  let bytes = path.as_bytes();
  let mut decoded = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%'
      && let Some(byte) = path
        .get(i + 1..i + 3)
        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
    {
      decoded.push(byte);
      i += 3;
    } else {
      decoded.push(bytes[i]);
      i += 1;
    }
  }
  let decoded = String::from_utf8(decoded).ok()?;
  // `/C:/x` is a Windows drive path.
  let drive = decoded.as_bytes();
  if drive.len() >= 3 && drive[0] == b'/' && drive[1].is_ascii_alphabetic() && drive[2] == b':' {
    return Some(PathBuf::from(decoded[1..].replace('/', "\\")));
  }
  Some(PathBuf::from(decoded))
}

#[cfg(test)]
mod tests {
  use ferrite_text::Version;

  use super::*;

  fn edit(buffer: &mut TextBuffer, range: Range<usize>, text: &str) {
    buffer
      .apply(&ChangeSet::single(Edit::new(range, text)).expect("valid"))
      .expect("applies");
  }

  /// Applies LSP changes the way a server would, counting in `encoding`.
  fn apply_as_server(
    text: &str,
    changes: &[TextDocumentContentChangeEvent],
    encoding: PositionEncoding,
  ) -> String {
    let mut buffer = TextBuffer::from_text(text);
    for change in changes {
      let range = change.range.expect("incremental");
      let range = to_offsets(&buffer, range, encoding);
      edit(&mut buffer, range, &change.text);
    }
    buffer.contents()
  }

  #[test]
  fn positions_round_trip_in_both_encodings() {
    let buffer = TextBuffer::from_text("a🦀b\nxé\n");
    for encoding in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
      for offset in [0, 1, 5, 6, 7, 8, 10, 11] {
        let position = to_position(&buffer, offset, encoding);
        assert_eq!(
          to_offset(&buffer, position, encoding),
          offset,
          "{encoding:?}"
        );
      }
    }
    assert_eq!(
      to_position(&buffer, 5, PositionEncoding::Utf8),
      Position::new(0, 5)
    );
    assert_eq!(
      to_position(&buffer, 5, PositionEncoding::Utf16),
      Position::new(0, 3)
    );
    // Out of range positions clamp.
    assert_eq!(
      to_offset(&buffer, Position::new(0, 99), PositionEncoding::Utf8),
      6
    );
    assert_eq!(
      to_offset(&buffer, Position::new(9, 0), PositionEncoding::Utf16),
      buffer.len()
    );
    // Inside a character snaps back to its start.
    assert_eq!(
      to_offset(&buffer, Position::new(0, 3), PositionEncoding::Utf8),
      1
    );
  }

  #[test]
  fn content_changes_reproduce_the_edits() {
    let original = "fn 🦀() {}\nlet é = 1;\n";
    let mut buffer = TextBuffer::from_text(original);
    let synced = buffer.snapshot();
    edit(&mut buffer, 13..13, "x");
    // A multi-cursor style change: several edits at once on one line.
    buffer
      .apply(
        &ChangeSet::new(vec![
          Edit::insert(0, "pub "),
          Edit::insert(3, "a"),
          Edit::new(7..8, "_b"),
        ])
        .expect("valid"),
      )
      .expect("applies");
    edit(&mut buffer, 0..0, "\n");
    let edits = buffer.edits_since(Version::INITIAL).expect("edits");
    for encoding in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
      let changes = content_changes(&synced, &edits, encoding);
      assert_eq!(
        apply_as_server(original, &changes, encoding),
        buffer.contents(),
        "{encoding:?}"
      );
    }
  }

  #[test]
  fn file_uris_round_trip() {
    let path = if cfg!(windows) {
      PathBuf::from(r"D:\Projects\my editor\src\main.rs")
    } else {
      PathBuf::from("/home/me/my editor/src/main.rs")
    };
    let uri = path_to_uri(&path).expect("uri");
    assert!(uri.as_str().starts_with("file:///"));
    assert!(uri.as_str().contains("my%20editor"));
    assert_eq!(uri_to_path(&uri), Some(path));
    let encoded_colon = Uri::from_str("file:///d%3A/x/y.rs").expect("uri");
    if cfg!(windows) {
      assert_eq!(
        uri_to_path(&encoded_colon),
        Some(PathBuf::from(r"d:\x\y.rs"))
      );
    }
  }
}
