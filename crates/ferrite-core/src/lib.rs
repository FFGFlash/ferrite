//! Ferrite's editor core: open documents, selections, editing operations,
//! undo history, and dirty state.
//!
//! The core is independent of any UI and performs no I/O. Callers load files
//! with `ferrite-workspace`, hand the text to [`Editor::open`], and save by
//! writing a [`SavePoint`] snapshot and then calling
//! [`Document::mark_saved`].
//!
//! Positions are UTF-8 byte offsets, as in `ferrite-text`.

mod brackets;
mod document;
mod editor;
mod error;
mod find;
mod folds;
mod history;
mod lines;
mod ops;
mod selection;
#[cfg(test)]
mod testing;
mod view;

pub use document::{Document, DocumentId, EXACT_COLUMN_BYTES, EditorConfig, SavePoint};
pub use editor::Editor;
pub use error::CoreError;
pub use find::{MAX_MATCHES, MatchSummary};
pub use history::{EditKind, HistoryLimits, StateId};
pub use lines::CommentTokens;
pub use ops::{Direction, EditorOp, Motion, Unit, VerticalDirection};
pub use selection::{Selection, Selections};
pub use view::{KnownLines, LineChunk, LineText, MAX_DISPLAY_LINE_BYTES, ViewportUpdate};
