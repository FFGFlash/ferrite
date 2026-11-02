//! Lines of a file for the preview in a peek view (references): the open
//! document's text if the file is open, otherwise the file on disk, with
//! syntax highlighting. The preview only reads, so nothing is kept between
//! requests.

use std::path::Path;
use std::time::Duration;

use ferrite_core::MAX_DISPLAY_LINE_BYTES;
use ferrite_syntax::{SyntaxTree, deadline, parse};
use ferrite_text::TextBuffer;
use ferrite_workspace::load_text_file;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::EditorState;
use super::syntax::{MAX_SYNTAX_BYTES, highlight_buffer_lines};
use crate::error::CommandError;
use crate::languages::LanguagesState;
use crate::paths;

/// Longest a file that isn't open may take to parse; past it, the lines
/// come without highlighting.
const PARSE_BUDGET: Duration = Duration::from_millis(500);
/// Most lines sent at once.
const MAX_LINES: usize = 2_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeekLineDto {
  pub text: String,
  /// Highlights, flattened as in views.
  pub spans: Vec<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeekLinesDto {
  pub line_count: usize,
  /// The first line sent.
  pub start: usize,
  pub lines: Vec<PeekLineDto>,
}

/// Lines `start..end` of the file at `path` (clamped to the file).
#[tauri::command]
pub async fn editor_peek_lines(
  app: AppHandle,
  path: String,
  start: usize,
  end: usize,
) -> Result<PeekLinesDto, CommandError> {
  let path = paths::normalize(Path::new(&path));
  let open = {
    let state = app.state::<EditorState>();
    let editor = state.lock();
    editor
      .find_by_path(&path)
      .and_then(|id| editor.document(id).ok())
      .map(|document| {
        (
          TextBuffer::from_snapshot(&document.buffer().snapshot()),
          state.syntax.tree(&app, document),
        )
      })
  };
  tauri::async_runtime::spawn_blocking(move || {
    let (buffer, tree) = match open {
      Some(open) => open,
      None => {
        let buffer = TextBuffer::from_text(&load_text_file(&path)?.text);
        let registry = app.state::<LanguagesState>().registry();
        let tree = registry
          .for_path(&path)
          .filter(|_| buffer.len() <= MAX_SYNTAX_BYTES)
          .and_then(|language| {
            parse(language, &registry, &buffer, None, deadline(PARSE_BUDGET))
              .ok()
              .flatten()
          });
        (buffer, tree)
      }
    };
    Ok(lines_of(&buffer, tree.as_ref(), start, end))
  })
  .await
  .map_err(CommandError::internal)?
}

fn lines_of(
  buffer: &TextBuffer,
  tree: Option<&SyntaxTree>,
  start: usize,
  end: usize,
) -> PeekLinesDto {
  let line_count = buffer.line_count();
  let start = start.min(line_count);
  let end = end.clamp(start, line_count).min(start + MAX_LINES);
  let spans = highlight_buffer_lines(tree, buffer, start..end);
  PeekLinesDto {
    line_count,
    start,
    lines: (start..end)
      .zip(spans)
      .map(|(line, spans)| PeekLineDto {
        text: buffer.line_prefix(line, MAX_DISPLAY_LINE_BYTES).0,
        spans,
      })
      .collect(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sends_the_lines_asked_for_within_the_file() {
    let buffer = TextBuffer::from_text("one\ntwo\nthree");
    let preview = lines_of(&buffer, None, 1, 10);
    assert_eq!(preview.line_count, 3);
    assert_eq!(preview.start, 1);
    let texts: Vec<&str> = preview
      .lines
      .iter()
      .map(|line| line.text.as_str())
      .collect();
    assert_eq!(texts, ["two", "three"]);
    assert!(lines_of(&buffer, None, 5, 8).lines.is_empty());
  }
}
