//! Foldable regions, for the fold operations and the gutter's markers: from
//! the fold query of the document's language, or by indentation for
//! languages without one (and documents without a language).

use std::ops::Range;

use ferrite_core::Document;
use tauri::{AppHandle, Manager};

use super::EditorState;
use super::protocol::OpDto;

/// Which regions an operation needs.
pub enum Scope {
  /// Those starting on or around these lines.
  Around(Vec<usize>),
  All,
}

/// The regions `op` needs, if it is a fold operation that needs any.
pub fn scope(op: &OpDto, document: &Document) -> Option<Scope> {
  match op {
    OpDto::Fold | OpDto::ToggleFold | OpDto::FoldRecursively | OpDto::UnfoldRecursively => {
      let buffer = document.buffer();
      let lines = document
        .selections()
        .iter()
        .map(|s| buffer.line_of(s.head))
        .collect();
      Some(Scope::Around(lines))
    }
    OpDto::ToggleFoldAt { line } => Some(Scope::Around(vec![*line])),
    OpDto::FoldAll | OpDto::FoldLevel { .. } => Some(Scope::All),
    _ => None,
  }
}

/// The document's foldable regions in `scope`, as line ranges.
pub fn regions(app: &AppHandle, document: &Document, scope: Scope) -> Vec<Range<usize>> {
  let ranges: Vec<Range<usize>> = match &scope {
    Scope::Around(lines) => lines.iter().map(|&line| line..line + 1).collect(),
    Scope::All => std::iter::once(0..document.buffer().line_count()).collect(),
  };
  let syntax = &app.state::<EditorState>().syntax;
  if let Some(regions) = syntax.fold_regions(app, document, &ranges) {
    return regions;
  }
  match scope {
    Scope::Around(lines) => {
      let mut regions: Vec<Range<usize>> = lines
        .into_iter()
        .flat_map(|line| document.indentation_folds(Some(line)))
        .collect();
      regions.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
      regions.dedup();
      regions
    }
    Scope::All => document.indentation_folds(None),
  }
}

/// The lines in `ranges` that start a foldable region, in order.
pub fn foldable_lines(app: &AppHandle, document: &Document, ranges: &[Range<usize>]) -> Vec<usize> {
  let syntax = &app.state::<EditorState>().syntax;
  match syntax.fold_regions(app, document, ranges) {
    Some(regions) => {
      let mut lines: Vec<usize> = regions
        .into_iter()
        .map(|region| region.start)
        .filter(|line| ranges.iter().any(|range| range.contains(line)))
        .collect();
      lines.dedup();
      lines
    }
    None => ranges
      .iter()
      .flat_map(|range| document.indentation_fold_starts(range.clone()))
      .collect(),
  }
}
