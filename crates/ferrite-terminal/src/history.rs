//! Recent output, kept so a terminal can be shown again after the window
//! reloads.

/// The most recent output, at most `limit` bytes (plus a quarter while it
/// waits to be trimmed, so trimming isn't a copy on every chunk).
#[derive(Debug)]
pub struct History {
  text: String,
  limit: usize,
}

/// How far past the cut point to look for a line break to trim at.
const LINE_SEARCH: usize = 4096;

impl History {
  pub fn new(limit: usize) -> Self {
    Self {
      text: String::new(),
      limit,
    }
  }

  pub fn push(&mut self, output: &str) {
    self.text.push_str(output);
    if self.text.len() <= self.limit + self.limit / 4 {
      return;
    }
    // Trim at a line break when there's one nearby, so replay is less likely
    // to start inside an escape sequence; otherwise at a character boundary.
    let mut cut = self.text.len() - self.limit;
    let window = &self.text.as_bytes()[cut..(cut + LINE_SEARCH).min(self.text.len())];
    if let Some(newline) = window.iter().position(|byte| *byte == b'\n') {
      cut += newline + 1;
    } else {
      while !self.text.is_char_boundary(cut) {
        cut += 1;
      }
    }
    self.text.drain(..cut);
  }

  pub fn text(&self) -> &str {
    &self.text
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keeps_everything_under_the_limit() {
    let mut history = History::new(100);
    history.push("one\r\n");
    history.push("two\r\n");
    assert_eq!(history.text(), "one\r\ntwo\r\n");
  }

  #[test]
  fn trims_old_output_at_a_line_break() {
    let mut history = History::new(20);
    for line in 0..10 {
      history.push(&format!("line {line}\r\n"));
    }
    assert!(history.text().len() <= 25);
    assert!(history.text().starts_with("line "), "{:?}", history.text());
    assert!(history.text().ends_with("line 9\r\n"));
  }

  #[test]
  fn trims_long_lines_at_a_character_boundary() {
    let mut history = History::new(10);
    history.push(&"€".repeat(LINE_SEARCH));
    assert!(history.text().len() <= 12);
    assert!(history.text().chars().all(|c| c == '€'));
  }
}
