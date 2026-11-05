//! Injections: languages embedded in a document, such as CSS in an HTML
//! `<style>` element. A language's injection query (`injections.scm`)
//! captures the embedded text as `@injection.content` and names its
//! language either with `(#set! injection.language "css")` or by capturing
//! the name as `@injection.language`. Each injection is parsed as its own
//! layer over the captured ranges (see `parse`); matches of a pattern with
//! `(#set! injection.combined)` form one layer together.
//!
//! The whole captured node is used, children included.

use std::ops::Range;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, Tree};

use crate::parse::TextSource;

/// Longer language names captured from the text are ignored.
const MAX_LANGUAGE_NAME: usize = 64;

pub(crate) struct Injections {
  query: Query,
  content: u32,
  language: Option<u32>,
  /// The language each pattern sets with `#set! injection.language`.
  fixed: Vec<Option<String>>,
  /// Whether each pattern sets `injection.combined`.
  combined: Vec<bool>,
}

/// An injection found in a tree: a language id and the ranges of its text,
/// in order.
pub(crate) struct Found {
  pub(crate) language: String,
  pub(crate) ranges: Vec<tree_sitter::Range>,
}

impl Injections {
  /// `None` if the query captures no `@injection.content`.
  pub(crate) fn new(query: Query) -> Option<Self> {
    let content = query.capture_index_for_name("injection.content")?;
    let language = query.capture_index_for_name("injection.language");
    let setting = |pattern: usize, key: &str| {
      query
        .property_settings(pattern)
        .iter()
        .find(|property| &*property.key == key)
        .map(|property| property.value.as_deref().unwrap_or_default().to_owned())
    };
    let patterns = 0..query.pattern_count();
    let fixed = patterns
      .clone()
      .map(|pattern| setting(pattern, "injection.language"))
      .collect();
    let combined = patterns
      .map(|pattern| setting(pattern, "injection.combined").is_some())
      .collect();
    Some(Self {
      query,
      content,
      language,
      fixed,
      combined,
    })
  }

  /// The injections in `tree`, a parse of `source`, in document order.
  pub(crate) fn find(&self, tree: &Tree, source: &dyn TextSource) -> Vec<Found> {
    let mut cursor = QueryCursor::new();
    let text = |node: Node| Chunks {
      source,
      offset: node.start_byte(),
      end: node.end_byte(),
    };
    let mut matches = cursor.matches(&self.query, tree.root_node(), text);
    let mut found: Vec<Found> = Vec::new();
    // Index in `found` of each (pattern, language) being combined.
    let mut combining: Vec<(usize, String, usize)> = Vec::new();
    while let Some(found_match) = matches.next() {
      let pattern = found_match.pattern_index;
      let mut content = Vec::new();
      let mut language = self.fixed[pattern].clone();
      for capture in found_match.captures() {
        if capture.index == self.content {
          content.push(capture.node.range());
        } else if Some(capture.index) == self.language && language.is_none() {
          language = name(source, capture.node.byte_range());
        }
      }
      let Some(language) = language.filter(|language| !language.is_empty()) else {
        continue;
      };
      if content.is_empty() {
        continue;
      }
      if self.combined[pattern] {
        if let Some(&(_, _, index)) = combining
          .iter()
          .find(|(p, l, _)| *p == pattern && *l == language)
        {
          found[index].ranges.extend(content);
          continue;
        }
        combining.push((pattern, language.clone(), found.len()));
      }
      found.push(Found {
        language,
        ranges: content,
      });
    }
    // Included ranges must be ordered and must not overlap.
    for injection in &mut found {
      injection.ranges.sort_by_key(|range| range.start_byte);
      injection
        .ranges
        .dedup_by(|next, previous| next.start_byte < previous.end_byte);
    }
    found.sort_by_key(|injection| injection.ranges[0].start_byte);
    found
  }
}

/// A language name captured from the text, lowercased.
fn name(source: &dyn TextSource, range: Range<usize>) -> Option<String> {
  if range.len() > MAX_LANGUAGE_NAME {
    return None;
  }
  let bytes: Vec<u8> = Chunks {
    source,
    offset: range.start,
    end: range.end,
  }
  .flatten()
  .copied()
  .collect();
  String::from_utf8(bytes)
    .ok()
    .map(|name| name.trim().to_ascii_lowercase())
}

/// The text of `offset..end` in `source`, chunk by chunk.
struct Chunks<'a> {
  source: &'a dyn TextSource,
  offset: usize,
  end: usize,
}

impl<'a> Iterator for Chunks<'a> {
  type Item = &'a [u8];

  fn next(&mut self) -> Option<&'a [u8]> {
    if self.offset >= self.end {
      return None;
    }
    let chunk = self.source.chunk_from(self.offset).as_bytes();
    if chunk.is_empty() {
      return None;
    }
    let chunk = &chunk[..chunk.len().min(self.end - self.offset)];
    self.offset += chunk.len();
    Some(chunk)
  }
}
