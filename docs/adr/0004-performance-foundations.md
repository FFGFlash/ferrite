# ADR 0004: Performance foundations

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

Milestone 4 asks for efficient frontend updates, viewport-oriented rendering,
large-file handling, incremental document notifications, background task
scheduling, instrumentation, and initial benchmarks. The brief says to measure
before optimizing and to avoid unsupported performance claims, so this ADR
records the strategy. Numbers live in [`performance.md`](../performance.md).

## Decisions

### One bounded worker pool for all background work

`flareite-tasks` provides a `Scheduler`: a fixed set of worker threads
(cores − 1, between one and four), three priorities, a bounded queue
(256), cooperative cancellation, and **keyed replacement**. Submitting a task
with the same key as a queued or running one cancels the older task. Indexing,
find in files, background match counting, reloads after file changes, and
opening startup files all run on it.

Alternatives considered:

- **Per-feature threads** (what M3 did for indexing) and Tokio's blocking pool
  (what M3 did for search). Neither bounds the total, and neither gives one
  place to prioritize or cancel work.
- **Rayon.** Rayon is designed for data parallelism, not for long-lived,
  cancellable, prioritized jobs.

Keyed replacement gives "only the latest request matters" (search-as-you-type,
match counting while typing) without every caller tracking cancel tokens.
Panics in tasks are caught and counted, so a failing task can't take a worker
down.

### Incremental views through line splices

`TextBuffer::apply` records, per version, how lines moved (`LineSplice`:
lines `start..start+removed` became `start..start+inserted`), in a bounded log
of 512 changes. The frontend tells the backend which lines it holds (version
and range). The backend replays the splices to work out which of those lines
are still valid and sends text only for the rest. The client replays the same
splices on its cache.

- A keystroke transfers one line instead of the whole viewport.
- Pressing Enter shifts cached lines instead of resending them.
- Scrolling transfers only newly exposed lines.

The same splice log is what Tree-sitter (M5), LSP synchronization, and
extension change notifications (M6) need, so it lives in `flareite-text`.

Both sides check consistency: a client too far behind gets a full refresh, and
an update the client can't apply makes it drop its cache and ask again. A
protocol bug therefore costs a round trip, never wrong text on screen.

Alternatives considered:

- **Diffing line contents on the client.** This requires sending the lines
  anyway.
- **Stable line IDs.** These would need bookkeeping on every edit in the rope.

### Large files: explicit limits, never silent changes

- Files up to 256 MiB open. They load off the main thread, including at
  startup.
- Lines longer than 16 KiB are **shortened for display** and marked with a
  "…" badge showing the full length. Editing, saving, and search always use
  the full text.
- Visual columns are exact within the first 4 KiB of a line and count
  characters beyond that, so cursor movement on huge lines stays O(log n).
- Find counts in documents over 1 MiB run in the background on a snapshot. The
  editor shows visible matches immediately and "Counting…" until the total
  arrives. A newer request cancels an older count.

### Measure in two layers

- **Microbenchmarks** (Criterion, `benches/`) cover text, core, file, and
  workspace operations on generated, deterministic inputs.
- **End-to-end benchmarks** (`scripts/bench.mjs`) run the release app with
  `--bench <name>`. The frontend drives the real editor, timing keystroke to
  paint (including IPC and rendering) and page jumps through 200,000 lines.
  The harness measures startup and the memory of the whole process tree
  (the app plus its WebView2 or WebKit processes).
- **Always-on instrumentation**, shown in an overlay (**Developer: Toggle
  Performance Overlay**), covers backend timing histograms (O(1) memory,
  power-of-two buckets), scheduler statistics per task type, frontend
  round-trip, render, and input-to-paint series, and process memory.

Benchmark runs use an ephemeral session, so they never overwrite the user's
saved state.

## Consequences

- Frontend correctness now depends on the splice protocol. Both sides are unit
  tested, and the client recovers from any mismatch by refetching.
- Very long lines are only partly visible. The badge makes that explicit; full
  long-line rendering (windowing horizontally around the cursor) can come later.
- On machines with one or two cores, background work gets a single worker, so
  a find-in-files search can delay an index rebuild. Priorities keep
  user-visible work first.
- Optimization targets are set against the measured baseline in
  `performance.md`, not up front.
