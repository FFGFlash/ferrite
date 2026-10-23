//! Line-level change descriptions, for incremental views.
//!
//! Every applied change records how it moved lines as [`LineSplice`]s. A
//! client that cached lines at an older version can replay the splices to
//! learn which of its lines are still valid (and where they moved), and
//! fetch only the rest.

use std::collections::VecDeque;
use std::ops::Range;

use crate::buffer::Version;

/// Lines `start..start + removed` (numbered as before the change, with
/// earlier splices of the same sequence already applied) were replaced by
/// lines `start..start + inserted`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineSplice {
  pub start: usize,
  pub removed: usize,
  pub inserted: usize,
}

/// Most changes remembered. Clients further behind get a full refresh.
const LOG_CAPACITY: usize = 512;

#[derive(Debug, Clone, Default)]
pub(crate) struct LineLog {
  entries: VecDeque<(Version, Vec<LineSplice>)>,
}

impl LineLog {
  pub(crate) fn push(&mut self, version: Version, splices: Vec<LineSplice>) {
    if self.entries.len() == LOG_CAPACITY {
      self.entries.pop_front();
    }
    self.entries.push_back((version, splices));
  }

  /// The splices from `since` to `current`, or `None` if the log no longer
  /// reaches back that far (or `since` is in the future).
  pub(crate) fn since(&self, since: Version, current: Version) -> Option<Vec<LineSplice>> {
    if since == current {
      return Some(Vec::new());
    }
    if since > current {
      return None;
    }
    let first = self
      .entries
      .iter()
      .position(|(version, _)| *version > since)?;
    // Versions increase by one per change, so the log must start right after `since`.
    if self.entries[first].0.get() != since.get() + 1 {
      return None;
    }
    Some(
      self
        .entries
        .iter()
        .skip(first)
        .flat_map(|(_, splices)| splices.iter().copied())
        .collect(),
    )
  }
}

/// Where the lines of `range` ended up after `splices`: the surviving
/// sub-ranges in the new numbering, sorted. Lines inside a splice are gone.
pub fn map_line_range(range: Range<usize>, splices: &[LineSplice]) -> Vec<Range<usize>> {
  let mut ranges = vec![range];
  for splice in splices {
    let removed_end = splice.start + splice.removed;
    // Only lines at or after `removed_end` (>= `removed`) shift, so this cannot underflow.
    let shift = |line: usize| line + splice.inserted - splice.removed;
    let mut next = Vec::with_capacity(ranges.len() + 1);
    for range in ranges {
      // Part before the splice is unchanged.
      if range.start < splice.start {
        next.push(range.start..range.end.min(splice.start));
      }
      // Part after the removed lines moves by the line delta.
      if range.end > removed_end {
        let start = range.start.max(removed_end);
        next.push(shift(start)..shift(range.end));
      }
    }
    ranges = next.into_iter().filter(|range| !range.is_empty()).collect();
  }
  ranges
}

/// `wanted` minus `have` (both sorted, non-overlapping): the lines still needed.
pub fn missing_ranges(wanted: Range<usize>, have: &[Range<usize>]) -> Vec<Range<usize>> {
  let mut missing = Vec::new();
  let mut cursor = wanted.start;
  for range in have {
    if range.end <= cursor || range.start >= wanted.end {
      continue;
    }
    if range.start > cursor {
      missing.push(cursor..range.start.min(wanted.end));
    }
    cursor = cursor.max(range.end);
    if cursor >= wanted.end {
      break;
    }
  }
  if cursor < wanted.end {
    missing.push(cursor..wanted.end);
  }
  missing
}

#[cfg(test)]
mod tests {
  use super::*;

  fn splice(start: usize, removed: usize, inserted: usize) -> LineSplice {
    LineSplice {
      start,
      removed,
      inserted,
    }
  }

  #[test]
  fn lines_inside_a_splice_are_dropped_and_later_lines_shift() {
    // Line 5 was edited and split into two lines.
    assert_eq!(map_line_range(0..10, &[splice(5, 1, 2)]), vec![0..5, 7..11]);
    // Lines 2..4 were deleted.
    assert_eq!(map_line_range(0..10, &[splice(2, 3, 1)]), vec![0..2, 3..8]);
    // A splice after the range changes nothing.
    assert_eq!(map_line_range(0..3, &[splice(5, 1, 4)]), vec![0..3]);
  }

  #[test]
  fn splices_apply_in_sequence() {
    let splices = [splice(1, 1, 3), splice(8, 2, 1)];
    // After the first: 0..1, 4..12 (old 2..10). The second drops 8..10 and shifts 10..12 to 9..11.
    assert_eq!(map_line_range(0..10, &splices), vec![0..1, 4..8, 9..11]);
  }

  #[test]
  #[allow(clippy::single_range_in_vec_init)]
  fn missing_ranges_fill_the_gaps() {
    assert_eq!(missing_ranges(0..10, &[2..4, 6..20]), vec![0..2, 4..6]);
    assert_eq!(missing_ranges(5..8, &[0..10]), Vec::<Range<usize>>::new());
    assert_eq!(missing_ranges(5..8, &[]), vec![5..8]);
    assert_eq!(missing_ranges(5..8, &[0..2, 9..12]), vec![5..8]);
  }

  #[test]
  fn the_log_reports_complete_histories_only() {
    let mut log = LineLog::default();
    let v = |n| {
      let mut version = Version::INITIAL;
      for _ in 0..n {
        version = version.next();
      }
      version
    };
    log.push(v(1), vec![splice(0, 1, 1)]);
    log.push(v(2), vec![splice(3, 1, 2)]);
    assert_eq!(
      log.since(v(0), v(2)),
      Some(vec![splice(0, 1, 1), splice(3, 1, 2)])
    );
    assert_eq!(log.since(v(1), v(2)), Some(vec![splice(3, 1, 2)]));
    assert_eq!(log.since(v(2), v(2)), Some(vec![]));
    assert_eq!(log.since(v(3), v(2)), None);

    for n in 3..(3 + LOG_CAPACITY as u64) {
      log.push(v(n), vec![]);
    }
    assert_eq!(log.since(v(0), v(2 + LOG_CAPACITY as u64)), None, "evicted");
  }
}
