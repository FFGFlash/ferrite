//! Syntax support for Ferrite, built on Tree-sitter.
//!
//! - [`LanguageId`] identifies the bundled languages and maps file names to
//!   them. Grammars are compiled into the binary; each language's highlight
//!   query is compiled the first time a file of that language is opened.
//! - [`parse`] (re)parses a buffer or snapshot, incrementally from an edited
//!   old tree, and can be stopped part way (by a time budget or cancellation)
//!   so callers can fall back to a background parse instead of blocking.
//! - [`highlight_lines`] turns a tree into per-line [`Span`]s of
//!   [`TokenKind`]s for display.
//!
//! Positions are UTF-8 byte offsets, as everywhere in Ferrite.

mod highlight;
mod languages;
mod parse;

pub use highlight::{Span, TokenKind, highlight_lines};
pub use languages::LanguageId;
pub use parse::{SyntaxError, TextSource, apply_edits, deadline, parse};
pub use tree_sitter::Tree;
