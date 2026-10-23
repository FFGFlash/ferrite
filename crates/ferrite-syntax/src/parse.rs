use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use ferrite_text::{ByteEdit, TextBuffer, TextPoint, TextSnapshot};
use tree_sitter::{InputEdit, ParseOptions, Parser, Point, Tree};

use crate::languages::LanguageId;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyntaxError {
  #[error("the {language} highlight query is invalid: {message}")]
  Query {
    language: &'static str,
    message: String,
  },
  #[error("the {language} grammar is incompatible with this version of Tree-sitter")]
  Grammar { language: &'static str },
}

/// Text Tree-sitter can read piece by piece: a live buffer or a snapshot.
pub trait TextSource {
  fn len(&self) -> usize;
  fn is_empty(&self) -> bool {
    self.len() == 0
  }
  /// The text from `offset` to the end of its internal chunk.
  fn chunk_from(&self, offset: usize) -> &str;
}

impl TextSource for TextBuffer {
  fn len(&self) -> usize {
    TextBuffer::len(self)
  }

  fn chunk_from(&self, offset: usize) -> &str {
    TextBuffer::chunk_from(self, offset)
  }
}

impl TextSource for TextSnapshot {
  fn len(&self) -> usize {
    TextSnapshot::len(self)
  }

  fn chunk_from(&self, offset: usize) -> &str {
    TextSnapshot::chunk_from(self, offset)
  }
}

/// Parses `source`. With `old`, the parse is incremental: `old` must already
/// reflect the edits since it was produced (see [`apply_edits`]). Polls
/// `stop` while parsing and gives up, returning `Ok(None)`, once it returns
/// true (see [`deadline`]).
pub fn parse(
  language: LanguageId,
  source: &dyn TextSource,
  old: Option<&Tree>,
  mut stop: impl FnMut() -> bool,
) -> Result<Option<Tree>, SyntaxError> {
  let grammar = language.grammar()?;
  let mut parser = Parser::new();
  parser
    .set_language(&grammar.language)
    .map_err(|_| SyntaxError::Grammar {
      language: language.id(),
    })?;
  let mut read = |offset: usize, _: Point| source.chunk_from(offset).as_bytes();
  let mut check = |_: &tree_sitter::ParseState| {
    if stop() {
      ControlFlow::Break(())
    } else {
      ControlFlow::Continue(())
    }
  };
  let options = ParseOptions::new().progress_callback(&mut check);
  Ok(parser.parse_with_options(&mut read, old, Some(options)))
}

/// A `stop` condition for [`parse`] that is met after `budget`.
pub fn deadline(budget: Duration) -> impl FnMut() -> bool {
  let deadline = Instant::now() + budget;
  move || Instant::now() > deadline
}

fn point(point: TextPoint) -> Point {
  Point {
    row: point.row,
    column: point.column,
  }
}

/// Updates `tree`'s positions for `edits` (in application order), so it can
/// be highlighted right away and reused for an incremental parse.
pub fn apply_edits(tree: &mut Tree, edits: &[ByteEdit]) {
  for edit in edits {
    tree.edit(&InputEdit {
      start_byte: edit.start,
      old_end_byte: edit.old_end,
      new_end_byte: edit.new_end,
      start_position: point(edit.start_point),
      old_end_position: point(edit.old_end_point),
      new_end_position: point(edit.new_end_point),
    });
  }
}

#[cfg(test)]
mod tests {
  use ferrite_text::{ChangeSet, Edit};

  use super::*;

  #[test]
  fn incremental_parses_match_full_parses() {
    let mut buffer = TextBuffer::from_text("fn main() {\n  let x = 1;\n}\n");
    let mut tree = parse(LanguageId::Rust, &buffer, None, || false)
      .expect("grammar")
      .expect("parses");
    let before = buffer.version();
    buffer
      .apply(&ChangeSet::single(Edit::insert(23, " + 2")).expect("valid"))
      .expect("applies");
    apply_edits(&mut tree, &buffer.edits_since(before).expect("edits"));
    let incremental = parse(LanguageId::Rust, &buffer, Some(&tree), || false)
      .expect("grammar")
      .expect("parses");
    let full = parse(LanguageId::Rust, &buffer, None, || false)
      .expect("grammar")
      .expect("parses");
    assert_eq!(
      incremental.root_node().to_sexp(),
      full.root_node().to_sexp()
    );
    assert!(
      incremental
        .root_node()
        .to_sexp()
        .contains("binary_expression")
    );
  }

  #[test]
  fn a_spent_budget_gives_up_instead_of_blocking() {
    let text = "fn f() { let x = [1, 2, 3]; }\n".repeat(200_000);
    let buffer = TextBuffer::from_text(&text);
    let tree = parse(LanguageId::Rust, &buffer, None, deadline(Duration::ZERO)).expect("grammar");
    assert!(tree.is_none());
  }

  #[test]
  fn snapshots_parse_like_buffers() {
    let buffer = TextBuffer::from_text("{\"a\": [1, true]}");
    let snapshot = buffer.snapshot();
    let tree = parse(LanguageId::Json, &snapshot, None, || false)
      .expect("grammar")
      .expect("parses");
    assert!(!tree.root_node().has_error());
  }
}
