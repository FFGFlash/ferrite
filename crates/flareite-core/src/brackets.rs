//! Jumping between matching brackets. Brackets are matched by scanning the
//! text, so brackets inside strings and comments count too.

use crate::document::Document;
use crate::selection::Selection;

const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// Most characters scanned in each direction looking for a bracket.
const MAX_SCAN_CHARS: usize = 1_000_000;

impl Document {
  /// Moves each cursor to the bracket matching the one next to it, or to
  /// the closing bracket around it, or else to the next bracket.
  pub(crate) fn jump_to_bracket(&mut self) {
    let selections = self
      .selections
      .map(|s| self.bracket_target(s.head).map_or(s, Selection::cursor));
    self.set_selections(selections);
  }

  fn bracket_target(&self, offset: usize) -> Option<usize> {
    let after = self.buffer.chars_at(offset).next();
    let before = self.buffer.chars_at(offset).prev();
    let touching = [
      after.map(|c| (c, offset)),
      before.map(|c| (c, offset - c.len_utf8())),
    ];
    for (c, at) in touching.into_iter().flatten() {
      for pair in PAIRS {
        if c == pair.0 {
          return self.closing_bracket(at + c.len_utf8(), pair);
        }
        if c == pair.1 {
          return self.opening_bracket(at, pair);
        }
      }
    }
    if let Some((at, pair)) = self.enclosing_bracket(offset) {
      return self.closing_bracket(at + 1, pair);
    }
    let mut at = offset;
    for c in self.buffer.chars_at(offset).take(MAX_SCAN_CHARS) {
      if PAIRS.iter().any(|&(open, close)| c == open || c == close) {
        return Some(at);
      }
      at += c.len_utf8();
    }
    None
  }

  /// The start of the bracket closing `pair` that was opened before `from`.
  fn closing_bracket(&self, from: usize, (open, close): (char, char)) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = from;
    for c in self.buffer.chars_at(from).take(MAX_SCAN_CHARS) {
      if c == open {
        depth += 1;
      } else if c == close {
        if depth == 0 {
          return Some(at);
        }
        depth -= 1;
      }
      at += c.len_utf8();
    }
    None
  }

  /// The start of the bracket opening `pair` that is closed at or after `before`.
  fn opening_bracket(&self, before: usize, (open, close): (char, char)) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = before;
    let mut chars = self.buffer.chars_at(before);
    for _ in 0..MAX_SCAN_CHARS {
      let c = chars.prev()?;
      at -= c.len_utf8();
      if c == close {
        depth += 1;
      } else if c == open {
        if depth == 0 {
          return Some(at);
        }
        depth -= 1;
      }
    }
    None
  }

  /// The nearest unclosed opening bracket before `offset`, of any kind.
  fn enclosing_bracket(&self, offset: usize) -> Option<(usize, (char, char))> {
    let mut depths = [0usize; PAIRS.len()];
    let mut at = offset;
    let mut chars = self.buffer.chars_at(offset);
    for _ in 0..MAX_SCAN_CHARS {
      let c = chars.prev()?;
      at -= c.len_utf8();
      for (index, pair) in PAIRS.into_iter().enumerate() {
        if c == pair.1 {
          depths[index] += 1;
        } else if c == pair.0 {
          if depths[index] == 0 {
            return Some((at, pair));
          }
          depths[index] -= 1;
        }
      }
    }
    None
  }
}

#[cfg(test)]
mod tests {
  use crate::ops::EditorOp;
  use crate::testing::{doc, run};

  #[test]
  fn jumps_between_matching_brackets() {
    let mut document = doc("f|(a, (b), c)");
    assert_eq!(
      run(&mut document, [EditorOp::JumpToBracket]),
      "f(a, (b), c|)"
    );
    assert_eq!(
      run(&mut document, [EditorOp::JumpToBracket]),
      "f|(a, (b), c)"
    );
    // Just after a bracket counts as next to it.
    assert_eq!(run(&mut doc("{x}|"), [EditorOp::JumpToBracket]), "|{x}");
    // (`[` and `]` mark selections in these tests.)
    assert_eq!(run(&mut doc("{|x}"), [EditorOp::JumpToBracket]), "{x|}");
  }

  #[test]
  fn jumps_out_of_or_towards_brackets() {
    assert_eq!(
      run(&mut doc("{ a(b) |c (d) }"), [EditorOp::JumpToBracket]),
      "{ a(b) c (d) |}"
    );
    assert_eq!(
      run(&mut doc("a|b (c)"), [EditorOp::JumpToBracket]),
      "ab |(c)"
    );
    assert_eq!(run(&mut doc("a|b"), [EditorOp::JumpToBracket]), "a|b");
    assert_eq!(run(&mut doc("(a|"), [EditorOp::JumpToBracket]), "(a|");
  }
}
