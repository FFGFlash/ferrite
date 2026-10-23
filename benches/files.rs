//! Opening and saving files: reading and validating UTF-8, building the
//! buffer, and atomic saves.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ferrite_benches::code_text;
use ferrite_core::Editor;
use ferrite_text::TextBuffer;
use ferrite_workspace::{load_text_file, save_text_file};

const MB: usize = 1024 * 1024;

fn open(c: &mut Criterion) {
  let dir = tempfile::tempdir().expect("temp dir");
  let mut group = c.benchmark_group("files/open");
  group
    .sample_size(10)
    .measurement_time(Duration::from_secs(10));
  for size in [MB, 10 * MB, 100 * MB] {
    let path = dir.path().join(format!("{size}.rs"));
    std::fs::write(&path, code_text(size)).expect("write fixture");
    group.bench_function(
      BenchmarkId::from_parameter(format!("{}MB", size / MB)),
      |b| {
        b.iter(|| {
          // What opening a document does: load (read + UTF-8 check) and build the rope.
          let mut editor = Editor::default();
          let loaded = load_text_file(black_box(&path)).expect("loads");
          editor.open(path.clone(), loaded);
          editor
        })
      },
    );
  }
  group.finish();
}

fn save(c: &mut Criterion) {
  let dir = tempfile::tempdir().expect("temp dir");
  let mut group = c.benchmark_group("files/save");
  group
    .sample_size(10)
    .measurement_time(Duration::from_secs(10));
  for size in [MB, 10 * MB] {
    let path = dir.path().join(format!("{size}.rs"));
    let snapshot = TextBuffer::from_text(&code_text(size)).snapshot();
    group.bench_function(
      BenchmarkId::from_parameter(format!("{}MB", size / MB)),
      |b| b.iter(|| save_text_file(black_box(&path), &snapshot, false, None).expect("saves")),
    );
  }
  group.finish();
}

criterion_group! {
  name = benches;
  config = Criterion::default().warm_up_time(Duration::from_secs(1));
  targets = open, save
}
criterion_main!(benches);
