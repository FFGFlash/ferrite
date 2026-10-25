//! The editor protocol: JSON shapes exchanged with the frontend, and their
//! conversion to and from core types.
//!
//! Positions on the wire are line plus UTF-16 column, so the frontend can use
//! JavaScript string indices directly. They are converted to byte offsets
//! against the document as it is when each operation is applied, and clamped
//! to valid positions.

use std::ops::Range;

use ferrite_core::{
  Direction, Document, EditorOp, KnownLines, LineText, Motion, Unit, VerticalDirection,
  ViewportUpdate,
};
use ferrite_text::{SearchQuery, TextBuffer, Utf16Position, Version};
use serde::{Deserialize, Serialize};

use super::syntax::LineHighlightsDto;
use crate::lsp::{DiagnosticDto, LanguageStatusDto};

/// Most lines returned in one view, regardless of the requested viewport.
pub const MAX_VIEW_LINES: usize = 1_000;
/// Most selections returned in one view.
pub const MAX_VIEW_SELECTIONS: usize = 10_000;
/// Most operations accepted in one batch.
pub const MAX_OPS_PER_BATCH: usize = 1_000;
/// Documents up to this size count find matches synchronously; larger ones
/// count on a background worker.
pub const SYNC_COUNT_BYTES: usize = 1024 * 1024;
/// Most match highlights returned for the visible lines.
const MAX_VISIBLE_MATCHES: usize = 5_000;

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
  pub(crate) fn to_offset(self, buffer: &TextBuffer) -> usize {
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
#[serde(rename_all = "camelCase")]
pub struct SearchQueryDto {
  pub pattern: String,
  #[serde(default)]
  pub regex: bool,
  #[serde(default)]
  pub case_sensitive: bool,
  #[serde(default)]
  pub whole_word: bool,
}

impl From<SearchQueryDto> for SearchQuery {
  fn from(query: SearchQueryDto) -> Self {
    Self {
      pattern: query.pattern,
      regex: query.regex,
      case_sensitive: query.case_sensitive,
      whole_word: query.whole_word,
    }
  }
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
  FindNext {
    query: SearchQueryDto,
    backward: bool,
  },
  SelectAllMatches {
    query: SearchQueryDto,
  },
  Replace {
    query: SearchQueryDto,
    replacement: String,
  },
  ReplaceAll {
    query: SearchQueryDto,
    replacement: String,
  },
  AddNextMatch,
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
      Self::FindNext { query, backward } => EditorOp::FindNext {
        query: query.into(),
        backward,
      },
      Self::SelectAllMatches { query } => EditorOp::SelectAllMatches {
        query: query.into(),
      },
      Self::Replace { query, replacement } => EditorOp::Replace {
        query: query.into(),
        replacement,
      },
      Self::ReplaceAll { query, replacement } => EditorOp::ReplaceAll {
        query: query.into(),
        replacement,
      },
      Self::AddNextMatch => EditorOp::AddNextMatch,
    }
  }
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeDto {
  pub start: Position,
  pub end: Position,
}

/// Document-wide matches of one query at one version, counted in the
/// background for large documents.
#[derive(Debug, Clone)]
pub struct MatchCount {
  pub version: Version,
  pub query: SearchQuery,
  pub matches: Vec<Range<usize>>,
  pub truncated: bool,
}

/// Find results for the view's query: highlights for the visible lines and
/// document-wide counts.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchView {
  pub matches: Vec<RangeDto>,
  /// Total matches, or `None` while a background count is running.
  pub total: Option<usize>,
  pub truncated: bool,
  /// Index of the match the primary selection covers, if any.
  pub current: Option<usize>,
  /// Why the query could not be used, e.g. an invalid regular expression.
  pub error: Option<String>,
}

impl SearchView {
  /// Builds the search section for lines `start_line..end_line`. `counted`
  /// holds background results for this document, if any. Also returns
  /// whether a background count is needed.
  pub fn build(
    document: &Document,
    query: &SearchQuery,
    start_line: usize,
    end_line: usize,
    counted: Option<&MatchCount>,
  ) -> (Self, bool) {
    let searcher = match query.compile() {
      Ok(searcher) => searcher,
      Err(error) => {
        let view = Self {
          error: Some(error.to_string()),
          ..Self::default()
        };
        return (view, false);
      }
    };
    let buffer = document.buffer();
    let visible = buffer.line_start(start_line)..buffer.line_end(end_line.saturating_sub(1));
    let to_dto = |range: &Range<usize>| RangeDto {
      start: buffer.offset_to_utf16(range.start).into(),
      end: buffer.offset_to_utf16(range.end).into(),
    };

    if buffer.len() <= SYNC_COUNT_BYTES {
      let summary = document.match_summary(&searcher, visible);
      let view = Self {
        matches: summary.visible.iter().map(to_dto).collect(),
        total: Some(summary.total),
        truncated: summary.truncated,
        current: summary.current,
        error: None,
      };
      return (view, false);
    }

    let matches = searcher
      .find_in_buffer(buffer, visible, MAX_VISIBLE_MATCHES)
      .iter()
      .map(to_dto)
      .collect();
    let fresh =
      counted.filter(|count| count.version == document.version() && count.query == *query);
    let view = match fresh {
      Some(count) => {
        let primary = document.selections().primary().range();
        Self {
          matches,
          total: Some(count.matches.len()),
          truncated: count.truncated,
          current: count
            .matches
            .binary_search_by_key(&primary.start, |found| found.start)
            .ok()
            .filter(|&index| count.matches[index] == primary),
          error: None,
        }
      }
      None => Self {
        matches,
        ..Self::default()
      },
    };
    (view, fresh.is_none())
  }
}

/// Lines the client holds, from its last view.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownDto {
  pub version: u64,
  pub start_line: usize,
  pub end_line: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpliceDto {
  pub start: usize,
  pub removed: usize,
  pub inserted: usize,
}

/// A line's text. Lines cut for display carry their full length.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum LineDto {
  Text(String),
  #[serde(rename_all = "camelCase")]
  Cut {
    text: String,
    full_length: usize,
  },
}

impl From<LineText> for LineDto {
  fn from(line: LineText) -> Self {
    match line.full_utf16_len {
      None => Self::Text(line.text),
      Some(full_length) => Self::Cut {
        text: line.text,
        full_length,
      },
    }
  }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkDto {
  pub start: usize,
  pub lines: Vec<LineDto>,
}

/// How the client updates its lines: replay `splices` on the lines it held
/// at version `base` (or discard them if `base` is `None`), then fill in
/// `chunks`. Afterwards it holds exactly `start_line..end_line` of the view.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinesDto {
  pub base: Option<u64>,
  pub splices: Vec<SpliceDto>,
  pub chunks: Vec<ChunkDto>,
}

impl From<ViewportUpdate> for LinesDto {
  fn from(update: ViewportUpdate) -> Self {
    Self {
      base: update.base.map(Version::get),
      splices: update
        .splices
        .into_iter()
        .map(|splice| SpliceDto {
          start: splice.start,
          removed: splice.removed,
          inserted: splice.inserted,
        })
        .collect(),
      chunks: update
        .chunks
        .into_iter()
        .map(|chunk| ChunkDto {
          start: chunk.start,
          lines: chunk.lines.into_iter().map(LineDto::from).collect(),
        })
        .collect(),
    }
  }
}

/// What the frontend needs to render one viewport of a document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorView {
  pub document_id: u64,
  pub version: u64,
  pub line_count: usize,
  /// The viewport this view describes: lines `start_line..end_line`.
  pub start_line: usize,
  pub end_line: usize,
  pub lines: LinesDto,
  /// Selections touching the viewport, plus the primary selection.
  pub selections: Vec<SelectionDto>,
  pub selection_count: usize,
  pub primary: CursorDto,
  pub dirty: bool,
  /// Present when the request included a find query.
  pub search: Option<SearchView>,
  /// Syntax highlights for view lines whose spans changed. Lines sent in
  /// `lines.chunks` start with no spans; other lines keep theirs.
  pub highlights: Vec<LineHighlightsDto>,
  /// Diagnostics on the view's lines.
  pub diagnostics: Vec<DiagnosticDto>,
  /// The document's language and its server, if it has a known language.
  pub language: Option<LanguageStatusDto>,
  /// Extension decorations on the view's lines.
  pub decorations: Vec<DecorationDto>,
}

/// A range decorated by an extension.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecorationDto {
  pub start: Position,
  pub end: Position,
  /// The decoration type, `<extension id>/<name>`.
  pub key: String,
}

impl EditorView {
  /// The view of `viewport` for a client holding `known`. `search` builds
  /// the search section for the clamped viewport lines, if requested;
  /// `highlights` the highlights for those lines, given the line ranges sent
  /// as chunks.
  pub fn of(
    document: &Document,
    viewport: Viewport,
    known: Option<KnownDto>,
    search: impl FnOnce(usize, usize) -> Option<SearchView>,
    highlights: impl FnOnce(Range<usize>, &[Range<usize>]) -> Vec<LineHighlightsDto>,
  ) -> Self {
    let buffer = document.buffer();
    let known = known.map(|known| KnownLines {
      version: Version::from_raw(known.version),
      start: known.start_line,
      end: known.end_line,
    });
    let update = document.viewport_update(
      known,
      viewport.start_line..viewport.end_line,
      MAX_VIEW_LINES,
    );
    let (start, end) = (update.lines.start, update.lines.end);
    let chunks: Vec<Range<usize>> = update
      .chunks
      .iter()
      .map(|chunk| chunk.start..chunk.start + chunk.lines.len())
      .collect();
    let highlights = highlights(start..end, &chunks);

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
      line_count: buffer.line_count(),
      start_line: start,
      end_line: end,
      lines: update.into(),
      selections: visible,
      selection_count: selections.len(),
      primary: CursorDto {
        position: buffer.offset_to_utf16(head).into(),
        visual_column: document.visual_column(head),
      },
      dirty: document.is_dirty(),
      search: search(start, end),
      highlights,
      diagnostics: Vec::new(),
      language: None,
      decorations: Vec::new(),
    }
  }
}

#[cfg(test)]
mod tests {
  use ferrite_core::{Editor, MAX_MATCHES};

  use super::*;

  fn ops(json: &str) -> Vec<OpDto> {
    serde_json::from_str(json).expect("valid ops")
  }

  fn viewport(start_line: usize, end_line: usize) -> Viewport {
    Viewport {
      start_line,
      end_line,
    }
  }

  fn view(document: &Document, viewport: Viewport, known: Option<KnownDto>) -> EditorView {
    EditorView::of(document, viewport, known, |_, _| None, |_, _| Vec::new())
  }

  fn json(view: &EditorView) -> serde_json::Value {
    serde_json::to_value(view).expect("serializes")
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

    let first = json(&view(document, viewport(1, 99), None));
    assert_eq!(
      first["lines"]["chunks"][0]["lines"],
      serde_json::json!(["two", "🦀3"])
    );
    assert_eq!(first["lines"]["base"], serde_json::Value::Null);
    assert_eq!(first["startLine"], 1);
    assert_eq!(first["endLine"], 3);
    assert_eq!(
      first["primary"]["position"],
      serde_json::json!({ "line": 2, "column": 3 })
    );
    assert_eq!(first["primary"]["visualColumn"], 2);
    assert_eq!(first["dirty"], true);

    let clamped = view(document, viewport(50, 10), None);
    assert_eq!((clamped.start_line, clamped.end_line), (2, 3));
  }

  #[test]
  fn views_send_only_what_the_client_is_missing() {
    let mut editor = Editor::default();
    let id = editor.create_untitled();
    let document = editor.document_mut(id).expect("open");
    document
      .apply(EditorOp::Insert("line\n".repeat(50)))
      .expect("applies");
    let first = view(document, viewport(0, 10), None);
    let known = KnownDto {
      version: first.version,
      start_line: first.start_line,
      end_line: first.end_line,
    };
    // The cursor is on line 50, outside the viewport.
    document
      .apply(EditorOp::Insert("!".into()))
      .expect("applies");
    let next = json(&view(document, viewport(0, 10), Some(known)));
    assert_eq!(next["lines"]["base"], first.version);
    assert_eq!(next["lines"]["chunks"], serde_json::json!([]));
    assert_eq!(next["lines"]["splices"].as_array().map(Vec::len), Some(1));
  }

  #[test]
  fn cut_lines_serialize_with_their_full_length() {
    let line = LineDto::from(LineText {
      text: "abc".into(),
      full_utf16_len: Some(99),
    });
    assert_eq!(
      serde_json::to_value(line).expect("serializes"),
      serde_json::json!({ "text": "abc", "fullLength": 99 })
    );
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
    let first = view(document, viewport(0, 10), None);
    assert_eq!(first.selection_count, 2);
    assert_eq!(
      first.selections.len(),
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
    let later = view(document, viewport(90, 99), None);
    assert_eq!(later.selections.len(), 1);
    assert!(later.selections[0].primary);
  }

  #[test]
  fn large_documents_count_matches_in_the_background() {
    let mut editor = Editor::default();
    let id = editor.create_untitled();
    let document = editor.document_mut(id).expect("open");
    // Large enough to count in the background, with a needle every ten lines
    // (so the total stays under the match limit).
    let block = format!("needle\n{}", "haystack haystack haystack\n".repeat(9));
    let blocks = SYNC_COUNT_BYTES / block.len() + 1;
    document
      .apply(EditorOp::Insert(block.repeat(blocks)))
      .expect("applies");
    let query = SearchQuery::literal("needle");

    let (pending, needs_count) = SearchView::build(document, &query, 0, 20, None);
    assert!(needs_count);
    assert_eq!(pending.total, None, "counting");
    assert_eq!(
      pending.matches.len(),
      2,
      "visible matches are found right away"
    );

    let searcher = query.compile().expect("compiles");
    let buffer = document.buffer();
    let count = MatchCount {
      version: document.version(),
      query: query.clone(),
      matches: searcher.find_in_buffer(buffer, 0..buffer.len(), MAX_MATCHES + 1),
      truncated: false,
    };
    let (counted, needs_count) = SearchView::build(document, &query, 0, 20, Some(&count));
    assert!(!needs_count);
    assert_eq!(counted.total, Some(blocks));

    let stale = MatchCount {
      query: SearchQuery::literal("other"),
      ..count
    };
    assert!(SearchView::build(document, &query, 0, 20, Some(&stale)).1);
  }
}
