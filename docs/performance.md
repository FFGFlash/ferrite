# Performance

This document describes how Ferrite's performance is measured, the baseline
measured at Milestone 4, and how future optimizations should be chosen. The
reasoning behind the architecture is in
[ADR 0004](adr/0004-performance-foundations.md).

All numbers below are single-machine measurements. They are a baseline to
compare against, not claims about other machines or other editors.

## How to measure

| What                                           | Command                                                                          |
| ---------------------------------------------- | -------------------------------------------------------------------------------- |
| Microbenchmarks (text, core, files, workspace) | `pnpm bench` (Criterion; reports in `target/criterion/`)                         |
| One suite or benchmark                         | `cargo bench -p ferrite-benches --bench workspace -- search`                     |
| Compare with a saved baseline                  | `cargo bench -p ferrite-benches -- --save-baseline m4`, later `-- --baseline m4` |
| End-to-end: startup and idle memory            | `pnpm tauri build --no-bundle`, then `pnpm bench:app startup`                    |
| End-to-end: typing latency                     | `pnpm bench:app typing`                                                          |
| End-to-end: large-document scrolling           | `pnpm bench:app scroll`                                                          |
| End-to-end: opening a file                     | `pnpm bench:app open --file path/to/file`                                        |
| Live statistics while using the app            | Command palette → **Developer: Toggle Performance Overlay**                      |

The end-to-end harness (`scripts/bench.mjs`) launches the release build with
`--bench <name>`. The frontend drives the real editor (IPC, React rendering,
DOM measurement, and painting included) and reports its timings. While the
app waits to exit, the harness measures the memory of the whole process tree:
the app plus its WebView processes. Benchmark runs use an empty, ephemeral
session and never touch your saved state. Results are written to
`target/bench/results.json`.

### Fixtures

Inputs are generated deterministically (`benches/src/lib.rs`), not checked
in:

- **Code-like text:** indented lines of 40–100 characters of keywords and
  punctuation.
- **Synthetic workspace:** 10,000 source files of about 2 KiB in a nested
  layout, plus a `.gitignore`d `target/` folder that searches must skip.
- **Synthetic path lists:** 100,000 paths for Go to File.
- **Large documents** for the end-to-end benchmarks: generated in the app
  (`bench_generate`), 200,000 lines of 80 columns (about 15.5 MB).

### Things that skew results

- **Overlapping launches.** Back-to-back startups while the previous
  instance's WebView processes are still exiting roughly double startup time;
  the harness pauses 3 s between runs (`--pause`).
- **Antivirus.** Real-time scanning (Windows Defender) adds a large per-file
  cost to opening files, which dominates find in files over many small files.
- **Display refresh rate.** Keystroke-to-paint latency can't be lower than the
  wait for the next frame. The development machine runs at 75 Hz (13.3 ms
  frames).
- **Cold starts.** The OS file cache is warm after the first run. A true cold
  start (after a reboot) isn't automated; the first run of a session is
  reported separately as the closest approximation.

## Baseline (Milestone 4)

Development machine: AMD Ryzen 7 5800X (8 cores, 16 threads), 32 GB RAM,
Windows 11 (build 26200), 75 Hz display. Release build, Rust 1.99,
WebView2 155.

### Startup and memory

| Measurement                                   | Result                          |
| --------------------------------------------- | ------------------------------- |
| Launch → frontend ready, first run            | 403 ms                          |
| Launch → frontend ready, 5 later runs         | 401 ms median (363–472 ms)      |
| Idle memory, whole process tree (7 processes) | 389 MB resident, 193 MB private |
| Idle memory, Ferrite backend process          | 34 MB resident                  |
| Backend with a 15.5 MB document open          | 67 MB resident                  |
| Backend with a 100 MB document open           | 149 MB resident                 |

Most idle memory belongs to WebView2 (browser, GPU, renderer, and utility
processes). "Resident" double-counts pages shared between those processes;
"private" is closer to what Ferrite costs the system. Memory with extensions
is measured once extensions exist (M6).

### Editing and rendering (end to end)

| Measurement                                             | p50     | p95     | max     |
| ------------------------------------------------------- | ------- | ------- | ------- |
| Keystroke → painted, small document                     | 13.3 ms | 13.7 ms | 26.8 ms |
| Keystroke → painted, 200,000-line document              | 13.3 ms | 13.7 ms | 19.8 ms |
| IPC round trip (request → view received)                | 2.0 ms  | 6.1 ms  | 21.8 ms |
| Backend `editor_apply` (apply the edit, build the view) | 0.10 ms | 0.26 ms | 1.5 ms  |
| Frontend render and measure                             | 0.4 ms  | 0.6 ms  | 3.3 ms  |
| Lines transferred per keystroke                         | 1       | 1       | —       |
| Page jump (~1,000 lines) → painted, 200,000 lines       | 26.6 ms | 28.0 ms | 36.4 ms |
| Lines transferred per page jump                         | 78      | 78      | —       |

Other end-to-end measurements:

- A 300-character typing burst (dispatched without waiting) settles in
  13.4 ms: the session batches it into a single request.
- Jumping to the end of a 200,000-line document paints in 13.0 ms.
- Opening a 100 MB file takes 90 ms to load and 111 ms to the first paint.

Keystroke latency is one frame: the work (≈2.5 ms) finishes well before the
next display refresh, so the measurement is the frame wait. A page jump is two
frames: the browser delivers the scroll event one frame after the scroll
position changes, then paints the new lines on the next. Ordinary wheel
scrolling stays within the 20 lines fetched above and below the viewport, so
it needs no request.

### Microbenchmarks (Criterion)

Median of Criterion's estimate.

| Benchmark                                                      | Time                          |
| -------------------------------------------------------------- | ----------------------------- |
| Insert + delete one character, 1 MB document                   | 1.7 µs                        |
| Insert + delete one character, 100 MB document                 | 2.4 µs                        |
| Offset → UTF-16 position, 100 MB                               | 0.36 µs                       |
| UTF-16 position → offset, 100 MB                               | 0.83 µs                       |
| Line changes since a version (100 edits)                       | 0.61 µs                       |
| Build a buffer from 10 MB / 100 MB of text                     | 3.9 ms / 66 ms                |
| Type 1,000 characters (core), empty document                   | 3.6 ms                        |
| Type 1,000 characters (core), 10 MB document                   | 1.8 ms                        |
| Undo and redo 1,000 groups                                     | 4.7 ms                        |
| Viewport update, 80 lines of a 50 MB document, full            | 46 µs                         |
| Viewport update after one keystroke (incremental)              | 0.81 µs                       |
| Find all, 10 MB, common literal                                | 1.0 ms                        |
| Find all, 10 MB, rare literal                                  | 1.9 ms                        |
| Find all, 10 MB, regex `\bindex\.\w+`                          | 2.5 ms                        |
| Replace all of a common word in 1 MB                           | 8.7 ms                        |
| Open (read, validate, build) 1 / 10 / 100 MB                   | 0.77 / 7.4 / 89 ms            |
| Save (atomic, with fsync) 1 / 10 MB                            | 17 / 53 ms                    |
| Discover and index 10,000 files                                | 218 ms                        |
| Find in 10,000 files, rare literal                             | 285 ms (982 ms on one thread) |
| Find in 10,000 files, regex `fn\s+\w+\(`                       | 291 ms (996 ms on one thread) |
| Find in 10,000 files, common literal (stops at 20,000 matches) | 51 ms (140 ms on one thread)  |
| List a directory of 100 entries                                | 2.4 ms                        |
| Go to File over 100,000 paths                                  | 5–9 ms                        |

## Optimizations made from these measurements

1. **Parallel find in files.** On one thread, searching 10,000 small files took
   about 1 s, although searching the same 20 MB in memory takes about 4 ms (extrapolated
   from the 10 MB benchmark) and
   walking the tree takes 218 ms. The time went to opening and reading files
   one at a time. Searching with up to four walker threads cut it to 285 ms
   (3.4×).
2. **Incremental views.** The backend now sends only lines the frontend lacks:
   one line per keystroke instead of the whole viewport (about 80 lines), and
   0.8 µs instead of 46 µs to build.
3. **Background match counting.** Counting every find match in a large
   document would scan it on every keystroke. Documents over 1 MiB show
   visible matches at once and count on the worker pool.

One hypothesis turned out to be wrong. Removing an extra animation-frame wait
before fetching lines on scroll didn't change page-jump latency; the two
frames come from scroll event delivery, as described above. The simpler code
was kept.

## Strategy for further optimization

- **Measure first.** Add or extend a benchmark that shows the problem, save a
  Criterion baseline, change one thing, and compare.
- **Latency budget.** Typing must stay within one frame end to end. Today about
  2.5 ms of the 13.3 ms frame is used, mostly by IPC. Features that run on every
  keystroke (syntax highlighting in M5, extension notifications in M6) get the
  remaining budget only if they're incremental. Anything else goes on the
  worker pool with keyed replacement.
- **Watch these numbers as features arrive:**
  - IPC round-trip time, since it's the largest share of a keystroke;
  - backend `editor.apply` p95;
  - process memory with large documents (the undo history keeps edited text);
  - idle WebView memory.
- **Known opportunities, not yet needed:**
  - a binary or more compact view encoding if IPC grows;
  - saving without `fsync` for very large files (a durability trade-off to
    decide deliberately);
  - prefetching neighboring pages during fast scrolling;
  - horizontal windowing for lines longer than 16 KiB;
  - parallel indexing for very large workspaces.
