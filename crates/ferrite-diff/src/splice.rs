//! Moving changes from one side of a diff to the other: reverting some of a
//! modified text's changes, or applying them to the original (staging them,
//! for Git).
//!
//! A change's lines are whole lines, each with its line break. The last line
//! of a text has none, so both texts are treated as if they ended with one,
//! and the edits are then fitted back to the real text.

use std::ops::{Range, RangeInclusive};

use crate::LineChange;

/// Text replacing a byte range. Line breaks in `text` are `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Splice {
  pub range: Range<usize>,
  pub text: String,
}

/// The changes touching `lines` of the modified text. A removal, between
/// lines, touches the lines on either side.
pub fn touching<'a>(
  changes: &'a [LineChange],
  lines: &[RangeInclusive<usize>],
) -> Vec<&'a LineChange> {
  changes
    .iter()
    .filter(|change| {
      let Range { start, end } = change.modified;
      lines.iter().any(|selected| {
        let (first, last) = (*selected.start(), *selected.end());
        if start == end {
          first <= start && start <= last + 1
        } else {
          start <= last && first < end
        }
      })
    })
    .collect()
}

/// Where each line of `text` starts, as if `text` ended with a line break,
/// and where that last line break would end.
fn line_starts(text: &str) -> Vec<usize> {
  let mut starts = vec![0];
  starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
  starts.push(text.len() + 1);
  starts
}

/// The edits to `target` that replace the lines of `chosen` (each its lines
/// in `target` and in `source`) with `source`'s, in order.
pub fn splices(target: &str, source: &str, chosen: &[(Range<usize>, Range<usize>)]) -> Vec<Splice> {
  let target_starts = line_starts(target);
  let source_starts = line_starts(source);
  let source_padded = format!("{source}\n");
  let at = |starts: &[usize], line: usize| starts[line.min(starts.len() - 1)];
  let mut splices: Vec<Splice> = chosen
    .iter()
    .map(|(lines, from)| {
      let mut start = at(&target_starts, lines.start);
      let mut end = at(&target_starts, lines.end);
      let mut text = source_padded[at(&source_starts, from.start)..at(&source_starts, from.end)]
        .replace("\r\n", "\n");
      // Fitted to the text without the line break added to its end.
      let len = target.len();
      if start > len {
        // New lines after the last: a line break, then them.
        start = len;
        end = len;
        text.pop();
        text.insert(0, '\n');
      } else if end > len {
        end = len;
        if text.is_empty() {
          // The last lines go, with the line break before them.
          start = start.saturating_sub(1);
          if start > 0 && target.as_bytes()[start - 1] == b'\r' {
            start -= 1;
          }
        } else {
          text.pop();
        }
      }
      Splice {
        range: start..end,
        text,
      }
    })
    .collect();
  splices.sort_by_key(|splice| splice.range.start);
  splices
}

/// `original` with the chosen changes made, its line breaks kept: what
/// staging them makes the original.
pub fn apply(original: &str, modified: &str, chosen: &[&LineChange]) -> String {
  let pairs: Vec<_> = chosen
    .iter()
    .map(|change| (change.original.clone(), change.modified.clone()))
    .collect();
  let line_break = if original.contains("\r\n") {
    "\r\n"
  } else {
    "\n"
  };
  let mut result = original.to_owned();
  for splice in splices(original, modified, &pairs).into_iter().rev() {
    result.replace_range(splice.range, &splice.text.replace('\n', line_break));
  }
  result
}

/// The edits to `modified` that undo the chosen changes, putting back the
/// original's lines.
pub fn revert(original: &str, modified: &str, chosen: &[&LineChange]) -> Vec<Splice> {
  let pairs: Vec<_> = chosen
    .iter()
    .map(|change| (change.modified.clone(), change.original.clone()))
    .collect();
  splices(modified, original, &pairs)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{DiffOptions, diff};

  fn changes(original: &str, modified: &str) -> Vec<LineChange> {
    diff(original, modified, DiffOptions::default()).expect("small")
  }

  fn reverted(original: &str, modified: &str, which: &[usize]) -> String {
    let all = changes(original, modified);
    let chosen: Vec<&LineChange> = which.iter().map(|&index| &all[index]).collect();
    let mut text = modified.to_owned();
    for splice in revert(original, modified, &chosen).into_iter().rev() {
      text.replace_range(splice.range, &splice.text);
    }
    text
  }

  fn applied(original: &str, modified: &str, which: &[usize]) -> String {
    let all = changes(original, modified);
    let chosen: Vec<&LineChange> = which.iter().map(|&index| &all[index]).collect();
    apply(original, modified, &chosen)
  }

  #[test]
  fn reverting_or_applying_every_change_gives_the_other_side() {
    let cases = [
      ("a\nb\nc\n", "a\nB\nc\n"),
      ("a\nb", "a\nb\nc"),
      ("a\nb\nc", "a\nb"),
      ("a\nb\n", "a\nb"),
      ("a\nb", "a\nb\n"),
      ("", "x\ny\n"),
      ("x\ny\n", ""),
      ("one\ntwo\nthree\nfour\n", "zero\none\nthree\nfour\nfive"),
    ];
    for (original, modified) in cases {
      let all: Vec<usize> = (0..changes(original, modified).len()).collect();
      assert_eq!(
        reverted(original, modified, &all),
        original,
        "{original:?} -> {modified:?}"
      );
      assert_eq!(
        applied(original, modified, &all),
        modified,
        "{original:?} -> {modified:?}"
      );
    }
  }

  #[test]
  fn some_changes_move_and_the_others_stay() {
    let original = "a\nb\nc\nd\ne\nf\ng\n";
    let modified = "a\nB\nc\nd\ne\nF\ng\n";
    assert_eq!(reverted(original, modified, &[1]), "a\nB\nc\nd\ne\nf\ng\n");
    assert_eq!(applied(original, modified, &[0]), "a\nB\nc\nd\ne\nf\ng\n");
  }

  #[test]
  fn line_breaks_follow_each_side() {
    // The original (as Git stores it) has \n; the file on disk \r\n.
    let original = "a\nb\nc\n";
    let modified = "a\r\nB\r\nc\r\n";
    let all = changes(original, modified);
    let chosen: Vec<&LineChange> = all.iter().collect();
    assert_eq!(apply(original, modified, &chosen), "a\nB\nc\n");
    let edits = revert(original, modified, &chosen);
    assert_eq!(edits.len(), 1);
    assert_eq!(&modified[edits[0].range.clone()], "B\r\n");
    assert_eq!(edits[0].text, "b\n");
  }

  // Each slice holds one range of lines, not the lines.
  #[allow(clippy::single_range_in_vec_init)]
  #[test]
  fn changes_touching_lines_are_found() {
    let all = changes("a\nb\nc\nd\ne\n", "a\nB\nc\ne\n");
    // Line 1 changed; line 3 (`d`) was removed before modified line 3.
    assert_eq!(touching(&all, &[1..=1]).len(), 1);
    assert_eq!(touching(&all, &[0..=0]).len(), 0);
    assert_eq!(
      touching(&all, &[2..=2]),
      vec![&all[1]],
      "a removal touches the line before"
    );
    assert_eq!(
      touching(&all, &[3..=3]),
      vec![&all[1]],
      "and the line after"
    );
    assert_eq!(touching(&all, &[0..=4]).len(), 2);
  }
}
