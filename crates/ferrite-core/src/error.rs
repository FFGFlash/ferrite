use ferrite_text::EditError;

use crate::document::DocumentId;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
  #[error("document {0} is not open")]
  UnknownDocument(DocumentId),
  #[error("invalid edit: {0}")]
  Edit(#[from] EditError),
}
