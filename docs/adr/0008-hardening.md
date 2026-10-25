# ADR 0008: Hardening

- **Status:** Accepted
- **Date:** 2026-10-25

## Context

Milestone 8 asks for a stable foundation. The brief lists error handling, crash recovery,
extension isolation, resource accounting, cross-platform behavior, accessibility, performance
regression testing, and documentation.

Reviewing the code before this milestone found these gaps:

- **Crash recovery.** A crash lost all unsaved changes.
- **Error isolation in the frontend.** A rendering error in any component unmounted the whole
  workbench and left an empty window.
- **Silent failures.** Problems found during startup (files that couldn't be reopened) were only
  logged.
- **Unsafe writes.** Settings edits weren't written atomically.
- **Exit on macOS.** Quit (`Cmd+Q`) could bypass the unsaved-changes prompt.
- **Screen readers** couldn't read the editor's text.
- **Extension memory** was capped per extension, but not in total.
- **No visibility** into what each extension costs.
- **Regression checks.** Nothing ran the tests on other platforms or checked benchmark results
  against a baseline.

## Decisions

### Crash recovery: backups per session, recognized by a lock

Unsaved documents are backed up continuously into a folder per running instance. A lock file
in it is held for the instance's lifetime.

- **Recognizing a crash.** The operating system releases a lock when its process ends, however
  it ends. So at startup, any session folder whose lock can be taken belongs to an instance that
  crashed or was killed, and its backups are restored.
- **Alternatives rejected.** A "clean exit" marker file, or a check whether a recorded process
  id is still running, would misjudge PID reuse and fail with several instances.
- **Clean exits.** A clean exit (everything saved, or the user chose **Don't Save**) removes the
  folder, so discarded changes never come back.
- **How backups are made.** The backup thread takes rope snapshots (O(1)) under the editor lock
  and writes them without it, so typing never waits for the disk.
- **Large documents** are backed up at most once per their size at 4 MB/s. A large file being
  edited doesn't cause continuous disk writes, and small files are still backed up within a
  second.
- **How a restored document looks.** It opens with its file's text, with the backup applied as
  one undoable change. The user sees exactly what was unsaved, undo leads back to the file, and
  saving keeps the usual conflict check.

This is not hot exit: a normal close still asks about unsaved changes.

### Errors stay where they happen

- **Frontend.** Error boundaries surround the workbench, each side bar view, the editor, and the
  status bar. A failure replaces only that part, with **Try Again** and **Reload Window**.
  Reloading is safe because all document state lives in the backend.
- **Startup problems.** Problems found before the frontend subscribes to events are queued and
  delivered once it does.
- **Notification timing.** Warnings and errors stay until dismissed. Information pauses while
  hovered or focused (WCAG 2.2.1, timing adjustable).

### Extensions: a shared memory pool and per-extension accounting

- **Memory.** Instances reserve their linear memory in a pool shared by all extensions (512 MiB),
  besides each instance's own limit (128 MiB). Ten installed extensions can no longer use
  1.28 GB between them. An extension refused by the pool fails with its own message, so the user
  can tell the difference from exceeding its own limit.
- **Accounting.** Per extension, the host counts busy time, calls, the slowest call, memory,
  dropped events, and compile time. The counters are lock-free atomics updated by the worker, so
  accounting costs one `Instant::now()` per call. The Extensions view shows them in a sentence.
  The performance overlay shows them as a table, next to the editor's own timings.

### Screen readers read a mirror of the cursor's line

The input textarea holds the cursor's line (or the selected lines, at most 100), with the
editor's selection. Screen readers read an edit field's text and caret natively, so moving,
selecting, and typing are announced without any extra announcements.

- **Alternative rejected.** A live region announcing lines and characters would fight the
  screen reader's own reading of the (empty) field.
- **Placement.** The textarea is laid over the rendered line, at the same font and tab size, so
  its text lines up with the visible text. IME candidate windows appear at the cursor, and
  in-progress composition text is shown in place.
- **Input.** The textarea never edits itself. `beforeinput` is cancelled and applied as editor
  operations; composition (which can't be cancelled) is applied when it ends, and the mirror
  follows each view. A browser that applies input anyway is handled by diffing against the
  mirror.
- **Forced colors** (Windows contrast themes) and **reduced motion** are supported through one
  stylesheet. axe-core checks the main surfaces in the test suite.

### Regression testing: count work in CI, time it on a known machine

- **Shared CI runners** are too noisy for timing comparisons. Tests that count work (lines and
  bytes sent per keystroke and per page in a 200,000-line document) are deterministic, so they
  run everywhere. They catch the regressions that matter most for responsiveness: resending
  lines, growing views.
- **Timing comparisons** run on a developer machine. `scripts/perf-check.mjs` compares an
  end-to-end run with a committed baseline (`benches/baseline.json`). Each kind of metric has its
  own tolerance: paint latency may not grow by a frame, other timings by 25%, memory by 15%, the
  binary by 10%. A second mode reads Criterion's comparison with a saved baseline and fails a
  benchmark only when even the optimistic end of its confidence interval is past the threshold.
- **Platforms.** CI runs formatting, Clippy, and all tests on Windows, macOS, and Linux.

## Consequences

- **Crashes.** A crash, forced shutdown, or `kill` loses at most about a second of typing.
  Backups take disk space only while documents are unsaved.
- **Rendering failures** in one part of the workbench no longer take down the others.
- **Accessibility.** The editor is usable with a screen reader for reading and editing.
  Diagnostics, hover cards, and completion items aren't announced yet.
- **Regression checks.** Performance regressions in what is sent per keystroke fail CI on every
  platform. Timing regressions are caught when someone runs the check, not automatically.
- **Limitation of the baseline.** It is per machine: results from another machine are compared
  with a warning, not refused.
