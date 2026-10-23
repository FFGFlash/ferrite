//! Text storage: edits, position conversion, line change tracking, and
//! building buffers from large inputs.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ferrite_benches::code_text;
use ferrite_text::{ChangeSet, Edit, TextBuffer, Utf16Position};

const MB: usize = 1024 * 1024;

fn edits(c: &mut Criterion) {
  let mut group = c.benchmark_group("text/insert_and_delete_char");
  for size in [MB, 100 * MB] {
    let mut buffer = TextBuffer::from_text(&code_text(size));
    let middle = buffer.line_start(buffer.line_count() / 2);
    let insert = ChangeSet::single(Edit::insert(middle, "x")).expect("valid");
    let delete = ChangeSet::single(Edit::delete(middle..middle + 1)).expect("valid");
    group.bench_function(
      BenchmarkId::from_parameter(format!("{}MB", size / MB)),
      |b| {
        b.iter(|| {
          buffer.apply(black_box(&insert)).expect("applies");
          buffer.apply(black_box(&delete)).expect("applies");
        });
      },
    );
  }
  group.finish();
}

fn positions(c: &mut Criterion) {
  let buffer = TextBuffer::from_text(&code_text(100 * MB));
  let offset = buffer.len() * 3 / 4;
  let position = buffer.offset_to_utf16(offset);
  let mut group = c.benchmark_group("text/utf16_conversion_100MB");
  group.bench_function("offset_to_utf16", |b| {
    b.iter(|| buffer.offset_to_utf16(black_box(offset)))
  });
  group.bench_function("utf16_to_offset", |b| {
    b.iter(|| {
      buffer.utf16_to_offset(black_box(Utf16Position {
        column: 10,
        ..position
      }))
    })
  });
  group.finish();
}

fn line_changes(c: &mut Criterion) {
  let mut buffer = TextBuffer::from_text(&code_text(MB));
  let since = buffer.version();
  for i in 0..100 {
    let at = buffer.line_start(i * 10);
    buffer
      .apply(&ChangeSet::single(Edit::insert(at, "x\n")).expect("valid"))
      .expect("applies");
  }
  c.bench_function("text/line_changes_since_100_edits", |b| {
    b.iter(|| buffer.line_changes_since(black_box(since)))
  });
}

fn build(c: &mut Criterion) {
  let mut group = c.benchmark_group("text/build_buffer");
  group
    .sample_size(10)
    .measurement_time(Duration::from_secs(10));
  for size in [10 * MB, 100 * MB] {
    let text = code_text(size);
    group.bench_function(
      BenchmarkId::from_parameter(format!("{}MB", size / MB)),
      |b| b.iter(|| TextBuffer::from_text(black_box(&text))),
    );
  }
  group.finish();
}

criterion_group! {
  name = benches;
  config = Criterion::default().warm_up_time(Duration::from_secs(1)).measurement_time(Duration::from_secs(3));
  targets = edits, positions, line_changes, build
}
criterion_main!(benches);
