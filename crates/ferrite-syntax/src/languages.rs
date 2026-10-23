use std::path::Path;
use std::sync::OnceLock;

use tree_sitter::{Language, Query};

use crate::highlight::TokenKind;
use crate::parse::SyntaxError;

/// A bundled language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageId {
  Rust,
  TypeScript,
  Tsx,
  JavaScript,
  Json,
  Toml,
  Python,
}

const ALL: [LanguageId; 7] = [
  LanguageId::Rust,
  LanguageId::TypeScript,
  LanguageId::Tsx,
  LanguageId::JavaScript,
  LanguageId::Json,
  LanguageId::Toml,
  LanguageId::Python,
];

/// A compiled grammar and highlight query.
pub(crate) struct Grammar {
  pub(crate) language: Language,
  pub(crate) query: Query,
  /// The token kind of each capture in `query`, by capture index.
  pub(crate) kinds: Vec<Option<TokenKind>>,
}

impl LanguageId {
  /// The language of a file, from its name or extension.
  pub fn from_path(path: &Path) -> Option<Self> {
    let name = path.file_name()?.to_str()?;
    if name == "Cargo.lock" {
      return Some(Self::Toml);
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
      "rs" => Self::Rust,
      "ts" | "mts" | "cts" => Self::TypeScript,
      "tsx" => Self::Tsx,
      "js" | "mjs" | "cjs" | "jsx" => Self::JavaScript,
      "json" | "jsonc" => Self::Json,
      "toml" => Self::Toml,
      "py" | "pyi" => Self::Python,
      _ => return None,
    })
  }

  /// Identifier used by LSP (`languageId`) and in settings.
  pub fn id(self) -> &'static str {
    match self {
      Self::Rust => "rust",
      Self::TypeScript => "typescript",
      Self::Tsx => "typescriptreact",
      Self::JavaScript => "javascript",
      Self::Json => "json",
      Self::Toml => "toml",
      Self::Python => "python",
    }
  }

  pub fn display_name(self) -> &'static str {
    match self {
      Self::Rust => "Rust",
      Self::TypeScript => "TypeScript",
      Self::Tsx => "TypeScript JSX",
      Self::JavaScript => "JavaScript",
      Self::Json => "JSON",
      Self::Toml => "TOML",
      Self::Python => "Python",
    }
  }

  pub fn all() -> impl Iterator<Item = Self> {
    ALL.into_iter()
  }

  fn index(self) -> usize {
    ALL
      .iter()
      .position(|&language| language == self)
      .unwrap_or(0)
  }

  /// The compiled grammar, built on first use.
  pub(crate) fn grammar(self) -> Result<&'static Grammar, SyntaxError> {
    static GRAMMARS: [OnceLock<Result<Grammar, SyntaxError>>; 7] = [
      OnceLock::new(),
      OnceLock::new(),
      OnceLock::new(),
      OnceLock::new(),
      OnceLock::new(),
      OnceLock::new(),
      OnceLock::new(),
    ];
    GRAMMARS[self.index()]
      .get_or_init(|| self.compile())
      .as_ref()
      .map_err(Clone::clone)
  }

  fn compile(self) -> Result<Grammar, SyntaxError> {
    let (language, query): (Language, String) = match self {
      Self::Rust => (
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::HIGHLIGHTS_QUERY.into(),
      ),
      // TypeScript's queries only add to JavaScript's.
      Self::TypeScript => (
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        format!(
          "{}\n{}",
          tree_sitter_typescript::HIGHLIGHTS_QUERY,
          tree_sitter_javascript::HIGHLIGHT_QUERY
        ),
      ),
      Self::Tsx => (
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        format!(
          "{}\n{}\n{}",
          tree_sitter_typescript::HIGHLIGHTS_QUERY,
          tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
          tree_sitter_javascript::HIGHLIGHT_QUERY
        ),
      ),
      Self::JavaScript => (
        tree_sitter_javascript::LANGUAGE.into(),
        format!(
          "{}\n{}",
          tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
          tree_sitter_javascript::HIGHLIGHT_QUERY
        ),
      ),
      Self::Json => (
        tree_sitter_json::LANGUAGE.into(),
        tree_sitter_json::HIGHLIGHTS_QUERY.into(),
      ),
      Self::Toml => (
        tree_sitter_toml_ng::LANGUAGE.into(),
        tree_sitter_toml_ng::HIGHLIGHTS_QUERY.into(),
      ),
      Self::Python => (
        tree_sitter_python::LANGUAGE.into(),
        tree_sitter_python::HIGHLIGHTS_QUERY.into(),
      ),
    };
    let query = Query::new(&language, &query).map_err(|error| SyntaxError::Query {
      language: self.id(),
      message: error.to_string(),
    })?;
    let kinds = query
      .capture_names()
      .iter()
      .map(|name| TokenKind::for_capture(name))
      .collect();
    Ok(Grammar {
      language,
      query,
      kinds,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn detects_languages_from_file_names() {
    let detect = |name: &str| LanguageId::from_path(Path::new(name));
    assert_eq!(detect("src/main.rs"), Some(LanguageId::Rust));
    assert_eq!(detect("App.TSX"), Some(LanguageId::Tsx));
    assert_eq!(detect("vite.config.ts"), Some(LanguageId::TypeScript));
    assert_eq!(detect("bench.mjs"), Some(LanguageId::JavaScript));
    assert_eq!(detect("Cargo.lock"), Some(LanguageId::Toml));
    assert_eq!(detect("notes.txt"), None);
    assert_eq!(detect("Makefile"), None);
  }

  #[test]
  fn every_bundled_grammar_and_query_compiles() {
    for language in LanguageId::all() {
      let grammar = language
        .grammar()
        .unwrap_or_else(|error| panic!("{}: {error}", language.id()));
      assert!(
        grammar.kinds.iter().any(Option::is_some),
        "{} highlights something",
        language.id()
      );
    }
  }
}
