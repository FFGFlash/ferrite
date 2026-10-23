use std::ops::Range;

/// Replaces the bytes in `range` (pre-edit coordinates) with `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
  pub range: Range<usize>,
  pub text: String,
}

impl Edit {
  pub fn new(range: Range<usize>, text: impl Into<String>) -> Self {
    Self {
      range,
      text: text.into(),
    }
  }

  pub fn insert(offset: usize, text: impl Into<String>) -> Self {
    Self::new(offset..offset, text)
  }

  pub fn delete(range: Range<usize>) -> Self {
    Self::new(range, String::new())
  }

  pub fn is_noop(&self) -> bool {
    self.range.is_empty() && self.text.is_empty()
  }
}

/// Why an edit was rejected. Rejected change sets leave the buffer untouched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
  #[error("edit {index} has an inverted range {start}..{end}")]
  InvertedRange {
    index: usize,
    start: usize,
    end: usize,
  },
  #[error("edit {index} overlaps or precedes the previous edit")]
  Unordered { index: usize },
  #[error("offset {offset} is beyond the end of the text ({len} bytes)")]
  OutOfBounds { offset: usize, len: usize },
  #[error("offset {offset} is not on a character boundary")]
  NotCharBoundary { offset: usize },
}

/// A set of edits applied as one atomic change.
///
/// Edits are expressed in the coordinates of the text *before* the change,
/// sorted by position, and must not overlap (they may touch). Several
/// insertions at the same offset are inserted in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeSet {
  edits: Vec<Edit>,
}

impl ChangeSet {
  pub fn new(edits: Vec<Edit>) -> Result<Self, EditError> {
    let mut previous_end = 0;
    for (index, edit) in edits.iter().enumerate() {
      let Range { start, end } = edit.range;
      if start > end {
        return Err(EditError::InvertedRange { index, start, end });
      }
      if start < previous_end {
        return Err(EditError::Unordered { index });
      }
      previous_end = end;
    }
    Ok(Self { edits })
  }

  pub fn single(edit: Edit) -> Result<Self, EditError> {
    Self::new(vec![edit])
  }

  /// Builds a change set whose ordering is already guaranteed by construction.
  pub(crate) fn from_sorted(edits: Vec<Edit>) -> Self {
    debug_assert!(Self::new(edits.clone()).is_ok());
    Self { edits }
  }

  pub fn edits(&self) -> &[Edit] {
    &self.edits
  }

  /// True when applying the change set would not modify the text.
  pub fn is_empty(&self) -> bool {
    self.edits.iter().all(Edit::is_noop)
  }

  /// The range each edit's inserted text occupies after the change is applied,
  /// in the same order as [`Self::edits`].
  pub fn post_ranges(&self) -> Vec<Range<usize>> {
    let (mut added, mut removed) = (0, 0);
    self
      .edits
      .iter()
      .map(|edit| {
        let start = edit.range.start - removed + added;
        removed += edit.range.len();
        added += edit.text.len();
        start..start + edit.text.len()
      })
      .collect()
  }

  /// Bytes of text carried by this change set; used for history accounting.
  pub fn text_bytes(&self) -> usize {
    self.edits.iter().map(|edit| edit.text.len()).sum()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn accepts_sorted_touching_edits() {
    let changes = ChangeSet::new(vec![
      Edit::new(0..2, "a"),
      Edit::insert(2, "b"),
      Edit::insert(2, "c"),
      Edit::delete(2..4),
    ]);
    assert!(changes.is_ok());
  }

  #[test]
  #[allow(clippy::reversed_empty_ranges)]
  fn rejects_overlapping_and_inverted_edits() {
    assert_eq!(
      ChangeSet::new(vec![Edit::delete(0..3), Edit::delete(2..4)]),
      Err(EditError::Unordered { index: 1 })
    );
    assert_eq!(
      ChangeSet::new(vec![Edit::delete(5..2)]),
      Err(EditError::InvertedRange {
        index: 0,
        start: 5,
        end: 2
      })
    );
  }

  #[test]
  fn post_ranges_account_for_earlier_edits() {
    let changes = ChangeSet::new(vec![
      Edit::new(0..3, "x"),
      Edit::insert(5, "yy"),
      Edit::delete(6..8),
    ])
    .expect("valid");
    assert_eq!(changes.post_ranges(), vec![0..1, 3..5, 6..6]);
  }

  #[test]
  fn noop_edits_make_an_empty_change_set() {
    let changes = ChangeSet::new(vec![Edit::insert(3, ""), Edit::delete(4..4)]).expect("valid");
    assert!(changes.is_empty());
    assert!(ChangeSet::default().is_empty());
  }
}
