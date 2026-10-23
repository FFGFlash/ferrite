//! Text search over buffers and plain strings.
//!
//! Queries compile to a regex (literal patterns are escaped), searched
//! directly over the rope's chunks without copying the text. `^` and `$`
//! match at line boundaries, including before CRLF. Empty matches are
//! skipped.

use std::ops::Range;

use regex_automata::util::{interpolate, syntax};
use regex_automata::{Anchored, Span};
use regex_cursor::Input;
use regex_cursor::engines::meta::Regex;

use crate::buffer::TextBuffer;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchQuery {
  pub pattern: String,
  /// Interpret `pattern` as a regular expression instead of literal text.
  pub regex: bool,
  pub case_sensitive: bool,
  /// Only match whole words (`\b` on both sides).
  pub whole_word: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
  #[error("the search pattern is empty")]
  EmptyPattern,
  #[error("invalid regular expression: {0}")]
  InvalidPattern(String),
}

impl SearchQuery {
  pub fn literal(pattern: impl Into<String>) -> Self {
    Self {
      pattern: pattern.into(),
      case_sensitive: true,
      ..Self::default()
    }
  }

  pub fn compile(&self) -> Result<Searcher, SearchError> {
    if self.pattern.is_empty() {
      return Err(SearchError::EmptyPattern);
    }
    let mut pattern = if self.regex {
      self.pattern.clone()
    } else {
      regex_syntax::escape(&self.pattern)
    };
    if self.whole_word {
      pattern = format!(r"\b(?:{pattern})\b");
    }
    let regex = Regex::builder()
      .syntax(
        syntax::Config::new()
          .case_insensitive(!self.case_sensitive)
          .multi_line(true)
          .crlf(true),
      )
      .build(&pattern)
      .map_err(|error| SearchError::InvalidPattern(error.to_string()))?;
    Ok(Searcher {
      regex,
      expands_groups: self.regex,
    })
  }
}

/// A compiled [`SearchQuery`].
#[derive(Debug, Clone)]
pub struct Searcher {
  regex: Regex,
  /// Whether replacements may refer to capture groups (`$1`, `${name}`).
  expands_groups: bool,
}

impl Searcher {
  /// Non-empty matches within `range`, in order, at most `limit`.
  pub fn find_in_buffer(
    &self,
    buffer: &TextBuffer,
    range: Range<usize>,
    limit: usize,
  ) -> Vec<Range<usize>> {
    let mut input = Input::new(&buffer.rope);
    input.span(range.start.min(buffer.len())..range.end.min(buffer.len()));
    self
      .regex
      .find_iter(input)
      .filter(|found| !found.is_empty())
      .take(limit)
      .map(|found| found.range())
      .collect()
  }

  /// Non-empty matches in `text`, in order.
  pub fn find_in_str<'a>(&'a self, text: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
    self
      .regex
      .find_iter(Input::new(text))
      .filter(|found| !found.is_empty())
      .map(|found| found.range())
  }

  /// The first match starting at or after `from`, wrapping around to the
  /// start of the text.
  pub fn next_match(&self, buffer: &TextBuffer, from: usize) -> Option<Range<usize>> {
    self
      .find_in_buffer(buffer, from..buffer.len(), 1)
      .into_iter()
      .next()
      .or_else(|| {
        self
          .find_in_buffer(buffer, 0..buffer.len(), 1)
          .into_iter()
          .next()
      })
  }

  /// The last match ending at or before `before`, wrapping around to the end
  /// of the text. Scans the text from the start.
  pub fn prev_match(&self, buffer: &TextBuffer, before: usize) -> Option<Range<usize>> {
    let all = self.find_in_buffer(buffer, 0..buffer.len(), usize::MAX);
    all
      .iter()
      .rev()
      .find(|found| found.end <= before)
      .or_else(|| all.last())
      .cloned()
  }

  /// The text that replaces the match at `range`. For regex queries, `$1`,
  /// `${name}` and `$$` in `replacement` expand to capture groups and `$`.
  pub fn expand_replacement(
    &self,
    buffer: &TextBuffer,
    range: Range<usize>,
    replacement: &str,
  ) -> String {
    if !self.expands_groups || !replacement.contains('$') {
      return replacement.to_owned();
    }
    let mut input = Input::new(&buffer.rope);
    input.span(range);
    input.anchored(Anchored::Yes);
    let mut captures = self.regex.create_captures();
    self.regex.captures(input, &mut captures);
    if !captures.is_match() {
      return replacement.to_owned();
    }
    let mut expanded = String::new();
    interpolate::string(
      replacement,
      |index, dst| {
        if let Some(Span { start, end }) = captures.get_group(index) {
          dst.push_str(&buffer.text_in(start..end));
        }
      },
      |name| {
        captures
          .group_info()
          .to_index(regex_automata::PatternID::ZERO, name)
      },
      &mut expanded,
    );
    expanded
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn query(pattern: &str) -> SearchQuery {
    SearchQuery {
      pattern: pattern.into(),
      ..SearchQuery::default()
    }
  }

  fn matches(query: &SearchQuery, text: &str) -> Vec<Range<usize>> {
    let buffer = TextBuffer::from_text(text);
    let searcher = query.compile().expect("compiles");
    searcher.find_in_buffer(&buffer, 0..buffer.len(), usize::MAX)
  }

  #[test]
  fn literal_patterns_escape_regex_syntax() {
    assert_eq!(matches(&query("a.b"), "a.b axb a.b"), vec![0..3, 8..11]);
    assert_eq!(matches(&query("("), "f(x)"), vec![1..2]);
  }

  #[test]
  fn case_sensitivity_is_optional() {
    assert_eq!(
      matches(&query("foo"), "Foo FOO foo"),
      vec![0..3, 4..7, 8..11]
    );
    let sensitive = SearchQuery {
      case_sensitive: true,
      ..query("foo")
    };
    assert_eq!(matches(&sensitive, "Foo FOO foo"), vec![8..11]);
  }

  #[test]
  fn whole_words_only() {
    let whole = SearchQuery {
      whole_word: true,
      ..query("cat")
    };
    assert_eq!(matches(&whole, "cat concat cats cat."), vec![0..3, 16..19]);
  }

  #[test]
  fn regex_anchors_work_per_line_including_crlf() {
    let pattern = SearchQuery {
      regex: true,
      ..query("^b.*$")
    };
    assert_eq!(matches(&pattern, "a\r\nbc\r\nbd"), vec![3..5, 7..9]);
  }

  #[test]
  fn empty_matches_are_skipped_and_invalid_patterns_reported() {
    let star = SearchQuery {
      regex: true,
      ..query("x*")
    };
    assert_eq!(matches(&star, "axxb"), vec![1..3]);
    assert_eq!(query("").compile().err(), Some(SearchError::EmptyPattern));
    let invalid = SearchQuery {
      regex: true,
      ..query("(")
    };
    assert!(matches!(
      invalid.compile(),
      Err(SearchError::InvalidPattern(_))
    ));
  }

  #[test]
  fn matches_span_rope_chunks_and_unicode() {
    let text = format!("{}é needle 🦀 needle", "x".repeat(10_000));
    let found = matches(&query("needle"), &text);
    assert_eq!(found.len(), 2);
    assert_eq!(&text[found[0].clone()], "needle");
  }

  #[test]
  fn next_and_previous_matches_wrap_around() {
    let buffer = TextBuffer::from_text("ab ab ab");
    let searcher = query("ab").compile().expect("compiles");
    assert_eq!(searcher.next_match(&buffer, 1), Some(3..5));
    assert_eq!(searcher.next_match(&buffer, 7), Some(0..2));
    assert_eq!(searcher.prev_match(&buffer, 5), Some(3..5));
    assert_eq!(searcher.prev_match(&buffer, 1), Some(6..8));
  }

  #[test]
  fn regex_replacements_expand_capture_groups() {
    let buffer = TextBuffer::from_text("let x = 1;");
    let pattern = SearchQuery {
      regex: true,
      ..query(r"let (?<name>\w+) = (\d+)")
    };
    let searcher = pattern.compile().expect("compiles");
    let found = searcher.next_match(&buffer, 0).expect("match");
    assert_eq!(
      searcher.expand_replacement(&buffer, found.clone(), "const ${name}: i32 = $2 $$"),
      "const x: i32 = 1 $"
    );
    // Literal queries never expand.
    let literal = query("let").compile().expect("compiles");
    assert_eq!(literal.expand_replacement(&buffer, 0..3, "$1"), "$1");
  }

  #[test]
  fn searches_plain_strings() {
    let searcher = query("b").compile().expect("compiles");
    assert_eq!(
      searcher.find_in_str("abcb").collect::<Vec<_>>(),
      vec![1..2, 3..4]
    );
  }
}
