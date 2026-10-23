//! Opening, editing, and saving files through the core and workspace crates,
//! the way the application wires them together.

use std::fs;

use ferrite_core::{Direction, Editor, EditorOp, Motion, Unit};
use ferrite_workspace::{FileError, load_text_file, save_text_file};

#[test]
fn edits_round_trip_through_disk() {
  let dir = tempfile::tempdir().expect("temp dir");
  let path = dir.path().join("main.rs");
  fs::write(&path, b"\xEF\xBB\xBFfn main() {\r\n}\r\n").expect("write");

  let mut editor = Editor::default();
  let id = editor.open(path.clone(), load_text_file(&path).expect("loads"));
  assert_eq!(editor.find_by_path(&path), Some(id));

  let document = editor.document_mut(id).expect("open");
  for op in [
    EditorOp::Move {
      motion: Motion::LineEnd,
      extend: false,
    },
    EditorOp::Newline,
    EditorOp::Insert("println!(\"hi\");".into()),
  ] {
    document.apply(op).expect("applies");
  }
  assert!(document.is_dirty());

  let point = document.save_point();
  let stamp = save_text_file(
    &path,
    &point.snapshot,
    point.has_bom,
    point.expected_stamp.as_ref(),
  )
  .expect("saves");
  document.mark_saved(&point, path.clone(), stamp);
  assert!(!document.is_dirty());

  // BOM and CRLF line endings are preserved; the new line uses CRLF too.
  assert_eq!(
    fs::read(&path).expect("read"),
    b"\xEF\xBB\xBFfn main() {\r\nprintln!(\"hi\");\r\n}\r\n"
  );
}

#[test]
fn saving_over_an_external_change_is_refused_until_confirmed() {
  let dir = tempfile::tempdir().expect("temp dir");
  let path = dir.path().join("notes.txt");
  fs::write(&path, "draft").expect("write");

  let mut editor = Editor::default();
  let id = editor.open(path.clone(), load_text_file(&path).expect("loads"));
  let document = editor.document_mut(id).expect("open");
  document
    .apply(EditorOp::Delete {
      direction: Direction::Forward,
      unit: Unit::Word,
    })
    .expect("applies");

  fs::write(&path, "rewritten by another program").expect("write");

  let point = document.save_point();
  let result = save_text_file(
    &path,
    &point.snapshot,
    point.has_bom,
    point.expected_stamp.as_ref(),
  );
  assert!(matches!(result, Err(FileError::ModifiedOnDisk { .. })));
  assert!(document.is_dirty(), "a refused save keeps the edits dirty");

  // The user chose to overwrite: save without the expected stamp.
  let stamp = save_text_file(&path, &point.snapshot, point.has_bom, None).expect("saves");
  document.mark_saved(&point, path.clone(), stamp);
  assert_eq!(fs::read_to_string(&path).expect("read"), "");
  assert!(!document.is_dirty());
}

#[test]
fn save_as_gives_untitled_documents_a_path() {
  let dir = tempfile::tempdir().expect("temp dir");
  let path = dir.path().join("new.txt");

  let mut editor = Editor::default();
  let id = editor.create_untitled();
  let document = editor.document_mut(id).expect("open");
  document
    .apply(EditorOp::Insert("hello".into()))
    .expect("applies");
  assert_eq!(document.title(), "Untitled-1");

  let point = document.save_point();
  assert!(point.expected_stamp.is_none());
  let stamp = save_text_file(&path, &point.snapshot, point.has_bom, None).expect("saves");
  document.mark_saved(&point, path.clone(), stamp);

  assert_eq!(document.title(), "new.txt");
  assert_eq!(editor.find_by_path(&path), Some(id));
  assert_eq!(fs::read_to_string(&path).expect("read"), "hello");
}
