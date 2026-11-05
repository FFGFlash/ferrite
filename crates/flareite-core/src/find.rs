//! Find and replace within a document.

use std::ops::Range;

use flareite_text::{Bias, ChangeSet, Edit, SearchQuery, Searcher};

use crate::document::Document;
use crate::error::CoreError;
use crate::history::EditKind;
use crate::selection::{Selection, Selections};

/// Most matches counted or selected at once. Beyond this, counts are
/// reported as truncated.
pub const MAX_MATCHES: usize = 20_000;

/// Matches of a query, for highlighting and the "3 of 17" display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchSummary {
  /// Matches overlapping the requested range.
  pub visible: Vec<Range<usize>>,
  /// Total matches in the document, up to [`MAX_MATCHES`].
  pub total: usize,
  pub truncated: bool,
  /// Index of the match the primary selection covers exactly, if any.
  pub current: Option<usize>,
}

impl Document {
  /// Matches of `searcher` for display: those overlapping `visible`, plus
  /// document-wide counts.
  pub fn match_summary(&self, searcher: &Searcher, visible: Range<usize>) -> MatchSummary {
    let all = searcher.find_in_buffer(&self.buffer, 0..self.buffer.len(), MAX_MATCHES + 1);
    let truncated = all.len() > MAX_MATCHES;
    let all = &all[..all.len().min(MAX_MATCHES)];
    let primary = self.selections.primary().range();
    let first_visible = all.partition_point(|found| found.end <= visible.start);
    MatchSummary {
      visible: all[first_visible..]
        .iter()
        .take_while(|found| found.start < visible.end)
        .cloned()
        .collect(),
      total: all.len(),
      truncated,
      current: all
        .binary_search_by_key(&primary.start, |found| found.start)
        .ok()
        .filter(|&index| all[index] == primary),
    }
  }

  pub(crate) fn find_next(&mut self, searcher: &Searcher, backward: bool) -> bool {
    let primary = self.selections.primary();
    let found = if backward {
      searcher.prev_match(&self.buffer, primary.start())
    } else {
      searcher.next_match(&self.buffer, primary.end())
    };
    match found {
      Some(range) => {
        self.set_selections(Selections::single(Selection::new(range.start, range.end)));
        true
      }
      None => false,
    }
  }

  pub(crate) fn select_all_matches(&mut self, searcher: &Searcher) {
    let matches = searcher.find_in_buffer(&self.buffer, 0..self.buffer.len(), MAX_MATCHES);
    if matches.is_empty() {
      return;
    }
    let primary_start = self.selections.primary().start();
    let primary = matches
      .iter()
      .position(|found| found.start >= primary_start)
      .unwrap_or(0);
    let selections = matches
      .into_iter()
      .map(|found| Selection::new(found.start, found.end))
      .collect();
    self.set_selections(Selections::new(selections, primary));
  }

  /// Selects every occurrence of the primary selection's text, or with an
  /// empty selection, of the word at the cursor (matching whole words).
  pub(crate) fn select_all_occurrences(&mut self) {
    let primary = self.selections.primary();
    let range = if primary.is_empty() {
      self.buffer.word_at(primary.head)
    } else {
      primary.range()
    };
    let text = self.buffer.text_in(range);
    if text.trim().is_empty() {
      return;
    }
    let whole_word = primary.is_empty() && text.chars().all(|c| c.is_alphanumeric() || c == '_');
    let query = SearchQuery {
      whole_word,
      ..SearchQuery::literal(text)
    };
    if let Ok(searcher) = query.compile() {
      self.select_all_matches(&searcher);
    }
  }

  pub(crate) fn replace_current(
    &mut self,
    searcher: &Searcher,
    replacement: &str,
  ) -> Result<bool, CoreError> {
    let primary = self.selections.primary();
    let is_match = !primary.is_empty()
      && searcher
        .find_in_buffer(&self.buffer, primary.range(), 1)
        .first()
        == Some(&primary.range());
    if !is_match {
      self.find_next(searcher, false);
      return Ok(false);
    }

    let text = searcher.expand_replacement(&self.buffer, primary.range(), replacement);
    let changes = ChangeSet::single(Edit::new(primary.range(), text))?;
    let before = self.selections.clone();
    let inverse = self.buffer.apply(&changes)?;
    let end = changes
      .post_ranges()
      .first()
      .map_or(primary.start(), |range| range.end);
    let after = Selections::cursor(end);
    self.selections = after.clone();
    self
      .history
      .record(changes, inverse, before, after, EditKind::Other, false);
    self.find_next(searcher, false);
    Ok(true)
  }

  pub(crate) fn replace_all(
    &mut self,
    searcher: &Searcher,
    replacement: &str,
  ) -> Result<bool, CoreError> {
    let edits: Vec<Edit> = searcher
      .find_in_buffer(&self.buffer, 0..self.buffer.len(), usize::MAX)
      .into_iter()
      .map(|found| {
        let text = searcher.expand_replacement(&self.buffer, found.clone(), replacement);
        Edit::new(found, text)
      })
      .collect();
    let changes = ChangeSet::new(edits)?;
    if changes.is_empty() {
      return Ok(false);
    }
    let before = self.selections.clone();
    let inverse = self.buffer.apply(&changes)?;
    let after = before.map(|s| Selection::cursor(changes.map_offset(s.head, Bias::After)));
    self.selections = after.clone();
    self
      .history
      .record(changes, inverse, before, after, EditKind::Other, false);
    Ok(true)
  }

  pub(crate) fn add_next_match(&mut self) {
    let primary = self.selections.primary();
    if primary.is_empty() {
      let word = self.buffer.word_at(primary.head);
      if !word.is_empty() {
        self.set_selections(Selections::single(Selection::new(word.start, word.end)));
      }
      return;
    }
    let Ok(searcher) = SearchQuery::literal(self.buffer.text_in(primary.range())).compile() else {
      return;
    };
    let matches = searcher.find_in_buffer(&self.buffer, 0..self.buffer.len(), MAX_MATCHES);
    let split = matches.partition_point(|found| found.start < primary.end());
    let next = matches[split..]
      .iter()
      .chain(&matches[..split])
      .find(|found| !self.selections.iter().any(|s| s.range() == **found));
    if let Some(found) = next {
      let added = self
        .selections
        .with_added(Selection::new(found.start, found.end));
      self.set_selections(added);
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::document::{DocumentId, EditorConfig};
  use crate::ops::EditorOp;

  use super::*;

  fn doc(text: &str, cursor: usize) -> Document {
    let mut document = Document::new(DocumentId(1), text, None, 1, &EditorConfig::default());
    document.selections = Selections::cursor(cursor);
    document
  }

  fn query(pattern: &str) -> SearchQuery {
    SearchQuery {
      pattern: pattern.into(),
      ..SearchQuery::default()
    }
  }

  fn ranges(document: &Document) -> Vec<Range<usize>> {
    document.selections().iter().map(Selection::range).collect()
  }

  #[test]
  fn find_next_and_previous_select_matches_and_wrap() {
    let mut document = doc("one two one two one", 0);
    let next = |backward| EditorOp::FindNext {
      query: query("one"),
      backward,
    };
    document.apply(next(false)).expect("applies");
    assert_eq!(ranges(&document), vec![0..3]);
    document.apply(next(false)).expect("applies");
    assert_eq!(ranges(&document), vec![8..11]);
    document.apply(next(true)).expect("applies");
    assert_eq!(ranges(&document), vec![0..3]);
    document.apply(next(true)).expect("applies");
    assert_eq!(ranges(&document), vec![16..19]);
  }

  #[test]
  fn summaries_count_and_locate_the_current_match() {
    let mut document = doc("ab\nab\nab\nab", 0);
    let searcher = query("ab").compile().expect("compiles");
    document.find_next(&searcher, false);
    document.find_next(&searcher, false);
    let summary = document.match_summary(&searcher, 2..7);
    assert_eq!(summary.total, 4);
    assert_eq!(summary.current, Some(1));
    assert_eq!(summary.visible, vec![3..5, 6..8]);
    assert!(!summary.truncated);
  }

  #[test]
  fn replace_replaces_the_current_match_then_moves_on() {
    let mut document = doc("cat cat cat", 0);
    let replace = || EditorOp::Replace {
      query: query("cat"),
      replacement: "dog".into(),
    };
    // The first call only selects a match.
    assert!(!document.apply(replace()).expect("applies"));
    assert_eq!(ranges(&document), vec![0..3]);
    assert!(document.apply(replace()).expect("applies"));
    assert_eq!(document.buffer().contents(), "dog cat cat");
    assert_eq!(ranges(&document), vec![4..7]);
  }

  #[test]
  fn replace_all_is_one_undo_step_and_expands_groups() {
    let mut document = doc("a=1, b=2", 8);
    let op = EditorOp::ReplaceAll {
      query: SearchQuery {
        regex: true,
        ..query(r"(\w)=(\d)")
      },
      replacement: "$2=$1".into(),
    };
    assert!(document.apply(op).expect("applies"));
    assert_eq!(document.buffer().contents(), "1=a, 2=b");
    assert_eq!(ranges(&document), vec![8..8]);
    document.apply(EditorOp::Undo).expect("applies");
    assert_eq!(document.buffer().contents(), "a=1, b=2");
  }

  #[test]
  fn select_all_matches_creates_one_selection_each() {
    let mut document = doc("x y x y x", 3);
    document
      .apply(EditorOp::SelectAllMatches { query: query("x") })
      .expect("applies");
    assert_eq!(ranges(&document), vec![0..1, 4..5, 8..9]);
    assert_eq!(document.selections().primary().range(), 4..5);
  }

  #[test]
  fn add_next_match_selects_the_word_then_adds_occurrences() {
    let mut document = doc("foo bar foo foo", 1);
    document.apply(EditorOp::AddNextMatch).expect("applies");
    assert_eq!(ranges(&document), vec![0..3]);
    document.apply(EditorOp::AddNextMatch).expect("applies");
    assert_eq!(ranges(&document), vec![0..3, 8..11]);
    document.apply(EditorOp::AddNextMatch).expect("applies");
    document.apply(EditorOp::AddNextMatch).expect("applies");
    assert_eq!(ranges(&document), vec![0..3, 8..11, 12..15]);
    document
      .apply(EditorOp::Insert("baz".into()))
      .expect("applies");
    assert_eq!(document.buffer().contents(), "baz bar baz baz");
  }

  #[test]
  fn select_all_occurrences_uses_the_selection_or_the_whole_word() {
    let mut document = doc("foo food foo", 1);
    document
      .apply(EditorOp::SelectAllOccurrences)
      .expect("applies");
    assert_eq!(ranges(&document), vec![0..3, 9..12]);
    document.selections = Selections::single(Selection::new(0, 3));
    document
      .apply(EditorOp::SelectAllOccurrences)
      .expect("applies");
    assert_eq!(ranges(&document), vec![0..3, 4..7, 9..12]);
    let mut document = doc("a  b", 2);
    document
      .apply(EditorOp::SelectAllOccurrences)
      .expect("applies");
    assert_eq!(ranges(&document), vec![2..2]);
  }

  #[test]
  fn invalid_patterns_are_errors() {
    let mut document = doc("text", 0);
    let result = document.apply(EditorOp::FindNext {
      query: SearchQuery {
        regex: true,
        ..query("[")
      },
      backward: false,
    });
    assert!(matches!(result, Err(CoreError::Search(_))));
  }
}
