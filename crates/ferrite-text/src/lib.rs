//! Text storage for Ferrite.
//!
//! # Positions
//!
//! Internally, every position is a **UTF-8 byte offset** (`usize`) from the
//! start of the text, and every range is a `Range<usize>` of byte offsets.
//! Offsets passed to editing APIs must lie on character boundaries.
//!
//! Other units appear only at boundaries, with their own types:
//!
//! - [`Utf16Position`]: a line plus a column in UTF-16 code units, as used by
//!   the web frontend (JavaScript string indices) and the Language Server
//!   Protocol.
//!
//! Lines are separated by LF, CRLF, or CR, matching LSP. Unicode line
//! separators (U+2028 etc.) are ordinary characters.

mod boundaries;
mod buffer;
mod change;
mod line_ending;

pub use buffer::{TextBuffer, TextSnapshot, Utf16Position, Version};
pub use change::{ChangeSet, Edit, EditError};
pub use line_ending::{LineEnding, normalize_line_endings};
