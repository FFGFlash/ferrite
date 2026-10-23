//! The editor protocol: JSON shapes exchanged with the frontend, and their
//! conversion to and from core types.
//!
//! Positions on the wire are line plus UTF-16 column, so the frontend can use
//! JavaScript string indices directly. They are converted to byte offsets
//! against the document as it is when each operation is applied, and clamped
//! to valid positions.

use ferrite_core::{Direction, Document, EditorOp, Motion, Unit, VerticalDirection};
use ferrite_text::{TextBuffer, Utf16Position};
use serde::{Deserialize, Serialize};

/// Most lines returned in one view, regardless of the requested viewport.
pub const MAX_VIEW_LINES: usize = 1_000;
/// Most selections returned in one view.
pub const MAX_VIEW_SELECTIONS: usize = 10_000;
/// Most operations accepted in one batch.
pub const MAX_OPS_PER_BATCH: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
  pub line: usize,
  pub column: usize,
}

impl From<Utf16Position> for Position {
  fn from(position: Utf16Position) -> Self {
    Self {
      line: position.line,
      column: position.column,
    }
  }
}

impl Position {
  fn to_offset(self, buffer: &TextBuffer) -> usize {
    buffer.utf16_to_offset(Utf16Position {
      line: self.line,
      column: self.column,
    })
  }
}

/// Lines `start_line..end_line` (end exclusive) the frontend is displaying.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
  pub start_line: usize,
  pub end_line: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DirectionDto {
  Backward,
  Forward,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnitDto {
  Grapheme,
  Word,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalDto {
  Up,
  Down,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MotionDto {
  Left,
  Right,
  WordLeft,
  WordRight,
  Up,
  Down,
  PageUp,
  PageDown,
  LineStart,
  LineEnd,
  DocumentStart,
  DocumentEnd,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
  tag = "type",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
pub enum OpDto {
  Insert {
    text: String,
  },
  Newline,
  Tab,
  Delete {
    direction: DirectionDto,
    unit: UnitDto,
  },
  DeleteSelection,
  Move {
    motion: MotionDto,
    extend: bool,
    /// Lines per page, for `pageUp` and `pageDown`.
    #[serde(default)]
    page_lines: Option<usize>,
  },
  SelectAll,
  CollapseSelections,
  SetCursor {
    position: Position,
    extend: bool,
    add: bool,
  },
  SelectWord {
    position: Position,
  },
  AddCursor {
    direction: VerticalDto,
  },
  Undo,
  Redo,
}

impl OpDto {
  pub fn into_core(self, buffer: &TextBuffer) -> EditorOp {
    match self {
      Self::Insert { text } => EditorOp::Insert(text),
      Self::Newline => EditorOp::Newline,
      Self::Tab => EditorOp::Tab,
      Self::Delete { direction, unit } => EditorOp::Delete {
        direction: match direction {
          DirectionDto::Backward => Direction::Backward,
          DirectionDto::Forward => Direction::Forward,
        },
        unit: match unit {
          UnitDto::Grapheme => Unit::Grapheme,
          UnitDto::Word => Unit::Word,
        },
      },
      Self::DeleteSelection => EditorOp::DeleteSelection,
      Self::Move {
        motion,
        extend,
        page_lines,
      } => {
        let lines = page_lines.unwrap_or(1).clamp(1, MAX_VIEW_LINES);
        let motion = match motion {
          MotionDto::Left => Motion::Left,
          MotionDto::Right => Motion::Right,
          MotionDto::WordLeft => Motion::WordLeft,
          MotionDto::WordRight => Motion::WordRight,
          MotionDto::Up => Motion::Up,
          MotionDto::Down => Motion::Down,
          MotionDto::PageUp => Motion::PageUp { lines },
          MotionDto::PageDown => Motion::PageDown { lines },
          MotionDto::LineStart => Motion::LineStart,
          MotionDto::LineEnd => Motion::LineEnd,
          MotionDto::DocumentStart => Motion::DocumentStart,
          MotionDto::DocumentEnd => Motion::DocumentEnd,
        };
        EditorOp::Move { motion, extend }
      }
      Self::SelectAll => EditorOp::SelectAll,
      Self::CollapseSelections => EditorOp::CollapseSelections,
      Self::SetCursor {
        position,
        extend,
        add,
      } => EditorOp::SetCursor {
        offset: position.to_offset(buffer),
        extend,
        add,
      },
      Self::SelectWord { position } => EditorOp::SelectWord {
        offset: position.to_offset(buffer),
      },
      Self::AddCursor { direction } => EditorOp::AddCursor(match direction {
        VerticalDto::Up => VerticalDirection::Up,
        VerticalDto::Down => VerticalDirection::Down,
      }),
      Self::Undo => EditorOp::Undo,
      Self::Redo => EditorOp::Redo,
    }
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
  pub id: u64,
  pub title: String,
  pub path: Option<String>,
  pub dirty: bool,
  pub line_ending: &'static str,
  pub encoding: &'static str,
}

impl DocumentSummary {
  pub fn of(document: &Document) -> Self {
    Self {
      id: document.id().get(),
      title: document.title(),
      path: document.path().map(|path| path.display().to_string()),
      dirty: document.is_dirty(),
      line_ending: document.line_ending().label(),
      encoding: if document.has_bom() {
        "UTF-8 with BOM"
      } else {
        "UTF-8"
      },
    }
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionDto {
  pub anchor: Position,
  pub head: Position,
  pub primary: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorDto {
  pub position: Position,
  /// Zero-based visual column (tabs expanded), for display.
  pub visual_column: usize,
}

/// What the frontend needs to render one viewport of a document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorView {
  pub document_id: u64,
  pub version: u64,
  pub line_count: usize,
  pub start_line: usize,
  /// Text of lines `start_line..start_line + lines.len()`, without line breaks.
  pub lines: Vec<String>,
  /// Selections touching the returned lines, plus the primary selection.
  pub selections: Vec<SelectionDto>,
  pub selection_count: usize,
  pub primary: CursorDto,
  pub dirty: bool,
}

impl EditorView {
  pub fn of(document: &Document, viewport: Viewport) -> Self {
    let buffer = document.buffer();
    let line_count = buffer.line_count();
    let start = viewport.start_line.min(line_count - 1);
    let end = viewport
      .end_line
      .clamp(start + 1, line_count)
      .min(start + MAX_VIEW_LINES);

    let selections = document.selections();
    let primary_index = selections.primary_index();
    let visible = selections
      .iter()
      .enumerate()
      .filter(|&(index, selection)| {
        index == primary_index
          || (buffer.line_of(selection.end()) >= start && buffer.line_of(selection.start()) < end)
      })
      .take(MAX_VIEW_SELECTIONS)
      .map(|(index, selection)| SelectionDto {
        anchor: buffer.offset_to_utf16(selection.anchor).into(),
        head: buffer.offset_to_utf16(selection.head).into(),
        primary: index == primary_index,
      })
      .collect();

    let head = selections.primary().head;
    Self {
      document_id: document.id().get(),
      version: document.version().get(),
      line_count,
      start_line: start,
      lines: (start..end).map(|line| buffer.line_text(line)).collect(),
      selections: visible,
      selection_count: selections.len(),
      primary: CursorDto {
        position: buffer.offset_to_utf16(head).into(),
        visual_column: document.visual_column(head),
      },
      dirty: document.is_dirty(),
    }
  }
}

#[cfg(test)]
mod tests {
  use ferrite_core::Editor;

  use super::*;

  fn ops(json: &str) -> Vec<OpDto> {
    serde_json::from_str(json).expect("valid ops")
  }

  #[test]
  fn deserializes_tagged_operations() {
    let parsed = ops(
      r#"[
        {"type": "insert", "text": "hi"},
        {"type": "delete", "direction": "backward", "unit": "word"},
        {"type": "move", "motion": "pageDown", "extend": true, "pageLines": 30},
        {"type": "setCursor", "position": {"line": 1, "column": 2}, "extend": false, "add": true},
        {"type": "addCursor", "direction": "up"},
        {"type": "undo"}
      ]"#,
    );
    let buffer = TextBuffer::from_text("ab\n🦀x");
    let core: Vec<_> = parsed.into_iter().map(|op| op.into_core(&buffer)).collect();
    assert_eq!(core[0], EditorOp::Insert("hi".into()));
    assert_eq!(
      core[2],
      EditorOp::Move {
        motion: Motion::PageDown { lines: 30 },
        extend: true
      }
    );
    // Line 1, UTF-16 column 2 is after the crab: byte 3 + 4.
    assert_eq!(
      core[3],
      EditorOp::SetCursor {
        offset: 7,
        extend: false,
        add: true
      }
    );
    assert_eq!(core[5], EditorOp::Undo);
  }

  #[test]
  fn rejects_unknown_operations() {
    assert!(serde_json::from_str::<Vec<OpDto>>(r#"[{"type": "format"}]"#).is_err());
    assert!(
      serde_json::from_str::<Vec<OpDto>>(
        r#"[{"type": "move", "motion": "sideways", "extend": false}]"#
      )
      .is_err()
    );
  }

  #[test]
  fn views_clamp_the_viewport_and_report_utf16_positions() {
    let mut editor = Editor::default();
    let id = editor.create_untitled();
    let document = editor.document_mut(id).expect("open");
    document
      .apply(EditorOp::Insert("one\ntwo\n🦀3".into()))
      .expect("applies");

    let view = EditorView::of(
      document,
      Viewport {
        start_line: 1,
        end_line: 99,
      },
    );
    assert_eq!(view.lines, vec!["two", "🦀3"]);
    assert_eq!(view.line_count, 3);
    assert_eq!(view.primary.position, Position { line: 2, column: 3 });
    assert_eq!(view.primary.visual_column, 2);
    assert!(view.dirty);

    let view = EditorView::of(
      document,
      Viewport {
        start_line: 50,
        end_line: 10,
      },
    );
    assert_eq!(view.start_line, 2);
    assert_eq!(view.lines.len(), 1);
  }

  #[test]
  fn views_include_only_nearby_selections_and_the_primary() {
    let mut editor = Editor::default();
    let id = editor.create_untitled();
    let document = editor.document_mut(id).expect("open");
    document
      .apply(EditorOp::Insert("a\n".repeat(100)))
      .expect("applies");
    for op in [
      EditorOp::SetCursor {
        offset: 0,
        extend: false,
        add: false,
      },
      EditorOp::SetCursor {
        offset: 100,
        extend: false,
        add: true,
      },
    ] {
      document.apply(op).expect("applies");
    }
    let view = EditorView::of(
      document,
      Viewport {
        start_line: 0,
        end_line: 10,
      },
    );
    assert_eq!(view.selection_count, 2);
    assert_eq!(
      view.selections.len(),
      2,
      "line 0 is visible and line 50 is primary"
    );

    document
      .apply(EditorOp::SetCursor {
        offset: 2,
        extend: false,
        add: true,
      })
      .expect("applies");
    let view = EditorView::of(
      document,
      Viewport {
        start_line: 90,
        end_line: 99,
      },
    );
    assert_eq!(view.selections.len(), 1);
    assert!(view.selections[0].primary);
  }
}
