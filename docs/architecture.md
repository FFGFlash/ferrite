# Architecture

This document describes Ferrite's architecture as it currently exists and how it is expected to
grow. The goals and constraints come from [`DESIGN.md`](DESIGN.md); decisions with lasting
consequences are recorded as ADRs in [`adr/`](adr/).

## Current state (Milestone 2)

```text
┌──────────────────────────── Desktop process (ferrite) ─────────────────────────────┐
│                                                                                    │
│  ┌─────────── WebView ───────────┐        ┌──────────── ferrite-app ────────────┐  │
│  │ React workbench (src/)        │        │ commands.rs   app info, logging     │  │
│  │  EditorView ─▶ EditorSession ─┼─ IPC ─▶│ editor/       document lifecycle,   │  │
│  │  (visible lines only)         │◀─view──│               editing commands,     │  │
│  │  tabs, status bar ◀─ stores   │        │               dialogs, clipboard    │  │
│  └───────────────────────────────┘        └──────┬──────────────────┬───────────┘  │
│                                                  │                  │              │
│                                    ┌─────────────▼──────┐   ┌───────▼───────────┐  │
│                                    │ ferrite-core       │──▶│ ferrite-workspace │  │
│                                    │ documents, cursors,│   │ load / save files │  │
│                                    │ ops, undo, dirty   │   └───────┬───────────┘  │
│                                    └─────────┬──────────┘           │              │
│                                              │   ┌──────────────────▼┐             │
│                                              └──▶│ ferrite-text      │             │
│                                                  │ rope, change sets,│             │
│                                                  │ positions, Unicode│             │
│                                                  └───────────────────┘             │
└────────────────────────────────────────────────────────────────────────────────────┘
```

## Crates

Crates use the `ferrite-` prefix. Dependencies point downward only. Only `ferrite-app` depends on
Tauri, and nothing in Rust depends on the frontend.

| Crate               | Responsibility                                                                                                        | Depends on                                            |
| ------------------- | --------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `ferrite-text`      | Rope-backed `TextBuffer`, validated `ChangeSet`s, versions, UTF-16 conversion, grapheme/word boundaries, line endings | `ropey`, `unicode-segmentation`                       |
| `ferrite-workspace` | Reading and atomically writing text files, external-change detection                                                  | `ferrite-text`                                        |
| `ferrite-core`      | `Editor` (open documents), `Document`, selections, `EditorOp`s, undo history, dirty state. No I/O.                    | `ferrite-text`, `ferrite-workspace` (for `FileStamp`) |
| `ferrite-app`       | Process lifecycle, Tauri commands, dialogs, clipboard, logging                                                        | all of the above, Tauri                               |

Planned: `ferrite-syntax` and `ferrite-lsp` (M5), `ferrite-extensions-api` and
`ferrite-extension-host` (M6). `ferrite-workspace` gains workspace roots, file discovery, and
file watching in M3. The design brief's `editor-app` and `src-tauri` are one crate
([ADR 0001](adr/0001-application-skeleton.md)).

## Text model

See [ADR 0002](adr/0002-text-model-and-editor-protocol.md) for the reasoning.

- **Positions are UTF-8 byte offsets** everywhere in Rust. Other units exist only at boundaries,
  as distinct types: `Utf16Position` (line + UTF-16 column) for the frontend now and LSP later.
- **Lines** are separated by LF, CRLF, or CR (Ropey with the `cr_lines` feature), matching LSP.
  Unicode separators such as U+2028 are ordinary characters.
- **Content is preserved byte-for-byte.** Mixed line endings are left alone. Each document detects
  its dominant line ending, and new line breaks (Enter, pasted or inserted text) use it. A UTF-8
  BOM is stripped on load and written back on save.
- **Edits** are `ChangeSet`s: sorted, non-overlapping `Edit { range, text }` in pre-edit
  coordinates, applied atomically. Ranges that are out of bounds or split a character are rejected
  with a structured `EditError`, and the buffer is left unchanged. Applying a change set returns
  its inverse, which history stores for undo.
- **Versions** increase with every content change, including undo and redo, and are never reused.
  They identify stale requests (frontend views now, extension edits and LSP later).
- **Cursor movement and deletion step over grapheme clusters**, so combining marks, emoji ZWJ
  sequences, and CRLF move as one unit. Word movement uses character classes (word, punctuation,
  whitespace), and a line break counts as a stop.

## Editor core

- **Selections**: each document has one or more `Selection { anchor, head, goal }`, kept sorted
  and merged when they overlap, with one primary. Every operation applies to all selections.
  Vertical movement keeps a visual goal column, with tabs expanded to tab stops.
- **Operations** (`EditorOp`): insert, newline (keeps indentation), tab (spaces to the next tab
  stop by default), delete by grapheme or word, motions, select all/word, set or add cursors, add
  a cursor above or below, undo, redo. Per-selection edits are merged into a single `ChangeSet`.
- **Undo history** stores each change as a forward/inverse pair in groups. Consecutive typing or
  deleting merges into one group. A group ends on cursor movement, a save, or whitespace typed
  after a word. Retention is bounded (`HistoryLimits`: 10,000 groups and 64 MiB of stored text by
  default), and the oldest groups are dropped first.
- **Dirty state** compares the current history state id with the saved one, so undoing back to
  the saved text makes the document clean again.
- **Lifecycle**: `Editor` owns documents by stable `DocumentId`s that are never reused. A file
  already open is found by path, so it isn't opened twice. Untitled documents are numbered
  `Untitled-N`.

## Files and saving

`ferrite-workspace` reads and writes files. `ferrite-app` coordinates, so the core stays free of
I/O.

- **Loading** accepts UTF-8 only. Other encodings are refused with an error that names the first
  invalid byte, rather than being decoded lossily. Files over 256 MiB are refused (`TooLarge`)
  until M4's large-file strategy.
- **Saving** takes a `SavePoint` (an O(1) rope snapshot plus the history state) under the editor
  lock, then writes it without the lock, so editing can continue. The file is written to a
  temporary file in the same directory, flushed with `fsync`, then renamed over the target. A
  crash therefore leaves either the old or the new file, never a partial one. Symlinks are
  followed. Read-only files are refused. Unix permission bits are copied to the new file.
- **External modification policy**: each document remembers the file's modification time and
  length from when it was loaded or saved. If they differ at save time, nothing is written and the
  user is asked whether to overwrite. A file deleted externally is recreated on save. Live file
  watching arrives with the workspace in M3.
- **Unsaved changes are never discarded silently.** Closing a dirty tab asks Save / Don't Save /
  Cancel. Closing the window with dirty documents asks once (Save All / Don't Save / Cancel); Save
  All prompts for a path for each untitled document, and any cancellation keeps the window open.
- **Save As** to a path that's open in another tab is refused, so two documents never share a
  path.

Hard links are not preserved (the rename replaces the link with a new file), and on Windows the
replacement file gets default attributes. A file changed externally within the filesystem's
timestamp granularity and with the same length isn't detected.

## The frontend–backend boundary

- **Rust owns authoritative state.** The frontend never holds a document's text; it renders views
  of the lines near the visible area.
- **Commands are the only entry point.** Tauri commands are defined in `src-tauri/src/commands.rs`
  and `src-tauri/src/editor/commands.rs`. They are mirrored by typed wrappers in `src/ipc/`, the
  only frontend code that calls `invoke`.
- **Payloads are explicit DTOs** (`src-tauri/src/editor/protocol.rs`) with camelCase fields, not
  core types.
- **Inputs are validated and bounded in Rust.** Positions are clamped and snapped to grapheme
  boundaries, batches are limited to 1,000 operations, and views to 1,000 lines and 10,000
  selections.
- **Errors** reach the frontend as `{ code, message }`, with stable codes such as `notUtf8`,
  `modifiedOnDisk`, and `unknownDocument`. The frontend shows them as notifications.
- **Browser fallback.** Outside the desktop shell (`pnpm dev`, tests) commands fail with code
  `unavailable` and the workbench still renders.

### Editor protocol

The frontend sends **operations**, not text diffs. Each response is the **view** for the caller's
viewport:

```text
editor_apply(id, ops[], viewport) → EditorView
EditorView = { version, lineCount, startLine, lines[], selections[], selectionCount,
               primary: { position, visualColumn }, dirty }
```

- Positions on the wire are `{ line, column }` with UTF-16 columns, which are JavaScript string
  indices into the rendered line. Rust converts them against the document as it is when each
  operation applies.
- `EditorSession` (`src/editor/session.ts`) keeps at most one request in flight. Operations
  issued in the meantime are sent together as the next batch, so ordering is preserved and fast
  typing costs few round trips. Consecutive drag positions collapse to the latest one.
- Each response contains only the viewport's lines (visible lines plus 20 lines of overscan
  above and below), not the document. Scrolling requests a new view only when the visible lines
  aren't already loaded.
- Before saving or closing, the workbench waits for the session to go idle, so the save includes
  every keystroke typed before it.

| Command                                      | Purpose                                                |
| -------------------------------------------- | ------------------------------------------------------ |
| `app_info`, `frontend_log`, `frontend_ready` | Process metadata, logging, startup timing              |
| `document_list`                              | Open documents (restores tabs after a webview reload)  |
| `document_new`                               | New untitled document                                  |
| `document_open`, `document_open_dialog`      | Open a path, or show the native open dialog            |
| `document_save`, `document_save_as`          | Save, prompting for a path or an overwrite when needed |
| `document_close`                             | Close, asking about unsaved changes                    |
| `editor_view`                                | View for a viewport (after scrolling)                  |
| `editor_apply`                               | Apply a batch of operations, return the new view       |
| `editor_clipboard`                           | Copy or cut the selection to the system clipboard      |

Files named on the command line (`ferrite path/to/file`) are opened during startup.

### Permissions, dialogs, and clipboard

The main window's capability grants **no** Tauri core or plugin permissions. File dialogs,
message dialogs, and clipboard writes (`tauri-plugin-dialog`, `tauri-plugin-clipboard-manager`)
are called from Rust, so the webview can't open arbitrary dialogs or read the clipboard. Copied
text goes from the backend straight to the clipboard. Paste uses the webview's native `paste`
event, which needs no permission.

The production Content Security Policy allows only same-origin scripts, IPC, and inline styles.
The development CSP additionally allows Vite's HMR socket and inline scripts.

## Frontend structure

- **`ipc/`**: typed command wrappers. `call()` normalizes failures to `CommandError`.
- **`state/`**: small observable stores (`createStore` + `useSyncExternalStore`): `documents`
  (tab summaries and the active id, plus actions), `editorStatus` (cursor position for the status
  bar), and `notifications`. Each component subscribes only to what it reads, so a keystroke
  re-renders the editor view and status bar, not the workbench.
- **`editor/EditorView`**: a scroll container sized to `lineCount × lineHeight`. Only the
  viewport's lines are in the DOM, positioned with a single `translateY`. Each line is one text
  node. Carets and selection rectangles are measured from the DOM with `Range` after each render,
  and clicks map back to columns with `caretPositionFromPoint`, so proportional glyphs, tabs, and
  wide characters line up without a custom text-layout engine.
- **Input**: a hidden textarea at the primary caret receives keys, text, IME composition (shown
  inline while composing), and paste. `editor/keymap.ts` maps keys to operations per platform
  (Ctrl on Windows/Linux, Cmd/Option on macOS). Workbench shortcuts (`app/shortcuts.ts`) are
  handled in the capture phase before the editor sees the key.
- **Theming**: `styles/tokens.css` defines semantic tokens for dark and light themes, selected by
  `data-theme` on `<html>`. Components use tokens only.
- **Accessibility**: the tab strip and tab panels use ARIA tab roles, unsaved tabs say so in their
  accessible name, errors are announced through `role="alert"`, and the sash is keyboard
  adjustable. Screen readers can't yet read the editor's text, because the input textarea is
  empty; that's planned for M8.

## Logging and diagnostics

`ferrite-app` installs a `tracing` subscriber during Tauri's `setup` hook, once the platform log
directory is known:

- Output goes to stderr (colored) and a daily rolling file through a non-blocking writer. Seven
  files are retained.
- `FERRITE_LOG` overrides the default `info` filter. Invalid directives fall back to the default
  with a warning.
- If the log directory can't be created, logging continues on stderr only. Logging problems never
  prevent startup.
- Panics are logged through `tracing` before the default hook runs.
- The file writer is flushed on `RunEvent::Exit`.
- Document lifecycle events (open, save, close) are logged at `info`, file errors at `warn`, and
  frontend errors are forwarded to the backend log.

## Startup measurement

`run()` records an `Instant` on entry. The backend logs `elapsed_ms` when setup finishes and when
the frontend first calls `frontend_ready`. Proper benchmarks come in M4.

Single, warm-cache runs on the development machine (Windows 11, x86_64), for orientation only:

| Build / scenario                                        | `backend initialized` | `frontend ready` |
| ------------------------------------------------------- | --------------------- | ---------------- |
| M1 debug                                                | 490 ms                | 673 ms           |
| M1 release                                              | 332 ms                | 372 ms           |
| M2 debug, 11 MB / 200,000-line file on the command line | 333 ms                | 629 ms           |

The M1 release `ferrite.exe` process had a working set of about 30 MB. WebView2 runs in separate
processes, which this figure excludes.

## Known limitations

These are tracked for later milestones:

- Very long lines are sent and rendered whole. A single multi-megabyte line in the viewport makes
  every keystroke slow (M4).
- Files are loaded completely and synchronously when opened; there's no lazy loading or
  large-file mode yet (M4).
- Closing a tab discards its scroll position. Tabs can't be reordered, and there's no tab
  overflow menu (M3).
- Multi-line Tab/Shift+Tab indentation, auto-closing brackets, and copying the whole line when
  nothing is selected aren't implemented. Alt+drag doesn't create column selections.
- Errors opening files named on the command line are only logged, not shown.
