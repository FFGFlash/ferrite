use flareite_text::{EditError, SearchError};

use crate::document::DocumentId;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
  #[error("document {0} is not open")]
  UnknownDocument(DocumentId),
  #[error("document {0} is read-only")]
  ReadOnly(DocumentId),
  #[error("invalid edit: {0}")]
  Edit(#[from] EditError),
  #[error(transparent)]
  Search(#[from] SearchError),
}
