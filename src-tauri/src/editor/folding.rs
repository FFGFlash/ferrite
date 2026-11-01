//! Foldable regions, for the fold operations and the gutter's markers: from
//! the fold query of the document's language, or by indentation for
//! languages without one (and documents without a language), plus regions
//! marked by `#region` comments. The marker comments also give the
//! minimap's section headers.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Mutex, PoisonError};

use ferrite_core::{CommentTokens, Document, DocumentId, RegionMarkers, SectionHeader};
use tauri::{AppHandle, Manager};

use super::EditorState;
use super::protocol::OpDto;
use super::syntax::MAX_SYNTAX_BYTES;
use crate::languages::LanguagesState;

/// Which regions an operation needs.
pub enum Scope {
  /// Those starting on or around these lines.
  Around(Vec<usize>),
  All,
  /// Only those marked by region comments.
  Marked,
}

/// The region comments of open documents, kept up to date as they change.
#[derive(Default)]
pub struct RegionStates(Mutex<HashMap<DocumentId, RegionMarkers>>);

impl RegionStates {
  /// `f` of `document`'s region markers, brought up to date. Called with the
  /// editor locked.
  fn with<T>(
    &self,
    app: &AppHandle,
    document: &Document,
    f: impl FnOnce(&RegionMarkers) -> T,
  ) -> T {
    let tokens = match document.path() {
      Some(path) if document.buffer().len() <= MAX_SYNTAX_BYTES => {
        app.state::<LanguagesState>().comments_for(path)
      }
      _ => CommentTokens::default(),
    };
    let mut documents = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    let markers = documents.entry(document.id()).or_default();
    markers.sync(document.buffer(), &tokens);
    f(markers)
  }

  /// Drops a closed document's markers.
  pub fn forget(&self, id: DocumentId) {
    self
      .0
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .remove(&id);
  }
}

/// The regions `document`'s region comments mark.
fn marked(app: &AppHandle, document: &Document) -> Vec<Range<usize>> {
  app
    .state::<EditorState>()
    .regions
    .with(app, document, |markers| {
      markers
        .regions()
        .into_iter()
        .map(|region| region.lines)
        .collect()
    })
}

/// The section headers on `lines`: named regions and `MARK:` comments.
pub fn section_headers(
  app: &AppHandle,
  document: &Document,
  lines: Range<usize>,
) -> Vec<SectionHeader> {
  app
    .state::<EditorState>()
    .regions
    .with(app, document, |markers| markers.headers(lines))
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
    OpDto::FoldAllRegions | OpDto::UnfoldAllRegions => Some(Scope::Marked),
    _ => None,
  }
}

/// The document's foldable regions in `scope`, as line ranges, in order.
pub fn regions(app: &AppHandle, document: &Document, scope: Scope) -> Vec<Range<usize>> {
  let marked = marked(app, document);
  let mut regions = match &scope {
    Scope::Marked => return marked,
    Scope::Around(lines) => marked
      .into_iter()
      .filter(|region| lines.iter().any(|line| region.contains(line)))
      .collect(),
    Scope::All => marked,
  };
  regions.extend(language_regions(app, document, scope));
  regions.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
  regions.dedup();
  regions
}

/// The regions from the language's fold query, or from indentation.
fn language_regions(app: &AppHandle, document: &Document, scope: Scope) -> Vec<Range<usize>> {
  let ranges: Vec<Range<usize>> = match &scope {
    Scope::Around(lines) => lines.iter().map(|&line| line..line + 1).collect(),
    Scope::All | Scope::Marked => std::iter::once(0..document.buffer().line_count()).collect(),
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
    Scope::All | Scope::Marked => document.indentation_folds(None),
  }
}

/// The lines in `ranges` that start a foldable region, in order.
pub fn foldable_lines(app: &AppHandle, document: &Document, ranges: &[Range<usize>]) -> Vec<usize> {
  let shown = |line: &usize| ranges.iter().any(|range| range.contains(line));
  let mut lines: Vec<usize> = marked(app, document)
    .into_iter()
    .map(|region| region.start)
    .filter(shown)
    .collect();
  lines.extend(language_foldable_lines(app, document, ranges));
  lines.sort_unstable();
  lines.dedup();
  lines
}

fn language_foldable_lines(
  app: &AppHandle,
  document: &Document,
  ranges: &[Range<usize>],
) -> Vec<usize> {
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
