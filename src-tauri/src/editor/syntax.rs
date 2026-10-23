//! Syntax highlighting for open documents.
//!
//! Each document of a known language keeps a Tree-sitter tree. Edits update
//! the tree's positions right away; then a small incremental reparse runs
//! inline within [`INLINE_PARSE_BUDGET`], and anything longer (first parses,
//! large documents) runs on the worker pool over a snapshot, after which the
//! frontend is told to refresh. Until then the edited old tree is used, so
//! colors stay in place while typing.
//!
//! Views carry highlights only for lines whose spans the client does not
//! already have: the last spans sent are remembered (as hashes) for the lines
//! the client holds and replayed through line splices like the lines
//! themselves.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use ferrite_core::{Document, DocumentId, MAX_DISPLAY_LINE_BYTES};
use ferrite_syntax::{LanguageId, Span, Tree, apply_edits, deadline, highlight_lines, parse};
use ferrite_tasks::Priority;
use ferrite_text::{TextBuffer, TextSnapshot, Version};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::protocol::KnownDto;
use crate::events::{self, AppEvent};
use crate::perf::Perf;

/// Larger documents are not highlighted.
pub const MAX_SYNTAX_BYTES: usize = 16 * 1024 * 1024;
/// Longest reparse done while handling an edit; longer ones go to a worker.
const INLINE_PARSE_BUDGET: Duration = Duration::from_millis(3);

/// The highlight spans of one line, flattened as `[start, end, kind, ...]`
/// with UTF-16 columns. Kinds index `TokenKind::NAMES`, starting at 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineHighlightsDto {
  pub line: usize,
  pub spans: Vec<u32>,
}

/// The spans sent for the lines a client holds.
struct Mirror {
  version: Version,
  start: usize,
  hashes: Vec<u64>,
}

struct ParseJob {
  language: LanguageId,
  snapshot: TextSnapshot,
  old: Option<Tree>,
  version: Version,
}

struct DocumentSyntax {
  language: LanguageId,
  tree: Option<Tree>,
  /// The text version `tree` is a parse of.
  parsed: Version,
  /// The version `tree`'s positions have been edited to.
  edited: Version,
  /// The version a background parse is running for.
  pending: Option<Version>,
  mirror: Option<Mirror>,
}

impl DocumentSyntax {
  fn new(language: LanguageId) -> Self {
    Self {
      language,
      tree: None,
      parsed: Version::INITIAL,
      edited: Version::INITIAL,
      pending: None,
      mirror: None,
    }
  }

  /// Brings the tree up to date with `buffer` as far as possible without
  /// blocking. Returns a parse to run in the background, if one is needed.
  fn update(&mut self, buffer: &TextBuffer) -> Option<ParseJob> {
    let version = buffer.version();
    if let Some(tree) = &mut self.tree
      && self.edited != version
    {
      match buffer.edits_since(self.edited) {
        Some(edits) => {
          apply_edits(tree, &edits);
          self.edited = version;
        }
        // Too far behind to catch up: start over.
        None => self.tree = None,
      }
    }
    if (self.tree.is_some() && self.parsed == version) || self.pending.is_some() {
      return None;
    }
    if let Some(tree) = &self.tree
      && let Ok(Some(tree)) = parse(
        self.language,
        buffer,
        Some(tree),
        deadline(INLINE_PARSE_BUDGET),
      )
    {
      self.tree = Some(tree);
      self.parsed = version;
      return None;
    }
    self.pending = Some(version);
    Some(ParseJob {
      language: self.language,
      snapshot: buffer.snapshot(),
      old: self.tree.clone(),
      version,
    })
  }

  /// Takes the result of a background parse of `version`. Returns whether
  /// the highlighting may have changed.
  fn finish(&mut self, version: Version, tree: Option<Tree>, buffer: &TextBuffer) -> bool {
    if self.pending == Some(version) {
      self.pending = None;
    }
    let Some(mut tree) = tree else {
      return false;
    };
    if self.tree.is_some() && self.parsed.get() >= version.get() {
      return false;
    }
    match buffer.edits_since(version) {
      Some(edits) => apply_edits(&mut tree, &edits),
      None => return false,
    }
    self.tree = Some(tree);
    self.parsed = version;
    self.edited = buffer.version();
    true
  }

  /// Highlights for `lines` that a client holding `known` lacks, given that
  /// the view replaces `chunks` (whose spans the client resets).
  fn highlights(
    &mut self,
    buffer: &TextBuffer,
    lines: Range<usize>,
    known: Option<KnownDto>,
    chunks: &[Range<usize>],
  ) -> Vec<LineHighlightsDto> {
    let spans = match &self.tree {
      Some(tree) => highlight_lines(
        self.language,
        tree,
        buffer,
        lines.clone(),
        MAX_DISPLAY_LINE_BYTES,
      ),
      None => vec![Vec::new(); lines.len()],
    };
    let carried = self.carried(buffer, known, lines.clone(), chunks);
    let mut changed = Vec::new();
    let mut hashes = Vec::with_capacity(lines.len());
    for ((line, spans), carried) in lines.clone().zip(spans).zip(carried) {
      let spans = if spans.is_empty() {
        Vec::new()
      } else {
        let (text, _) = buffer.line_prefix(line, MAX_DISPLAY_LINE_BYTES);
        flatten_utf16(&text, &spans)
      };
      let hash = hash_spans(&spans);
      if carried != Some(hash) {
        changed.push(LineHighlightsDto { line, spans });
      }
      hashes.push(hash);
    }
    self.mirror = Some(Mirror {
      version: buffer.version(),
      start: lines.start,
      hashes,
    });
    changed
  }

  /// The hash of the spans the client has for each of `lines`, if known.
  fn carried(
    &self,
    buffer: &TextBuffer,
    known: Option<KnownDto>,
    lines: Range<usize>,
    chunks: &[Range<usize>],
  ) -> Vec<Option<u64>> {
    let mut carried = vec![None; lines.len()];
    if let (Some(mirror), Some(known)) = (&self.mirror, known)
      && mirror.version.get() == known.version
      && mirror.start == known.start_line
      && mirror.start + mirror.hashes.len() == known.end_line
      && let Some(splices) = buffer.line_changes_since(mirror.version)
    {
      // As the client does with its lines (see `lineCache.ts`).
      let mut held: Vec<(usize, u64)> = (mirror.start..)
        .zip(mirror.hashes.iter().copied())
        .collect();
      for splice in splices {
        let removed_end = splice.start + splice.removed;
        held.retain(|&(line, _)| line < splice.start || line >= removed_end);
        for (line, _) in &mut held {
          if *line >= removed_end {
            *line = *line + splice.inserted - splice.removed;
          }
        }
      }
      for (line, hash) in held {
        if lines.contains(&line) {
          carried[line - lines.start] = Some(hash);
        }
      }
    }
    let empty = hash_spans(&[]);
    for line in chunks.iter().flat_map(Clone::clone) {
      if lines.contains(&line) {
        carried[line - lines.start] = Some(empty);
      }
    }
    carried
  }
}

fn hash_spans(spans: &[u32]) -> u64 {
  let mut hasher = DefaultHasher::new();
  spans.hash(&mut hasher);
  hasher.finish()
}

/// `spans` (byte columns into `text`, in order) as flat UTF-16 triples.
fn flatten_utf16(text: &str, spans: &[Span]) -> Vec<u32> {
  let mut flat = Vec::with_capacity(spans.len() * 3);
  let ascii = text.is_ascii();
  let column = |byte: usize, walked: &mut (usize, usize)| -> u32 {
    let byte = byte.min(text.len());
    if ascii {
      return byte as u32;
    }
    // Spans are in order, so each conversion resumes where the last ended.
    let (mut offset, mut column) = *walked;
    for ch in text[offset..].chars() {
      if offset >= byte {
        break;
      }
      offset += ch.len_utf8();
      column += ch.len_utf16();
    }
    *walked = (offset, column);
    column as u32
  };
  let mut walked = (0, 0);
  for span in spans {
    let start = column(span.start, &mut walked);
    let end = column(span.end, &mut walked);
    if start < end {
      flat.extend([start, end, span.kind as u32]);
    }
  }
  flat
}

#[derive(Default)]
pub struct SyntaxStates {
  documents: Mutex<HashMap<DocumentId, DocumentSyntax>>,
}

impl SyntaxStates {
  fn lock(&self) -> MutexGuard<'_, HashMap<DocumentId, DocumentSyntax>> {
    self
      .documents
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
  }

  /// Highlights for a view of `lines` (see [`DocumentSyntax::highlights`]),
  /// scheduling a background parse if needed. Called with the editor locked.
  pub fn highlights(
    &self,
    app: &AppHandle,
    document: &Document,
    lines: Range<usize>,
    known: Option<KnownDto>,
    chunks: &[Range<usize>],
  ) -> Vec<LineHighlightsDto> {
    let id = document.id();
    let buffer = document.buffer();
    let language = document
      .path()
      .and_then(LanguageId::from_path)
      .filter(|_| buffer.len() <= MAX_SYNTAX_BYTES);
    let mut documents = self.lock();
    let Some(language) = language else {
      documents.remove(&id);
      return Vec::new();
    };
    let syntax = documents
      .entry(id)
      .and_modify(|syntax| {
        if syntax.language != language {
          *syntax = DocumentSyntax::new(language);
        }
      })
      .or_insert_with(|| DocumentSyntax::new(language));
    let started = Instant::now();
    let job = syntax.update(buffer);
    let highlights = syntax.highlights(buffer, lines, known, chunks);
    drop(documents);
    let perf = app.state::<Perf>();
    perf.record("syntax.highlight", started.elapsed());
    if let Some(job) = job {
      spawn_parse(app, id, job);
    }
    highlights
  }

  /// Drops the syntax state of a closed document.
  pub fn forget(&self, id: DocumentId) {
    self.lock().remove(&id);
  }
}

fn spawn_parse(app: &AppHandle, id: DocumentId, job: ParseJob) {
  let task_app = app.clone();
  app.state::<Perf>().scheduler.spawn(
    "parse",
    Priority::High,
    Some(format!("parse:{id}")),
    move |token| {
      let started = Instant::now();
      let tree = match parse(job.language, &job.snapshot, job.old.as_ref(), || {
        token.is_cancelled()
      }) {
        Ok(tree) => tree,
        Err(error) => {
          tracing::error!(%error, "syntax highlighting is unavailable");
          None
        }
      };
      task_app
        .state::<Perf>()
        .record("syntax.parse", started.elapsed());
      let state = task_app.state::<super::EditorState>();
      let updated = {
        let editor = state.lock();
        let Ok(document) = editor.document(id) else {
          return;
        };
        let mut documents = state.syntax.lock();
        documents
          .get_mut(&id)
          .is_some_and(|syntax| syntax.finish(job.version, tree, document.buffer()))
      };
      if updated {
        events::emit(
          &task_app,
          AppEvent::SyntaxUpdated {
            document_id: id.get(),
          },
        );
      }
    },
  );
}

#[cfg(test)]
// Chunk ranges are naturally single-element arrays here.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
  use ferrite_syntax::TokenKind;
  use ferrite_text::{ChangeSet, Edit};

  use super::*;

  fn parsed(text: &str) -> (TextBuffer, DocumentSyntax) {
    let buffer = TextBuffer::from_text(text);
    let mut syntax = DocumentSyntax::new(LanguageId::Rust);
    let job = syntax
      .update(&buffer)
      .expect("first parse runs in the background");
    let tree = parse(job.language, &job.snapshot, None, || false).expect("grammar");
    assert!(syntax.finish(job.version, tree, &buffer));
    (buffer, syntax)
  }

  fn known(version: Version, lines: Range<usize>) -> Option<KnownDto> {
    Some(KnownDto {
      version: version.get(),
      start_line: lines.start,
      end_line: lines.end,
    })
  }

  #[test]
  fn sends_only_highlights_the_client_lacks() {
    let (mut buffer, mut syntax) = parsed("fn a() {}\nfn b() {}\nfn c() {}\n");
    let first = syntax.highlights(&buffer, 0..4, None, &[0..4]);
    assert_eq!(first.len(), 3, "the empty last line needs nothing");
    let v1 = buffer.version();

    // Unchanged: nothing to send.
    assert!(
      syntax
        .highlights(&buffer, 0..4, known(v1, 0..4), &[])
        .is_empty()
    );

    // Editing line 1 resends it (as a chunk) and nothing else.
    buffer
      .apply(&ChangeSet::single(Edit::insert(13, "x")).expect("valid"))
      .expect("applies");
    assert!(
      syntax.update(&buffer).is_none(),
      "small reparse runs inline"
    );
    let after = syntax.highlights(&buffer, 0..4, known(v1, 0..4), &[1..2]);
    assert_eq!(after.iter().map(|h| h.line).collect::<Vec<_>>(), [1]);

    // A view from a client that does not match resends everything.
    let mismatch = syntax.highlights(&buffer, 0..4, known(v1, 0..4), &[]);
    assert_eq!(mismatch.len(), 4);
  }

  #[test]
  fn edits_that_change_later_lines_resend_them() {
    let (mut buffer, mut syntax) = parsed("a();\nb();\n");
    syntax.highlights(&buffer, 0..3, None, &[0..3]);
    let v1 = buffer.version();
    buffer
      .apply(&ChangeSet::single(Edit::insert(0, "/*")).expect("valid"))
      .expect("applies");
    syntax.update(&buffer);
    let after = syntax.highlights(&buffer, 0..3, known(v1, 0..3), &[0..1]);
    let lines: Vec<usize> = after.iter().map(|h| h.line).collect();
    assert_eq!(lines, [0, 1], "line 1 is inside the new comment");
    assert_eq!(after[1].spans, [0, 4, TokenKind::Comment as u32]);
  }

  #[test]
  fn stale_background_results_are_caught_up_or_dropped() {
    let mut buffer = TextBuffer::from_text("fn a() {}\n");
    let mut syntax = DocumentSyntax::new(LanguageId::Rust);
    let job = syntax.update(&buffer).expect("background parse");
    assert!(syntax.update(&buffer).is_none(), "one parse at a time");
    buffer
      .apply(&ChangeSet::single(Edit::insert(0, "\n")).expect("valid"))
      .expect("applies");
    let tree = parse(job.language, &job.snapshot, None, || false).expect("grammar");
    assert!(syntax.finish(job.version, tree, &buffer));
    let tree = syntax.tree.as_ref().expect("tree");
    assert_eq!(
      tree.root_node().child(0).expect("fn").start_position().row,
      1
    );
    assert_eq!(syntax.edited, buffer.version());
  }

  #[test]
  fn converts_byte_columns_to_utf16() {
    let span = |start, end| Span {
      start,
      end,
      kind: TokenKind::String,
    };
    let kind = TokenKind::String as u32;
    assert_eq!(flatten_utf16("ab", &[span(0, 2)]), [0, 2, kind]);
    // "é" is 2 bytes and 1 unit; "🦀" is 4 bytes and 2 units.
    assert_eq!(
      flatten_utf16("é🦀x", &[span(0, 2), span(2, 6), span(6, 7)]),
      [0, 1, kind, 1, 3, kind, 3, 4, kind]
    );
  }
}
