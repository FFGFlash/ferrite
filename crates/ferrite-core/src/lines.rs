//! Line operations: inserting, moving, copying, and deleting whole lines,
//! indentation, comments, and line selection. Most work on the blocks of
//! lines the selections touch.

use std::ops::Range;

use ferrite_text::{Bias, ChangeSet, Edit, normalize_line_endings};

use crate::document::Document;
use crate::error::CoreError;
use crate::history::EditKind;
use crate::ops::VerticalDirection;
use crate::selection::{Selection, Selections};

/// A language's comment syntax, for toggling comments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentTokens {
  /// Starts a comment that runs to the end of the line, e.g. `//`.
  pub line: Option<String>,
  /// Open and close a block comment, e.g. `/*` and `*/`.
  pub block: Option<(String, String)>,
}

/// One block's rewrite: `edit` changes the text around the block, and each
/// piece of old text in `moved` ends up `landing` bytes into the edit's new
/// text. The block comes first; selections in it move with it. Folded
/// regions move with their piece; others in the edit are unfolded.
struct Rewrite {
  edit: Edit,
  moved: Vec<(Range<usize>, usize)>,
}

impl Document {
  /// The lines a selection covers. A selection ending at the start of a line
  /// doesn't cover that line; one ending on the first line of a folded
  /// region covers the whole region.
  pub(crate) fn selection_lines(&self, s: Selection) -> Range<usize> {
    let first = self.buffer.line_of(s.start());
    let mut last = self.buffer.line_of(s.end());
    if last > first && s.end() == self.buffer.line_start(last) {
      last -= 1;
    }
    first..self.through_fold(last) + 1
  }

  /// The lines the selections cover, as sorted blocks; overlapping and
  /// adjacent blocks are merged.
  fn line_blocks(&self) -> Vec<Range<usize>> {
    let mut blocks: Vec<Range<usize>> = Vec::new();
    for &s in self.selections.iter() {
      let lines = self.selection_lines(s);
      match blocks.last_mut() {
        Some(last) if lines.start <= last.end => last.end = last.end.max(lines.end),
        _ => blocks.push(lines),
      }
    }
    blocks
  }

  /// The block in `blocks` containing a selection.
  fn block_of(&self, blocks: &[Range<usize>], s: Selection) -> usize {
    let line = self.buffer.line_of(s.start());
    blocks.partition_point(|block| block.end <= line)
  }

  /// Where `line` ends, including its line break.
  fn full_line_end(&self, line: usize) -> usize {
    if line + 1 < self.buffer.line_count() {
      self.buffer.line_start(line + 1)
    } else {
      self.buffer.len()
    }
  }

  /// Applies `changes` as one undoable edit, leaving the selections `after`
  /// computes from the changed document.
  fn commit(
    &mut self,
    changes: ChangeSet,
    after: impl FnOnce(&Self, &ChangeSet) -> Selections,
  ) -> Result<bool, CoreError> {
    if changes.is_empty() {
      return Ok(false);
    }
    let before = self.selections.clone();
    let inverse = self.buffer.apply(&changes)?;
    let after = after(self, &changes);
    self.selections = after.clone();
    self
      .history
      .record(changes, inverse, before, after, EditKind::Other, false);
    Ok(true)
  }

  /// Applies one rewrite per block, moving each selection with its block.
  fn rewrite_blocks(
    &mut self,
    blocks: &[Range<usize>],
    rewrites: Vec<Rewrite>,
  ) -> Result<bool, CoreError> {
    let moves: Vec<(usize, usize)> = rewrites
      .iter()
      .map(|r| (r.moved[0].0.start, r.moved[0].1))
      .collect();
    let owners: Vec<usize> = self
      .selections
      .iter()
      .map(|&s| self.block_of(blocks, s))
      .collect();
    let changes = ChangeSet::new(rewrites.iter().map(|r| r.edit.clone()).collect())?;
    if changes.is_empty() {
      return Ok(false);
    }
    let post = changes.post_ranges();
    // (rewrite, header, end) of each folded region that moves, relative to
    // the rewrite's new text.
    let mut carried = Vec::new();
    for (index, rewrite) in rewrites.iter().enumerate() {
      for (header, end) in self.folds.take_within(rewrite.edit.range.clone()) {
        if let Some((from, landing)) = rewrite
          .moved
          .iter()
          .find(|(from, _)| from.start <= header && end <= from.end)
        {
          carried.push((
            index,
            header - from.start + landing,
            end - from.start + landing,
          ));
        }
      }
    }
    let changed = self.commit(changes, |document, _| {
      let ranges = document
        .selections
        .iter()
        .zip(&owners)
        .map(|(s, &block)| {
          let (origin, landing) = moves[block];
          let target = &post[block];
          let map = |offset: usize| (target.start + landing + offset - origin).min(target.end);
          Selection::new(map(s.anchor), map(s.head))
        })
        .collect();
      Selections::new(ranges, document.selections.primary_index())
    })?;
    self.folds.sync(&self.buffer);
    let moved = carried
      .into_iter()
      .map(|(index, header, end)| (post[index].start + header, post[index].start + end));
    self.folds.put(&self.buffer, moved);
    Ok(changed)
  }

  /// Inserts a line above or below each selection's lines, indented like
  /// the line next to it, and moves the cursor there.
  pub(crate) fn insert_line(&mut self, direction: VerticalDirection) -> Result<bool, CoreError> {
    let buffer = &self.buffer;
    let ending = self.line_ending.as_str();
    let targets: Vec<usize> = self
      .selections
      .iter()
      .map(|&s| {
        let lines = self.selection_lines(s);
        match direction {
          VerticalDirection::Up => lines.start,
          VerticalDirection::Down => lines.end - 1,
        }
      })
      .collect();
    let mut lines = targets.clone();
    lines.dedup();
    let mut cursors = Vec::with_capacity(lines.len());
    let edits = lines
      .iter()
      .map(|&line| {
        let start = buffer.line_start(line);
        let indent: String = buffer
          .chars_at(start)
          .take_while(|&c| c == ' ' || c == '\t')
          .collect();
        match direction {
          VerticalDirection::Up => {
            cursors.push(indent.len());
            Edit::insert(start, format!("{indent}{ending}"))
          }
          VerticalDirection::Down => {
            cursors.push(ending.len() + indent.len());
            Edit::insert(buffer.line_end(line), format!("{ending}{indent}"))
          }
        }
      })
      .collect();
    let changes = ChangeSet::new(edits)?;
    let post = changes.post_ranges();
    self.commit(changes, |document, _| {
      let ranges = targets
        .iter()
        .map(|line| {
          let index = lines.binary_search(line).unwrap_or_default();
          Selection::cursor(post[index].start + cursors[index])
        })
        .collect();
      Selections::new(ranges, document.selections.primary_index())
    })
  }

  /// Swaps each block of selected lines with the line above or below it (a
  /// folded region and its first line count as one line). Does nothing if a
  /// block is already at the top or bottom.
  pub(crate) fn move_lines(&mut self, direction: VerticalDirection) -> Result<bool, CoreError> {
    let blocks = self.line_blocks();
    let buffer = &self.buffer;
    let line_count = buffer.line_count();
    let at_edge = match direction {
      VerticalDirection::Up => blocks[0].start == 0,
      VerticalDirection::Down => blocks[blocks.len() - 1].end >= line_count,
    };
    if at_edge {
      return Ok(false);
    }
    let rewrites = blocks
      .iter()
      .map(|lines| {
        let (first, last) = (lines.start, lines.end - 1);
        let start = buffer.line_start(first);
        match direction {
          VerticalDirection::Up => {
            let above_line = self
              .folds
              .hidden_at(first - 1)
              .map_or(first - 1, |hidden| hidden.start - 1);
            let above = buffer.line_start(above_line);
            let end = self.full_line_end(last);
            let mut text = buffer.text_in(start..end);
            let neighbour;
            if last + 1 < line_count {
              neighbour = (above..start, text.len());
              text += &buffer.text_in(above..start);
            } else {
              // The last line has no line break: take the one above.
              let above_end = buffer.line_end(first - 1);
              text += &buffer.text_in(above_end..start);
              neighbour = (above..above_end, text.len());
              text += &buffer.text_in(above..above_end);
            }
            Rewrite {
              edit: Edit::new(above..end, text),
              moved: vec![(start..end, 0), neighbour],
            }
          }
          VerticalDirection::Down => {
            let below_last = self.through_fold(last + 1);
            let below = buffer.line_start(last + 1);
            let end = self.full_line_end(below_last);
            let mut text = buffer.text_in(below..end);
            let block;
            if below_last + 1 < line_count {
              block = (start..below, text.len());
              text += &buffer.text_in(start..below);
            } else {
              // The line below is the last and has no line break: take ours.
              let block_end = buffer.line_end(last);
              text += &buffer.text_in(block_end..below);
              block = (start..block_end, text.len());
              text += &buffer.text_in(start..block_end);
            }
            Rewrite {
              edit: Edit::new(start..end, text),
              moved: vec![block, (below..end, 0)],
            }
          }
        }
      })
      .collect();
    self.rewrite_blocks(&blocks, rewrites)
  }

  /// Duplicates each block of selected lines. Copying down moves the
  /// selections to the copy; copying up leaves them on the upper copy.
  pub(crate) fn copy_lines(&mut self, direction: VerticalDirection) -> Result<bool, CoreError> {
    let blocks = self.line_blocks();
    let buffer = &self.buffer;
    let ending = self.line_ending.as_str();
    let rewrites = blocks
      .iter()
      .map(|lines| {
        let start = buffer.line_start(lines.start);
        let end = self.full_line_end(lines.end - 1);
        let block = buffer.text_in(start..end);
        let has_break = lines.end < buffer.line_count();
        let (edit, landing) = match (direction, has_break) {
          (VerticalDirection::Up, true) => (Edit::insert(start, block), 0),
          (VerticalDirection::Up, false) => (Edit::insert(start, block + ending), 0),
          (VerticalDirection::Down, true) => (Edit::insert(end, block), 0),
          (VerticalDirection::Down, false) => {
            (Edit::insert(end, format!("{ending}{block}")), ending.len())
          }
        };
        Rewrite {
          edit,
          moved: vec![(start..end, landing)],
        }
      })
      .collect();
    self.rewrite_blocks(&blocks, rewrites)
  }

  /// Deletes the selected lines, leaving a cursor on the line that takes
  /// their place, in the same column.
  pub(crate) fn delete_lines(&mut self) -> Result<bool, CoreError> {
    let blocks = self.line_blocks();
    let buffer = &self.buffer;
    let mut goals = vec![None; blocks.len()];
    for &s in self.selections.iter() {
      let block = self.block_of(&blocks, s);
      goals[block].get_or_insert_with(|| s.goal.unwrap_or_else(|| self.visual_column(s.head)));
    }
    let edits = blocks
      .iter()
      .map(|lines| {
        let range = if lines.end >= buffer.line_count() && lines.start > 0 {
          // The last line: remove the line break before it instead.
          buffer.line_end(lines.start - 1)..buffer.len()
        } else {
          buffer.line_start(lines.start)..self.full_line_end(lines.end - 1)
        };
        Edit::delete(range)
      })
      .collect();
    let primary = self.block_of(&blocks, self.selections.primary());
    let changes = ChangeSet::new(edits)?;
    let post = changes.post_ranges();
    self.commit(changes, |document, _| {
      let ranges = post
        .iter()
        .zip(&goals)
        .map(|(range, goal)| {
          let goal = goal.unwrap_or_default();
          let line = document.buffer.line_of(range.start);
          Selection {
            goal: Some(goal),
            ..Selection::cursor(document.offset_at_visual_column(line, goal))
          }
        })
        .collect();
      Selections::new(ranges, primary)
    })
  }

  /// The text of every line with a cursor, each ending in a line break, for
  /// copying when nothing is selected.
  pub fn cursor_lines_text(&self) -> String {
    let ending = self.line_ending.as_str();
    let mut text = String::new();
    for lines in self.line_blocks() {
      let start = self.buffer.line_start(lines.start);
      text += &self
        .buffer
        .text_in(start..self.full_line_end(lines.end - 1));
      if lines.end >= self.buffer.line_count() {
        text += ending;
      }
    }
    text
  }

  /// Pastes text copied as whole lines (with [`Self::cursor_lines_text`])
  /// above each cursor's line, leaving the cursors where they were. Any
  /// other paste replaces the selections, like typing.
  pub(crate) fn paste(&mut self, text: &str, whole_lines: bool) -> Result<bool, CoreError> {
    if !whole_lines || self.selections.iter().any(|s| !s.is_empty()) {
      return self.apply(crate::ops::EditorOp::Insert(text.to_owned()));
    }
    let text = normalize_line_endings(text, self.line_ending);
    let mut lines: Vec<usize> = self
      .selections
      .iter()
      .map(|s| self.buffer.line_of(s.head))
      .collect();
    lines.dedup();
    let edits = lines
      .iter()
      .map(|&line| Edit::insert(self.buffer.line_start(line), text.as_ref()))
      .collect();
    let changes = ChangeSet::new(edits)?;
    self.commit(changes, |document, changes| {
      document
        .selections
        .map(|s| Selection::cursor(changes.map_offset(s.head, Bias::After)))
    })
  }

  /// Indents or outdents the selected lines to the next or previous tab
  /// stop. Blank lines in a multi-line block are left alone.
  pub(crate) fn shift_lines(&mut self, outdent: bool) -> Result<bool, CoreError> {
    let tab = self.tab_size;
    let mut edits = Vec::new();
    for lines in self.line_blocks() {
      let several = lines.len() > 1;
      for line in lines {
        let start = self.buffer.line_start(line);
        if several && start == self.buffer.line_end(line) {
          continue;
        }
        let (indent, width) = self.indentation(line);
        let width = match (outdent, width) {
          (true, 0) => continue,
          (true, width) => (width.div_ceil(tab) - 1) * tab,
          (false, width) => (width / tab + 1) * tab,
        };
        let new = if self.insert_spaces {
          " ".repeat(width)
        } else {
          "\t".repeat(width / tab) + &" ".repeat(width % tab)
        };
        let shared = common_prefix(&indent, &new);
        edits.push(Edit::new(
          start + shared..start + indent.len(),
          &new[shared..],
        ));
      }
    }
    self.commit_line_edits(edits)
  }

  /// A line's leading whitespace and its width in columns.
  fn indentation(&self, line: usize) -> (String, usize) {
    let indent: String = self
      .buffer
      .chars_at(self.buffer.line_start(line))
      .take_while(|&c| c == ' ' || c == '\t')
      .collect();
    let width = indent.chars().fold(0, |width, c| match c {
      '\t' => (width / self.tab_size + 1) * self.tab_size,
      _ => width + 1,
    });
    (indent, width)
  }

  /// Applies edits made near the starts of lines. Cursors move with the
  /// text; a selection's end at the start of a line stays there, so a
  /// selection of whole lines keeps covering them.
  fn commit_line_edits(&mut self, edits: Vec<Edit>) -> Result<bool, CoreError> {
    let changes = ChangeSet::new(edits)?;
    let buffer = &self.buffer;
    let bias = |offset: usize| {
      if offset == buffer.line_start(buffer.line_of(offset)) {
        Bias::Before
      } else {
        Bias::After
      }
    };
    let after = self.selections.map(|s| {
      if s.is_empty() {
        Selection::cursor(changes.map_offset(s.head, Bias::After))
      } else {
        Selection::new(
          changes.map_offset(s.anchor, bias(s.anchor)),
          changes.map_offset(s.head, bias(s.head)),
        )
      }
    });
    self.commit(changes, |_, _| after)
  }

  /// Comments out the selected lines, or uncomments them if they all are
  /// comments. Languages without line comments get one block comment per
  /// block of lines.
  pub(crate) fn toggle_comment(&mut self, tokens: &CommentTokens) -> Result<bool, CoreError> {
    match tokens {
      CommentTokens {
        line: Some(token), ..
      } => self.toggle_line_comment(token),
      CommentTokens {
        block: Some((open, close)),
        ..
      } => self.toggle_block_comment(open, close),
      _ => Ok(false),
    }
  }

  fn toggle_line_comment(&mut self, token: &str) -> Result<bool, CoreError> {
    let edits = self.line_comment_edits(token);
    self.commit_line_edits(edits)
  }

  fn line_comment_edits(&self, token: &str) -> Vec<Edit> {
    let blocks = self.line_blocks();
    // Blank lines are skipped unless every line is blank.
    let all_blank = blocks
      .iter()
      .flat_map(Clone::clone)
      .all(|line| self.is_blank(line));
    let candidates = |lines: &Range<usize>| {
      lines
        .clone()
        .filter(move |&line| all_blank || !self.is_blank(line))
    };
    let commented = blocks.iter().flat_map(candidates).all(|line| {
      let text_start = self.buffer.line_start(line) + self.indentation(line).0.len();
      self.text_starts_with(text_start, token)
    });
    let mut edits = Vec::new();
    for lines in &blocks {
      if commented {
        for line in candidates(lines) {
          let start = self.buffer.line_start(line) + self.indentation(line).0.len();
          let mut end = start + token.len();
          if self.text_starts_with(end, " ") {
            end += 1;
          }
          edits.push(Edit::delete(start..end));
        }
      } else {
        // Line the comments up at the block's least indentation.
        let column = candidates(lines)
          .map(|line| self.indentation(line).0.len())
          .min()
          .unwrap_or_default();
        for line in candidates(lines) {
          let start = self.buffer.line_start(line) + column;
          edits.push(Edit::insert(start, format!("{token} ")));
        }
      }
    }
    edits
  }

  fn toggle_block_comment(&mut self, open: &str, close: &str) -> Result<bool, CoreError> {
    let edits = self.block_comment_edits(open, close);
    self.commit_line_edits(edits)
  }

  fn block_comment_edits(&self, open: &str, close: &str) -> Vec<Edit> {
    let mut edits = Vec::new();
    for lines in self.line_blocks() {
      let mut text_lines = lines.filter(|&line| !self.is_blank(line));
      let Some(first) = text_lines.next() else {
        continue;
      };
      let last = text_lines.next_back().unwrap_or(first);
      let start = self.buffer.line_start(first) + self.indentation(first).0.len();
      let end = self.buffer.line_end(last);
      let text = self.buffer.text_in(start..end);
      let trimmed = text.trim_end();
      let end = start + trimmed.len();
      if trimmed.len() >= open.len() + close.len()
        && trimmed.starts_with(open)
        && trimmed.ends_with(close)
      {
        let mut open_end = start + open.len();
        let mut close_start = end - close.len();
        if close_start > open_end && self.text_starts_with(open_end, " ") {
          open_end += 1;
        }
        if close_start > open_end && trimmed[..close_start - start].ends_with(' ') {
          close_start -= 1;
        }
        edits.push(Edit::delete(start..open_end));
        edits.push(Edit::delete(close_start..end));
      } else {
        edits.push(Edit::insert(start, format!("{open} ")));
        edits.push(Edit::insert(end, format!(" {close}")));
      }
    }
    edits
  }

  fn is_blank(&self, line: usize) -> bool {
    let start = self.buffer.line_start(line);
    start + self.indentation(line).0.len() == self.buffer.line_end(line)
  }

  fn text_starts_with(&self, offset: usize, prefix: &str) -> bool {
    let mut chars = self.buffer.chars_at(offset);
    prefix.chars().all(|c| chars.next() == Some(c))
  }

  /// Selects each selection's whole lines, line breaks included. Repeating
  /// it extends the selection by a line.
  pub(crate) fn select_line(&mut self) {
    let selections = self.selections.map(|s| {
      let start = self.buffer.line_start(self.buffer.line_of(s.start()));
      let last = self.through_fold(self.buffer.line_of(s.end()));
      Selection::new(start, self.full_line_end(last))
    });
    self.set_selections(selections);
  }

  /// Replaces each selection with a cursor at the end of every line it
  /// covers.
  pub(crate) fn cursors_at_line_ends(&mut self) {
    let mut ranges = Vec::new();
    let mut primary = 0;
    for (index, &s) in self.selections.iter().enumerate() {
      if s.is_empty() {
        ranges.push(s);
      } else {
        let first = self.buffer.line_of(s.start());
        let last = self.buffer.line_of(s.end());
        for line in (first..last).filter(|&line| !self.is_hidden(line)) {
          ranges.push(Selection::cursor(self.buffer.line_end(line)));
        }
        if first == last || s.end() > self.buffer.line_start(last) {
          ranges.push(Selection::cursor(s.end()));
        }
      }
      if index == self.selections.primary_index() {
        primary = ranges.len() - 1;
      }
    }
    self.set_selections(Selections::new(ranges, primary));
  }
}

/// The length of the longest common prefix of two ASCII-whitespace strings.
fn common_prefix(a: &str, b: &str) -> usize {
  a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ops::EditorOp;
  use crate::testing::{doc, run};

  const UP: VerticalDirection = VerticalDirection::Up;
  const DOWN: VerticalDirection = VerticalDirection::Down;

  #[test]
  fn insert_line_below_and_above_keeps_indentation() {
    assert_eq!(
      run(&mut doc("  fo|o\nbar"), [EditorOp::InsertLine(DOWN)]),
      "  foo\n  |\nbar"
    );
    assert_eq!(
      run(&mut doc("a\n\tfo|o"), [EditorOp::InsertLine(UP)]),
      "a\n\t|\n\tfoo"
    );
    // Two cursors on one line insert one line.
    assert_eq!(
      run(&mut doc("a|b|c"), [EditorOp::InsertLine(DOWN)]),
      "abc\n|"
    );
    assert_eq!(
      run(&mut doc("a|b\r\nc"), [EditorOp::InsertLine(UP)]),
      "|\r\nab\r\nc"
    );
  }

  #[test]
  fn move_lines_swaps_with_the_neighbouring_line() {
    assert_eq!(
      run(&mut doc("a\nb|\nc"), [EditorOp::MoveLines(UP)]),
      "b|\na\nc"
    );
    assert_eq!(
      run(&mut doc("a\nb|\nc"), [EditorOp::MoveLines(DOWN)]),
      "a\nc\nb|"
    );
    assert_eq!(
      run(&mut doc("a\nb\nc|"), [EditorOp::MoveLines(UP)]),
      "a\nc|\nb"
    );
    assert_eq!(
      run(&mut doc("a|\nb\nc"), [EditorOp::MoveLines(UP)]),
      "a|\nb\nc"
    );
    assert_eq!(
      run(&mut doc("a\nb\nc|"), [EditorOp::MoveLines(DOWN)]),
      "a\nb\nc|"
    );
  }

  #[test]
  fn move_lines_moves_whole_selected_blocks() {
    // A selection ending at the start of a line doesn't take that line.
    assert_eq!(
      run(&mut doc("x\n[a\nb\n]c"), [EditorOp::MoveLines(UP)]),
      "[a\nb\n]x\nc"
    );
    assert_eq!(
      run(&mut doc("a|\nb|\nc\nd"), [EditorOp::MoveLines(DOWN)]),
      "c\na|\nb|\nd"
    );
    // Separate blocks move independently.
    assert_eq!(
      run(&mut doc("a\nb|\nc\nd|"), [EditorOp::MoveLines(UP)]),
      "b|\na\nd|\nc"
    );
    let mut document = doc("a\nb|\nc");
    run(&mut document, [EditorOp::MoveLines(DOWN)]);
    assert_eq!(run(&mut document, [EditorOp::Undo]), "a\nb|\nc");
  }

  #[test]
  fn copy_lines_duplicates_the_block() {
    assert_eq!(
      run(&mut doc("a|b\nc"), [EditorOp::CopyLines(DOWN)]),
      "ab\na|b\nc"
    );
    assert_eq!(
      run(&mut doc("a|b\nc"), [EditorOp::CopyLines(UP)]),
      "a|b\nab\nc"
    );
    assert_eq!(
      run(&mut doc("x\na|b"), [EditorOp::CopyLines(DOWN)]),
      "x\nab\na|b"
    );
    assert_eq!(
      run(&mut doc("x\na|b"), [EditorOp::CopyLines(UP)]),
      "x\na|b\nab"
    );
    assert_eq!(
      run(&mut doc("[a\nb]\nc"), [EditorOp::CopyLines(DOWN)]),
      "a\nb\n[a\nb]\nc"
    );
  }

  #[test]
  fn delete_lines_keeps_the_column() {
    assert_eq!(
      run(&mut doc("abc\nde|f\nghi"), [EditorOp::DeleteLines]),
      "abc\ngh|i"
    );
    assert_eq!(run(&mut doc("abc\nde|f"), [EditorOp::DeleteLines]), "ab|c");
    assert_eq!(run(&mut doc("on|ly"), [EditorOp::DeleteLines]), "|");
    assert_eq!(
      run(&mut doc("[a\nb]\nc\nd|"), [EditorOp::DeleteLines]),
      "c|"
    );
  }

  #[test]
  fn copying_without_a_selection_takes_whole_lines() {
    assert_eq!(doc("a\nb|\nc").cursor_lines_text(), "b\n");
    assert_eq!(doc("a\nb|").cursor_lines_text(), "b\n");
    assert_eq!(doc("a|\nb\nc|").cursor_lines_text(), "a\nc\n");
    assert_eq!(doc("x\r\n|").cursor_lines_text(), "\r\n");
  }

  #[test]
  fn pasting_whole_lines_inserts_above_the_cursor() {
    let paste = |text: &str, whole_lines| EditorOp::Paste {
      text: text.into(),
      whole_lines,
    };
    assert_eq!(run(&mut doc("a\nb|c"), [paste("x\n", true)]), "a\nx\nb|c");
    assert_eq!(run(&mut doc("a\nb|c"), [paste("x\n", false)]), "a\nbx\n|c");
    assert_eq!(
      run(&mut doc("a\r\n[b]c"), [paste("x\n", true)]),
      "a\r\nx\r\n|c"
    );
  }

  #[test]
  fn tab_and_shift_lines_indent_to_tab_stops() {
    assert_eq!(
      run(&mut doc("[a\n  b\n\nc]"), [EditorOp::Tab]),
      "[    a\n    b\n\n    c]"
    );
    assert_eq!(run(&mut doc("  a|"), [EditorOp::Indent]), "    a|");
    assert_eq!(run(&mut doc("|a"), [EditorOp::Indent]), "    |a");
    assert_eq!(
      run(&mut doc("      a|\n b|"), [EditorOp::Outdent]),
      "    a|\nb|"
    );
    assert_eq!(run(&mut doc("a|"), [EditorOp::Outdent]), "a|");
    // A single-line selection is replaced, as typing would.
    assert_eq!(run(&mut doc("[ab]c"), [EditorOp::Tab]), "    |c");
  }

  #[test]
  fn shifting_lines_with_tabs() {
    let mut document = doc("\ta|\n  b|");
    document.insert_spaces = false;
    assert_eq!(run(&mut document, [EditorOp::Indent]), "\t\ta|\n\tb|");
    assert_eq!(
      run(&mut document, [EditorOp::Outdent, EditorOp::Outdent]),
      "a|\nb|"
    );
  }

  #[test]
  fn toggling_line_comments() {
    let tokens = CommentTokens {
      line: Some("//".into()),
      block: None,
    };
    let toggle = || EditorOp::ToggleComment(tokens.clone());
    let mut document = doc("[  a\n\n    b\n]c");
    assert_eq!(run(&mut document, [toggle()]), "[  // a\n\n  //   b\n]c");
    assert_eq!(run(&mut document, [toggle()]), "[  a\n\n    b\n]c");
    // Mixed lines are commented, not uncommented.
    assert_eq!(run(&mut doc("[// a\nb]"), [toggle()]), "[// // a\n// b]");
    assert_eq!(run(&mut doc("x|"), [toggle()]), "// x|");
    assert_eq!(run(&mut doc("//x|"), [toggle()]), "x|");
  }

  #[test]
  fn falling_back_to_block_comments() {
    let tokens = CommentTokens {
      line: None,
      block: Some(("/*".into(), "*/".into())),
    };
    let toggle = || EditorOp::ToggleComment(tokens.clone());
    let mut document = doc("  a {|\n  }");
    run(&mut document, [EditorOp::SelectLine, EditorOp::SelectLine]);
    assert_eq!(run(&mut document, [toggle()]), "[  /* a {\n  } */]");
    assert_eq!(run(&mut document, [toggle()]), "[  a {\n  }]");
    assert!(
      !doc("x|")
        .apply(EditorOp::ToggleComment(CommentTokens::default()))
        .expect("applies")
    );
  }

  #[test]
  fn select_line_extends_line_by_line() {
    let mut document = doc("a\nb|c\nd");
    assert_eq!(run(&mut document, [EditorOp::SelectLine]), "a\n[bc\n]d");
    assert_eq!(run(&mut document, [EditorOp::SelectLine]), "a\n[bc\nd]");
    assert_eq!(run(&mut document, [EditorOp::SelectLine]), "a\n[bc\nd]");
  }

  #[test]
  fn cursors_at_line_ends_of_selections() {
    assert_eq!(
      run(&mut doc("a[b\ncd\ne]f"), [EditorOp::CursorsAtLineEnds]),
      "ab|\ncd|\ne|f"
    );
    assert_eq!(
      run(&mut doc("[ab\n]cd"), [EditorOp::CursorsAtLineEnds]),
      "ab|\ncd"
    );
    assert_eq!(
      run(&mut doc("a[b]c"), [EditorOp::CursorsAtLineEnds]),
      "ab|c"
    );
  }
}
