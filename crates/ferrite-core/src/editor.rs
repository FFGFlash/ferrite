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
    let backing = Some((path, file.encoding, file.stamp));
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

  /// Follows a file or folder that was renamed or moved from `from` to `to`:
  /// documents of that file, or of files in that folder, get their new
  /// paths. Returns the documents that moved.
  pub fn follow_move(&mut self, from: &Path, to: &Path) -> Vec<DocumentId> {
    let mut moved = Vec::new();
    for document in self.documents.values_mut() {
      let Some(rest) = document
        .path()
        .and_then(|path| path.strip_prefix(from).ok())
      else {
        continue;
      };
      let path = if rest.as_os_str().is_empty() {
        to.to_path_buf()
      } else {
        to.join(rest)
      };
      document.moved_to(path);
      moved.push(document.id());
    }
    moved
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

  pub fn config(&self) -> &EditorConfig {
    &self.config
  }

  /// Replaces the configuration. Indentation settings apply to open
  /// documents immediately; history limits apply to documents opened later.
  pub fn set_config(&mut self, config: EditorConfig) {
    for document in self.documents.values_mut() {
      document.set_indentation(config.tab_size, config.insert_spaces);
    }
    self.config = config;
  }

  pub fn document_ids(&self) -> Vec<DocumentId> {
    self.documents.keys().copied().collect()
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

  #[test]
  fn documents_follow_their_files_when_moved() {
    let dir = tempfile::tempdir().expect("temp dir");
    let open = |editor: &mut Editor, path: PathBuf| {
      std::fs::write(&path, "x").expect("write");
      let loaded = ferrite_workspace::load_text_file(&path).expect("loads");
      editor.open(path, loaded)
    };
    let mut editor = Editor::default();
    std::fs::create_dir(dir.path().join("src")).expect("mkdir");
    let lib = open(&mut editor, dir.path().join("src").join("lib.rs"));
    let srcs = open(&mut editor, dir.path().join("srcs.rs"));
    let untitled = editor.create_untitled();

    let to = dir.path().join("crate");
    assert_eq!(editor.follow_move(&dir.path().join("src"), &to), vec![lib]);
    let path = |id| {
      editor
        .document(id)
        .expect("open")
        .path()
        .map(Path::to_path_buf)
    };
    assert_eq!(path(lib), Some(to.join("lib.rs")));
    // A sibling whose name starts the same isn't inside the folder.
    assert_eq!(path(srcs), Some(dir.path().join("srcs.rs")));
    assert_eq!(path(untitled), None);

    let main = dir.path().join("main.rs");
    assert_eq!(
      editor.follow_move(&dir.path().join("srcs.rs"), &main),
      vec![srcs]
    );
    assert_eq!(editor.document(srcs).expect("open").title(), "main.rs");
  }
}
