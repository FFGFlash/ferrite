use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ferrite_workspace::LoadedFile;

use crate::document::{Document, DocumentId, EditorConfig};
use crate::error::CoreError;

/// The set of open documents.
#[derive(Debug)]
pub struct Editor {
  documents: BTreeMap<DocumentId, Document>,
  next_id: u64,
  next_untitled: u32,
  config: EditorConfig,
}

impl Default for Editor {
  fn default() -> Self {
    Self::new(EditorConfig::default())
  }
}

impl Editor {
  pub fn new(config: EditorConfig) -> Self {
    Self {
      documents: BTreeMap::new(),
      next_id: 1,
      next_untitled: 1,
      config,
    }
  }

  pub fn create_untitled(&mut self) -> DocumentId {
    let id = self.allocate_id();
    let number = self.next_untitled;
    self.next_untitled += 1;
    self
      .documents
      .insert(id, Document::new(id, "", None, number, &self.config));
    id
  }

  /// Opens a loaded file. Callers check [`Self::find_by_path`] first to avoid
  /// opening the same file twice.
  pub fn open(&mut self, path: PathBuf, file: LoadedFile) -> DocumentId {
    let id = self.allocate_id();
    let backing = Some((path, file.has_bom, file.stamp));
    self
      .documents
      .insert(id, Document::new(id, &file.text, backing, 0, &self.config));
    id
  }

  pub fn find_by_path(&self, path: &Path) -> Option<DocumentId> {
    self
      .documents
      .values()
      .find(|document| document.path() == Some(path))
      .map(Document::id)
  }

  pub fn document(&self, id: DocumentId) -> Result<&Document, CoreError> {
    self
      .documents
      .get(&id)
      .ok_or(CoreError::UnknownDocument(id))
  }

  pub fn document_mut(&mut self, id: DocumentId) -> Result<&mut Document, CoreError> {
    self
      .documents
      .get_mut(&id)
      .ok_or(CoreError::UnknownDocument(id))
  }

  /// Closes a document, discarding unsaved changes. Callers ask the user first.
  pub fn close(&mut self, id: DocumentId) -> Result<Document, CoreError> {
    self
      .documents
      .remove(&id)
      .ok_or(CoreError::UnknownDocument(id))
  }

  /// Open documents in the order they were opened.
  pub fn documents(&self) -> impl Iterator<Item = &Document> {
    self.documents.values()
  }

  fn allocate_id(&mut self) -> DocumentId {
    let id = DocumentId(self.next_id);
    self.next_id += 1;
    id
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ids_and_untitled_numbers_are_never_reused() {
    let mut editor = Editor::default();
    let first = editor.create_untitled();
    editor.close(first).expect("open");
    let second = editor.create_untitled();
    assert_ne!(first, second);
    assert_eq!(editor.document(second).expect("open").title(), "Untitled-2");
    assert!(matches!(
      editor.document(first),
      Err(CoreError::UnknownDocument(_))
    ));
  }

  #[test]
  fn new_documents_are_clean_and_empty() {
    let mut editor = Editor::default();
    let id = editor.create_untitled();
    let document = editor.document(id).expect("open");
    assert!(!document.is_dirty());
    assert_eq!(document.buffer().contents(), "");
    assert_eq!(editor.documents().count(), 1);
  }
}
