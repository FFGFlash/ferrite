//! Helpers for tests that describe documents as marked-up text.

use crate::document::{Document, DocumentId, EditorConfig};
use crate::ops::EditorOp;
use crate::selection::{Selection, Selections};

/// Builds a document from text where `|` marks a cursor and `[...]` a
/// selection whose head is at `]`.
pub fn doc(marked: &str) -> Document {
  let mut text = String::new();
  let mut selections = Vec::new();
  let mut open = None;
  for c in marked.chars() {
    match c {
      '|' => selections.push(Selection::cursor(text.len())),
      '[' => open = Some(text.len()),
      ']' => selections.push(Selection::new(open.take().expect("balanced"), text.len())),
      c => text.push(c),
    }
  }
  let mut document = Document::new(DocumentId(1), &text, None, 1, &EditorConfig::default());
  document.selections = Selections::new(selections, 0);
  document
}

/// The document's text with its selections marked as [`doc`] reads them.
pub fn render(document: &Document) -> String {
  let mut text = document.buffer.contents();
  for s in document.selections.iter().rev() {
    if s.is_empty() {
      text.insert(s.head, '|');
    } else {
      text.insert(s.end(), ']');
      text.insert(s.start(), '[');
    }
  }
  text
}

/// Applies the operations and renders the result.
pub fn run(document: &mut Document, ops: impl IntoIterator<Item = EditorOp>) -> String {
  for op in ops {
    document.apply(op).expect("operation applies");
  }
  render(document)
}
