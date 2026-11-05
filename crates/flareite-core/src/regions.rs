//! Region comments: `// #region Name` … `// #endregion`, written with the
//! language's comment syntax (`# region` in Python, `/* #region */` in CSS,
//! `<!-- #region -->` in HTML), which mark foldable regions; and section
//! comments, `// MARK: Name` (or `// MARK: - Name`, with a separator), which
//! only label a place. The minimap shows both as headers.
//!
//! [`RegionMarkers`] finds the marker comments of a buffer and keeps them up
//! to date: after an edit only the changed lines are read again.

use std::ops::Range;

use flareite_text::{TextBuffer, Version, map_line_range, missing_ranges};

use crate::lines::CommentTokens;

/// Only this much of a line is read for a marker; longer names are cut.
const MAX_MARKER_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
  Start(String),
  End,
  Mark { name: String, separator: bool },
}

/// A region marked by `#region` and `#endregion` comments: lines `start..end`,
/// the `#endregion` line included, as a foldable region (see
/// [`crate::folds`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkedRegion {
  pub lines: Range<usize>,
  pub name: String,
}

/// A labelled place: a named region's first line, or a `MARK:` comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionHeader {
  pub line: usize,
  /// Empty for a separator without a name (`// MARK: -`).
  pub name: String,
  /// Whether a line separates it from what comes before (`MARK: -`).
  pub separator: bool,
  /// Whether it is a region's first line, rather than a `MARK:` comment.
  pub region: bool,
}

/// The marker comments of a buffer, kept up to date with its edits.
#[derive(Debug, Clone, Default)]
pub struct RegionMarkers {
  /// The text version, line count, and comment syntax `markers` are for.
  synced: Option<(Version, usize, CommentTokens)>,
  /// By line, in order.
  markers: Vec<(usize, Kind)>,
}

impl RegionMarkers {
  /// Brings the markers up to date with `buffer`, whose language comments
  /// with `tokens`. Reads only the lines changed since the last call, or
  /// every line the first time (and when `tokens` change).
  pub fn sync(&mut self, buffer: &TextBuffer, tokens: &CommentTokens) {
    let version = buffer.version();
    let line_count = buffer.line_count();
    let changed = match &self.synced {
      Some((synced, _, synced_tokens)) if *synced == version && synced_tokens == tokens => return,
      Some((synced, old_count, synced_tokens)) if synced_tokens == tokens => buffer
        .line_changes_since(*synced)
        .map(|splices| (splices, *old_count)),
      _ => None,
    };
    match changed {
      Some((splices, old_count)) => {
        self.markers.retain_mut(|(line, _)| {
          for splice in &splices {
            let removed_end = splice.start + splice.removed;
            if (splice.start..removed_end).contains(line) {
              return false;
            }
            if *line >= removed_end {
              *line = *line + splice.inserted - splice.removed;
            }
          }
          true
        });
        let kept = map_line_range(0..old_count, &splices);
        for lines in missing_ranges(0..line_count, &kept) {
          self.scan(buffer, tokens, lines);
        }
        self.markers.sort_by_key(|(line, _)| *line);
      }
      None => {
        self.markers.clear();
        self.scan(buffer, tokens, 0..line_count);
      }
    }
    self.synced = Some((version, line_count, tokens.clone()));
  }

  fn scan(&mut self, buffer: &TextBuffer, tokens: &CommentTokens, lines: Range<usize>) {
    if tokens.line.is_none() && tokens.block.is_none() {
      return;
    }
    for line in lines {
      if let Some(kind) = marker(buffer, line, tokens) {
        self.markers.push((line, kind));
      }
    }
  }

  /// The regions, in order of their first line. A `#region` without a
  /// matching `#endregion` marks nothing.
  pub fn regions(&self) -> Vec<MarkedRegion> {
    let mut found = Vec::new();
    let mut open: Vec<(usize, &str)> = Vec::new();
    for (line, kind) in &self.markers {
      match kind {
        Kind::Start(name) => open.push((*line, name)),
        Kind::End => {
          if let Some((start, name)) = open.pop()
            && *line > start
          {
            found.push(MarkedRegion {
              lines: start..line + 1,
              name: name.to_owned(),
            });
          }
        }
        Kind::Mark { .. } => {}
      }
    }
    found.sort_by_key(|region| region.lines.start);
    found
  }

  /// The section headers on `lines`: named regions' first lines and `MARK:`
  /// comments, in order.
  pub fn headers(&self, lines: Range<usize>) -> Vec<SectionHeader> {
    let regions = self.regions();
    let mut headers: Vec<SectionHeader> = regions
      .into_iter()
      .filter(|region| !region.name.is_empty() && lines.contains(&region.lines.start))
      .map(|region| SectionHeader {
        line: region.lines.start,
        name: region.name,
        separator: false,
        region: true,
      })
      .collect();
    for (line, kind) in &self.markers {
      if let Kind::Mark { name, separator } = kind
        && lines.contains(line)
      {
        headers.push(SectionHeader {
          line: *line,
          name: name.clone(),
          separator: *separator,
          region: false,
        });
      }
    }
    headers.sort_by_key(|header| header.line);
    headers
  }
}

/// The marker comment on `line`, if it is one.
fn marker(buffer: &TextBuffer, line: usize, tokens: &CommentTokens) -> Option<Kind> {
  let (text, _) = buffer.line_prefix(line, MAX_MARKER_BYTES);
  let text = text.trim_start();
  let (body, close) = if let Some(rest) = tokens
    .line
    .as_deref()
    .and_then(|token| text.strip_prefix(token))
  {
    (rest, None)
  } else {
    let (open, close) = tokens.block.as_ref()?;
    (text.strip_prefix(open.as_str())?, Some(close.as_str()))
  };
  let body = body.trim();
  let body = close
    .and_then(|close| body.strip_suffix(close))
    .unwrap_or(body)
    .trim_end();
  if let Some(name) = body.strip_prefix("MARK:") {
    let name = name.trim();
    return Some(match name.strip_prefix('-') {
      Some(rest) => Kind::Mark {
        name: rest.trim().to_owned(),
        separator: true,
      },
      None => Kind::Mark {
        name: name.to_owned(),
        separator: false,
      },
    });
  }
  let word = body.strip_prefix('#').unwrap_or(body);
  let ends_word = |rest: &str| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_');
  if let Some(rest) = word.strip_prefix("endregion")
    && ends_word(rest)
  {
    return Some(Kind::End);
  }
  match word.strip_prefix("region") {
    Some(rest) if ends_word(rest) => Some(Kind::Start(rest.trim().to_owned())),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use flareite_text::{ChangeSet, Edit};

  use super::*;

  fn slashes() -> CommentTokens {
    CommentTokens {
      line: Some("//".into()),
      block: Some(("/*".into(), "*/".into())),
    }
  }

  fn regions(text: &str, tokens: &CommentTokens) -> Vec<(Range<usize>, String)> {
    let mut markers = RegionMarkers::default();
    markers.sync(&TextBuffer::from_text(text), tokens);
    markers
      .regions()
      .into_iter()
      .map(|region| (region.lines, region.name))
      .collect()
  }

  #[test]
  fn finds_nested_regions_in_each_comment_syntax() {
    let text =
      "// #region Outer\nfn a() {}\n  //#region inner\n  x\n  // #endregion\n// #endregion\n";
    assert_eq!(
      regions(text, &slashes()),
      [(0..6, "Outer".to_owned()), (2..5, "inner".to_owned())]
    );
    let python = CommentTokens {
      line: Some("#".into()),
      block: None,
    };
    assert_eq!(
      regions("# region Setup\nx = 1\n#endregion\n", &python),
      [(0..3, "Setup".to_owned())]
    );
    let html = CommentTokens {
      line: None,
      block: Some(("<!--".into(), "-->".into())),
    };
    assert_eq!(
      regions("<!-- #region Nav -->\n<nav/>\n<!-- #endregion -->\n", &html),
      [(0..3, "Nav".to_owned())]
    );
  }

  #[test]
  fn ignores_lookalikes_and_unmatched_markers() {
    let text = "// #regional\n// #endregion\n// #region\n// regionless\nlet region = 1;\n";
    assert!(regions(text, &slashes()).is_empty());
    assert!(regions("// #region\nx\n// #endregion\n", &CommentTokens::default()).is_empty());
  }

  #[test]
  fn mark_comments_are_headers() {
    let text = "// MARK: Setup\n// #region Body\nx\n// #endregion\n// MARK: - Tail\n// MARK: -\n";
    let mut markers = RegionMarkers::default();
    markers.sync(&TextBuffer::from_text(text), &slashes());
    let all = markers.headers(0..6);
    let headers: Vec<(usize, &str, bool, bool)> = all
      .iter()
      .map(|h| (h.line, h.name.as_str(), h.separator, h.region))
      .collect();
    assert_eq!(
      headers,
      [
        (0, "Setup", false, false),
        (1, "Body", false, true),
        (4, "Tail", true, false),
        (5, "", true, false),
      ]
    );
    assert_eq!(markers.headers(2..5).len(), 1, "only lines 2 to 4");
  }

  #[test]
  fn edits_rescan_only_changed_lines_and_shift_the_rest() {
    let mut buffer = TextBuffer::from_text("// #region A\nx\n// #endregion\n");
    let mut markers = RegionMarkers::default();
    markers.sync(&buffer, &slashes());
    buffer
      .apply(&ChangeSet::single(Edit::insert(0, "top\n// #region B\n")).expect("valid"))
      .expect("applies");
    let end = buffer.len();
    buffer
      .apply(&ChangeSet::single(Edit::insert(end, "// #endregion\n")).expect("valid"))
      .expect("applies");
    markers.sync(&buffer, &slashes());
    let found: Vec<_> = markers.regions().into_iter().map(|r| r.lines).collect();
    assert_eq!(found, [1..6, 2..5]);
    // A full scan agrees.
    let mut fresh = RegionMarkers::default();
    fresh.sync(&buffer, &slashes());
    assert_eq!(fresh.regions(), markers.regions());
  }
}
