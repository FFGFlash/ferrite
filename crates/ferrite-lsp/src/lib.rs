//! Language Server Protocol client for Ferrite.
//!
//! - [`LanguageServer`] runs one server process and speaks JSON-RPC with it
//!   over stdio ([`framing`] handles the base protocol).
//! - [`convert`] maps between byte offsets and LSP positions in either
//!   negotiated encoding (UTF-8 is preferred, as it needs no conversion) and
//!   builds incremental document changes from the buffer's edit log.
//! - [`config`] lists the known servers and finds workspace roots.
//!
//! Which documents are open on which server, diagnostics, and requests on
//! behalf of the UI are managed by the application.

pub mod client;
pub mod config;
pub mod convert;
pub mod framing;

pub use client::{
  CONTENT_MODIFIED, EventHandler, LanguageServer, LspError, REQUEST_CANCELLED, ServerCommand,
  ServerEvent,
};
pub use config::{ServerConfig, find_root};
pub use convert::PositionEncoding;
pub use lsp_types;
