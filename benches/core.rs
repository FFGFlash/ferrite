//! Editor core: typing, undo and redo, viewport updates, and find/replace.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use ferrite_benches::code_text;
use ferrite_core::{Direction, Editor, EditorOp, KnownLines, Motion, Unit};
use ferrite_text::SearchQuery;

const MB: usize = 1024 * 1024;

fn editor_with(text: &str) -> (Editor, ferrite_core::DocumentId) {
  let mut editor = Editor::default();
  let id = editor.create_untitled();
  let document = editor.document_mut(id).expect("open");
  document
    .apply(EditorOp::Insert(text.to_owned()))
    .expect("applies");
  document
    .apply(EditorOp::Move {
      motion: Motion::DocumentStart,
      extend: false,
    })
    .expect("applies");
  (editor, id)
}

fn typing(c: &mut Criterion) {
  let mut group = c.benchmark_group("core/type_1000_chars");
  for (name, size) in [("empty", 0), ("10MB", 10 * MB)] {
    let text = code_text(size);
    group.bench_function(name, |b| {
      b.iter_batched(
        || editor_with(&text),
        |(mut editor, id)| {
          let document = editor.document_mut(id).expect("open");
          for i in 0..1000 {
            let op = if i % 8 == 7 {
              EditorOp::Insert(" ".into())
            } else {
              EditorOp::Insert("x".into())
            };
            document.apply(op).expect("applies");
          }
          editor
        },
        BatchSize::LargeInput,
      );
    });
  }
  group.finish();
}

fn undo_redo(c: &mut Criterion) {
  c.bench_function("core/undo_and_redo_1000_groups", |b| {
    b.iter_batched(
      || {
        let (mut editor, id) = editor_with(&code_text(MB));
        let document = editor.document_mut(id).expect("open");
        for _ in 0..1000 {
          document.apply(EditorOp::Newline).expect("applies");
          document
            .apply(EditorOp::Insert("word".into()))
            .expect("applies");
        }
        (editor, id)
      },
      |(mut editor, id)| {
        let document = editor.document_mut(id).expect("open");
        while document.apply(EditorOp::Undo).expect("applies") {}
        while document.apply(EditorOp::Redo).expect("applies") {}
        editor
      },
      BatchSize::LargeInput,
    );
  });
}

fn viewport(c: &mut Criterion) {
  let (mut editor, id) = editor_with(&code_text(50 * MB));
  let document = editor.document_mut(id).expect("open");
  let middle = document.buffer().line_count() / 2;
  let wanted = middle..middle + 80;

  let mut group = c.benchmark_group("core/viewport_update_80_lines_50MB");
  group.bench_function("full", |b| {
    b.iter(|| document.viewport_update(None, black_box(wanted.clone()), 1000))
  });

  let known = KnownLines {
    version: document.version(),
    ranges: vec![wanted.clone()],
  };
  let offset = document.buffer().line_start(middle + 10);
  document
    .apply(EditorOp::SetCursor {
      offset,
      extend: false,
      add: false,
    })
    .expect("applies");
  document
    .apply(EditorOp::Insert("x".into()))
    .expect("applies");
  group.bench_function("after_one_keystroke", |b| {
    b.iter(|| document.viewport_update(Some(black_box(&known)), wanted.clone(), 1000))
  });
  group.finish();
}

fn find(c: &mut Criterion) {
  let (mut editor, id) = editor_with(&code_text(10 * MB));
  let document = editor.document_mut(id).expect("open");
  let mut group = c.benchmark_group("core/find_10MB");
  group.sample_size(20);
  for (name, query) in [
    ("literal_common", SearchQuery::literal("buffer")),
    ("literal_rare", SearchQuery::literal("selection(cursor")),
    (
      "regex_word",
      SearchQuery {
        pattern: r"\bindex\.\w+".into(),
        regex: true,
        case_sensitive: true,
        whole_word: false,
      },
    ),
  ] {
    let searcher = query.compile().expect("compiles");
    group.bench_function(name, |b| {
      b.iter(|| document.match_summary(&searcher, 0..1000))
    });
  }
  group.finish();

  c.bench_function("core/replace_all_in_1MB", |b| {
    b.iter_batched(
      || editor_with(&code_text(MB)),
      |(mut editor, id)| {
        let document = editor.document_mut(id).expect("open");
        document
          .apply(EditorOp::ReplaceAll {
            query: SearchQuery::literal("value"),
            replacement: "amount".into(),
          })
          .expect("applies");
        editor
      },
      BatchSize::LargeInput,
    );
  });

  c.bench_function("core/delete_word_backward", |b| {
    b.iter_batched(
      || {
        let (mut editor, id) = editor_with(&code_text(MB));
        editor
          .document_mut(id)
          .expect("open")
          .apply(EditorOp::Move {
            motion: Motion::DocumentEnd,
            extend: false,
          })
          .expect("applies");
        (editor, id)
      },
      |(mut editor, id)| {
        editor
          .document_mut(id)
          .expect("open")
          .apply(EditorOp::Delete {
            direction: Direction::Backward,
            unit: Unit::Word,
          })
          .expect("applies");
        editor
      },
      BatchSize::LargeInput,
    );
  });
}

criterion_group! {
  name = benches;
  config = Criterion::default().warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
  targets = typing, undo_redo, viewport, find
}
criterion_main!(benches);
