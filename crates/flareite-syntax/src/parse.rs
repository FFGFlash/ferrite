use std::cell::Cell;
use std::ops::{ControlFlow, Range};
use std::time::{Duration, Instant};

use flareite_text::{ByteEdit, TextBuffer, TextPoint, TextSnapshot};
use tree_sitter::{InputEdit, ParseOptions, Parser, Point, Tree, WasmStore};

use crate::languages::{Grammar, Language, Registry, engine};

/// Injections inside injections are followed this deep.
const MAX_INJECTION_DEPTH: usize = 4;
/// Most injected layers in one document; later injections aren't parsed.
const MAX_LAYERS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyntaxError {
  #[error("the {language} {kind} query is invalid: {message}")]
  Query {
    language: String,
    kind: &'static str,
    message: String,
  },
  #[error("the {language} grammar is incompatible with this version of Tree-sitter")]
  Grammar { language: String },
  #[error("the {language} grammar could not be loaded: {message}")]
  Load { language: String, message: String },
}

/// Text Tree-sitter can read piece by piece: a live buffer or a snapshot.
pub trait TextSource {
  fn len(&self) -> usize;
  fn is_empty(&self) -> bool {
    self.len() == 0
  }
  /// The text from `offset` to the end of its internal chunk.
  fn chunk_from(&self, offset: usize) -> &str;
}

impl TextSource for TextBuffer {
  fn len(&self) -> usize {
    TextBuffer::len(self)
  }

  fn chunk_from(&self, offset: usize) -> &str {
    TextBuffer::chunk_from(self, offset)
  }
}

impl TextSource for TextSnapshot {
  fn len(&self) -> usize {
    TextSnapshot::len(self)
  }

  fn chunk_from(&self, offset: usize) -> &str {
    TextSnapshot::chunk_from(self, offset)
  }
}

/// A parse of a document: the tree of its language, and a layer per
/// injected language (see [`crate::injections`]). Cheap to clone.
#[derive(Debug, Clone)]
pub struct SyntaxTree {
  language: Language,
  tree: Tree,
  pub(crate) layers: Vec<Layer>,
}

#[derive(Debug, Clone)]
pub(crate) struct Layer {
  pub(crate) language: Language,
  pub(crate) tree: Tree,
  /// From the start of its first range to the end of its last.
  pub(crate) span: Range<usize>,
  depth: usize,
}

impl SyntaxTree {
  pub fn language(&self) -> &Language {
    &self.language
  }

  /// The tree of the document's own language.
  pub fn tree(&self) -> &Tree {
    &self.tree
  }

  /// The injected languages and the bytes each layer spans, in the order
  /// they are painted.
  pub fn injections(&self) -> impl Iterator<Item = (&Language, Range<usize>)> {
    self
      .layers
      .iter()
      .map(|layer| (&layer.language, layer.span.clone()))
  }
}

/// Parses `source` as `language`, then the languages injected into it
/// (looked up in `registry`). With `old`, the parse is incremental: `old`
/// must already reflect the edits since it was produced (see
/// [`apply_edits`]). Polls `stop` while parsing and gives up, returning
/// `Ok(None)`, once it returns true (see [`deadline`]).
pub fn parse(
  language: &Language,
  registry: &Registry,
  source: &dyn TextSource,
  old: Option<&SyntaxTree>,
  mut stop: impl FnMut() -> bool,
) -> Result<Option<SyntaxTree>, SyntaxError> {
  let grammar = language.grammar()?;
  let old = old.filter(|old| old.language == *language);
  let Some(tree) = parse_tree(
    language,
    grammar,
    source,
    old.map(|old| &old.tree),
    &[],
    &mut stop,
  )?
  else {
    return Ok(None);
  };

  // Breadth first: the document's tree, then each layer as it is added.
  let mut layers: Vec<Layer> = Vec::new();
  let mut host = Some((language.clone(), tree.clone(), 0));
  let mut next = 0;
  while let Some((host_language, host_tree, depth)) = host.take() {
    if let Some(injections) = host_language
      .grammar()
      .ok()
      .and_then(|grammar| grammar.injections.as_ref())
      .filter(|_| depth < MAX_INJECTION_DEPTH)
    {
      for found in injections.find(&host_tree, source) {
        if layers.len() >= MAX_LAYERS {
          break;
        }
        let Some(injected) = registry.for_name(&found.language) else {
          continue;
        };
        let Ok(grammar) = injected.grammar() else {
          continue;
        };
        let span = found.ranges[0].start_byte..found.ranges[found.ranges.len() - 1].end_byte;
        // Tree-sitter only reuses the parts of an old tree that are inside
        // both its ranges and the new ones, so any overlapping layer will do.
        let old_layer = old.and_then(|old| {
          old.layers.iter().find(|layer| {
            layer.language == *injected
              && layer.span.start <= span.end
              && span.start <= layer.span.end
          })
        });
        let Some(tree) = parse_tree(
          injected,
          grammar,
          source,
          old_layer.map(|layer| &layer.tree),
          &found.ranges,
          &mut stop,
        )?
        else {
          return Ok(None);
        };
        layers.push(Layer {
          language: injected.clone(),
          tree,
          span,
          depth: depth + 1,
        });
      }
    }
    host = layers
      .get(next)
      .map(|layer| (layer.language.clone(), layer.tree.clone(), layer.depth));
    next += 1;
  }
  Ok(Some(SyntaxTree {
    language: language.clone(),
    tree,
    layers,
  }))
}

/// A parser kept per thread, so a grammar loaded from WebAssembly is
/// instantiated once per thread rather than on every parse.
struct PooledParser {
  parser: Parser,
  /// Whether `parser` has a WebAssembly store.
  wasm: bool,
}

thread_local! {
  static PARSER: Cell<Option<PooledParser>> = const { Cell::new(None) };
}

fn parse_tree(
  language: &Language,
  grammar: &Grammar,
  source: &dyn TextSource,
  old: Option<&Tree>,
  ranges: &[tree_sitter::Range],
  stop: &mut dyn FnMut() -> bool,
) -> Result<Option<Tree>, SyntaxError> {
  let mut pooled = PARSER.take().unwrap_or_else(|| PooledParser {
    parser: Parser::new(),
    wasm: false,
  });
  let result = parse_with(&mut pooled, language, grammar, source, old, ranges, stop);
  PARSER.set(Some(pooled));
  result
}

fn parse_with(
  pooled: &mut PooledParser,
  language: &Language,
  grammar: &Grammar,
  source: &dyn TextSource,
  old: Option<&Tree>,
  ranges: &[tree_sitter::Range],
  stop: &mut dyn FnMut() -> bool,
) -> Result<Option<Tree>, SyntaxError> {
  let incompatible = || SyntaxError::Grammar {
    language: language.id().to_owned(),
  };
  if !pooled.wasm {
    let store = WasmStore::new(engine()).map_err(|error| SyntaxError::Load {
      language: language.id().to_owned(),
      message: error.message,
    })?;
    pooled
      .parser
      .set_wasm_store(store)
      .map_err(|_| incompatible())?;
    pooled.wasm = true;
  }
  let parser = &mut pooled.parser;
  parser
    .set_language(&grammar.language)
    .map_err(|_| incompatible())?;
  if parser.set_included_ranges(ranges).is_err() {
    // Only injections pass ranges, and they come sorted from a tree.
    return Ok(None);
  }
  let mut read = |offset: usize, _: Point| source.chunk_from(offset).as_bytes();
  let mut check = |_: &tree_sitter::ParseState| {
    if stop() {
      ControlFlow::Break(())
    } else {
      ControlFlow::Continue(())
    }
  };
  let options = ParseOptions::new().progress_callback(&mut check);
  let tree = parser.parse_with_options(&mut read, old, Some(options));
  // A stopped parser would otherwise resume this parse next time.
  parser.reset();
  Ok(tree)
}

/// A `stop` condition for [`parse`] that is met after `budget`.
pub fn deadline(budget: Duration) -> impl FnMut() -> bool {
  let deadline = Instant::now() + budget;
  move || Instant::now() > deadline
}

fn point(point: TextPoint) -> Point {
  Point {
    row: point.row,
    column: point.column,
  }
}

/// Updates `syntax`'s positions for `edits` (in application order), so it
/// can be highlighted right away and reused for an incremental parse.
pub fn apply_edits(syntax: &mut SyntaxTree, edits: &[ByteEdit]) {
  for edit in edits {
    let input = InputEdit {
      start_byte: edit.start,
      old_end_byte: edit.old_end,
      new_end_byte: edit.new_end,
      start_position: point(edit.start_point),
      old_end_position: point(edit.old_end_point),
      new_end_position: point(edit.new_end_point),
    };
    syntax.tree.edit(&input);
    for layer in &mut syntax.layers {
      layer.tree.edit(&input);
      // Grown by insertions at either end, which the layer may take in.
      layer.span = edit.map_offset(layer.span.start, false)..edit.map_offset(layer.span.end, true);
    }
  }
}

#[cfg(test)]
mod tests {
  use flareite_text::{ChangeSet, Edit};

  use super::*;
  use crate::testing;

  fn language(id: &str) -> Language {
    testing::registry().get(id).expect("an example").clone()
  }

  fn parse_all(id: &str, source: &dyn TextSource, old: Option<&SyntaxTree>) -> SyntaxTree {
    parse(&language(id), &testing::registry(), source, old, || false)
      .expect("grammar")
      .expect("parses")
  }

  #[test]
  fn incremental_parses_match_full_parses() {
    let mut buffer = TextBuffer::from_text("fn main() {\n  let x = 1;\n}\n");
    let mut tree = parse_all("rust", &buffer, None);
    let before = buffer.version();
    buffer
      .apply(&ChangeSet::single(Edit::insert(23, " + 2")).expect("valid"))
      .expect("applies");
    apply_edits(&mut tree, &buffer.edits_since(before).expect("edits"));
    let incremental = parse_all("rust", &buffer, Some(&tree));
    let full = parse_all("rust", &buffer, None);
    assert_eq!(
      incremental.tree().root_node().to_sexp(),
      full.tree().root_node().to_sexp()
    );
    assert!(
      incremental
        .tree()
        .root_node()
        .to_sexp()
        .contains("binary_expression")
    );
  }

  #[test]
  fn a_spent_budget_gives_up_instead_of_blocking() {
    let text = "fn f() { let x = [1, 2, 3]; }\n".repeat(200_000);
    let buffer = TextBuffer::from_text(&text);
    let tree = parse(
      &language("rust"),
      &testing::registry(),
      &buffer,
      None,
      deadline(Duration::ZERO),
    )
    .expect("grammar");
    assert!(tree.is_none());
    // The pooled parser starts the next parse afresh.
    let small = TextBuffer::from_text("fn g() {}");
    assert!(
      !parse_all("rust", &small, None)
        .tree()
        .root_node()
        .has_error()
    );
  }

  #[test]
  fn snapshots_parse_like_buffers() {
    let buffer = TextBuffer::from_text("{\"a\": [1, true]}");
    let snapshot = buffer.snapshot();
    let tree = parse_all("json", &snapshot, None);
    assert!(!tree.tree().root_node().has_error());
    assert_eq!(tree.injections().count(), 0);
  }
}
