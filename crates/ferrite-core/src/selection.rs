use std::ops::Range;

/// The text between `anchor` (where the selection started) and `head` (where
/// the cursor is). Empty when they are equal. Offsets are UTF-8 bytes.
#[derive(Debug, Clone, Copy)]
pub struct Selection {
  pub anchor: usize,
  pub head: usize,
  /// The visual column that vertical movement aims for, kept across
  /// consecutive up/down moves through shorter lines.
  pub goal: Option<usize>,
}

/// Selections compare by range only; the goal column is transient.
impl PartialEq for Selection {
  fn eq(&self, other: &Self) -> bool {
    self.anchor == other.anchor && self.head == other.head
  }
}

impl Eq for Selection {}

impl Selection {
  pub fn new(anchor: usize, head: usize) -> Self {
    Self {
      anchor,
      head,
      goal: None,
    }
  }

  pub fn cursor(offset: usize) -> Self {
    Self::new(offset, offset)
  }

  pub fn start(&self) -> usize {
    self.anchor.min(self.head)
  }

  pub fn end(&self) -> usize {
    self.anchor.max(self.head)
  }

  pub fn range(&self) -> Range<usize> {
    self.start()..self.end()
  }

  pub fn is_empty(&self) -> bool {
    self.anchor == self.head
  }

  /// Moves the head to `head`, keeping the anchor when extending.
  pub fn moved_to(self, head: usize, extend: bool) -> Self {
    Self::new(if extend { self.anchor } else { head }, head)
  }
}

/// One or more selections, sorted by position and non-overlapping, with one
/// designated primary selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selections {
  ranges: Vec<Selection>,
  primary: usize,
}

impl Selections {
  pub fn single(selection: Selection) -> Self {
    Self {
      ranges: vec![selection],
      primary: 0,
    }
  }

  pub fn cursor(offset: usize) -> Self {
    Self::single(Selection::cursor(offset))
  }

  /// Sorts `ranges` and merges overlapping selections. `primary` indexes into
  /// `ranges`; the selection it ends up merged into becomes primary.
  pub fn new(ranges: Vec<Selection>, primary: usize) -> Self {
    if ranges.is_empty() {
      return Self::cursor(0);
    }
    let mut order: Vec<usize> = (0..ranges.len()).collect();
    order.sort_by_key(|&i| (ranges[i].start(), ranges[i].end()));

    let mut merged: Vec<Selection> = Vec::with_capacity(ranges.len());
    let mut new_primary = 0;
    for index in order {
      let next = ranges[index];
      match merged.last_mut() {
        Some(last) if overlaps(last, &next) => {
          let (start, end) = (last.start(), last.end().max(next.end()));
          let forward = last.head >= last.anchor;
          *last = if forward {
            Selection::new(start, end)
          } else {
            Selection::new(end, start)
          };
        }
        _ => merged.push(next),
      }
      if index == primary.min(ranges.len() - 1) {
        new_primary = merged.len() - 1;
      }
    }
    Self {
      ranges: merged,
      primary: new_primary,
    }
  }

  pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Selection> + ExactSizeIterator {
    self.ranges.iter()
  }

  pub fn as_slice(&self) -> &[Selection] {
    &self.ranges
  }

  pub fn len(&self) -> usize {
    self.ranges.len()
  }

  /// Always false: there is at least one selection.
  pub fn is_empty(&self) -> bool {
    false
  }

  pub fn primary(&self) -> Selection {
    self.ranges[self.primary]
  }

  pub fn primary_index(&self) -> usize {
    self.primary
  }

  /// Applies `f` to every selection, then re-normalizes.
  pub fn map(&self, f: impl FnMut(Selection) -> Selection) -> Self {
    Self::new(self.ranges.iter().copied().map(f).collect(), self.primary)
  }

  /// Adds a selection and makes it primary.
  pub fn with_added(&self, selection: Selection) -> Self {
    let mut ranges = self.ranges.clone();
    ranges.push(selection);
    let primary = ranges.len() - 1;
    Self::new(ranges, primary)
  }
}

/// Whether `next` (which starts at or after `last`) must merge with `last`:
/// they overlap, start at the same place, or `next` is a cursor at `last`'s end.
fn overlaps(last: &Selection, next: &Selection) -> bool {
  next.start() < last.end()
    || next.start() == last.start()
    || (next.is_empty() && next.start() == last.end())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sorts_and_tracks_the_primary_selection() {
    let selections = Selections::new(
      vec![
        Selection::cursor(9),
        Selection::cursor(1),
        Selection::cursor(5),
      ],
      0,
    );
    let heads: Vec<_> = selections.iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![1, 5, 9]);
    assert_eq!(selections.primary().head, 9);
  }

  #[test]
  fn merges_overlapping_and_duplicate_selections() {
    let selections = Selections::new(
      vec![
        Selection::new(0, 4),
        Selection::new(6, 2),
        Selection::cursor(10),
        Selection::cursor(10),
        Selection::new(12, 14),
        Selection::cursor(14),
      ],
      1,
    );
    assert_eq!(
      selections.as_slice(),
      &[
        Selection::new(0, 6),
        Selection::cursor(10),
        Selection::new(12, 14)
      ]
    );
    assert_eq!(selections.primary_index(), 0);
  }

  #[test]
  fn keeps_touching_non_empty_selections_apart() {
    let selections = Selections::new(vec![Selection::new(0, 3), Selection::new(3, 5)], 0);
    assert_eq!(selections.len(), 2);
  }

  #[test]
  fn goal_columns_do_not_affect_equality() {
    let mut with_goal = Selection::cursor(3);
    with_goal.goal = Some(7);
    assert_eq!(with_goal, Selection::cursor(3));
  }
}
