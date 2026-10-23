# Architecture

This document describes Ferrite's architecture as it currently exists and how it is expected to
grow. The goals and constraints come from [`DESIGN.md`](DESIGN.md); decisions with lasting
consequences are recorded as ADRs in [`adr/`](adr/).

## Current state (Milestone 5)

```text
┌──────────────────────────── Desktop process (ferrite) ─────────────────────────────┐
│                                                                                    │
│  ┌─────────── WebView ───────────┐        ┌──────────── ferrite-app ────────────┐  │
│  │ React workbench (src/)        │        │ commands.rs   app info, logging     │  │
│  │  EditorView ─▶ EditorSession ─┼─ IPC ─▶│ editor/       documents, editing,   │  │
│  │  (visible lines, highlights,  │◀─view──│               syntax state          │  │
│  │   squiggles, hover, suggest)  │        │ lsp/          servers, sync, hover, │  │
│  │  palette, explorer, search,   │◀─events│               definition, complete  │  │
│  │  tabs, status bar ◀─ stores   │ channel│ workspace.rs, settings.rs, ...      │  │
│  └───────────────────────────────┘        └──┬─────────┬──────────┬─────────┬───┘  │
│                                              │         │          │         │      │
│        ┌─────────────────┐  ┌────────────────▼─┐ ┌─────▼────────┐ │  ┌──────▼────┐ │
│        │ ferrite-syntax  │◀─┤ ferrite-core     │ │ ferrite-lsp  │ │  │ ferrite-  │ │
│        │ Tree-sitter     │  │ documents, ops,  │ │ JSON-RPC,    │ │  │ workspace │ │
│        │ parse, highlight│  │ find, undo       │ │ positions    │ │  │           │ │
│        └───────┬─────────┘  └────────┬─────────┘ └──────┬───────┘ │  └─────┬─────┘ │
│                └─────────────────────▼──────────────────▼─────────▼────────▼       │
│                                   ferrite-text: rope, change sets, edit log        │
└──────────────────────────────────────────────────────────┬─────────────────────────┘
                                                            │ stdio
                                           language servers (rust-analyzer, ...)
```

## Crates

Crates use the `ferrite-` prefix. Dependencies point downward only. Only `ferrite-app` depends on
Tauri, and nothing in Rust depends on the frontend.

| Crate               | Responsibility                                                                                                                                                                                                  | Depends on                                                                     |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `ferrite-text`      | Rope-backed `TextBuffer`, validated `ChangeSet`s, versions, the line-splice log, UTF-16 conversion, grapheme/word boundaries, line endings, regex/literal search over the rope                                  | `ropey`, `unicode-segmentation`, `regex-cursor`                                |
| `ferrite-workspace` | Reading and atomically writing files, directory listings with ignore rules, the Go to File index, find in files, file watching                                                                                  | `ferrite-text`, `ignore`, `globset`, `nucleo-matcher`, `notify-debouncer-mini` |
| `ferrite-core`      | `Editor` (open documents), `Document`, selections, `EditorOp`s (including find/replace), undo history, dirty state, reload from disk, incremental viewport updates. No I/O.                                     | `ferrite-text`, `ferrite-workspace` (for `FileStamp`)                          |
| `ferrite-syntax`    | Bundled Tree-sitter grammars and highlight queries (Rust, TypeScript/TSX, JavaScript, JSON, TOML, Python), language detection, incremental and interruptible parsing, per-line highlight spans                  | `ferrite-text`, `tree-sitter`, grammar crates                                  |
| `ferrite-lsp`       | Language server processes and JSON-RPC over stdio, UTF-8/UTF-16 position conversion, incremental document changes from the edit log, known servers and workspace roots, a scripted test server                  | `ferrite-text`, `lsp-types`, `serde_json`                                      |
| `ferrite-tasks`     | Bounded background scheduler (priorities, keyed replacement, cancellation) and timing histograms                                                                                                                | `tracing`                                                                      |
| `ferrite-app`       | Process lifecycle, Tauri commands, dialogs, clipboard, settings, session state, workspace coordination, syntax and language server state per document, backend events, instrumentation, benchmark mode, logging | all of the above, Tauri, `toml`, `toml_edit`, `memory-stats`                   |
| `ferrite-benches`   | Criterion benchmarks and deterministic fixtures (`benches/`), not part of the app                                                                                                                               | `ferrite-text`, `ferrite-core`, `ferrite-workspace`, `criterion`               |

Planned: `ferrite-extensions-api` and `ferrite-extension-host` (M6). The design brief's `editor-app` and `src-tauri` are one crate
([ADR 0001](adr/0001-application-skeleton.md)). Workspace, settings, and command decisions are in
[ADR 0003](adr/0003-workbench-services.md), and performance decisions in
[ADR 0004](adr/0004-performance-foundations.md), and syntax and language server decisions in
[ADR 0005](adr/0005-syntax-and-language-servers.md). Measurements are in
[`performance.md`](performance.md).

## Background work

All background work runs on one `ferrite-tasks` scheduler owned by the app: a fixed pool of
worker threads (CPU cores − 1, between one and four) with three priorities and a queue of at most
256 tasks. When the queue is full, the least important, oldest task is dropped.

- Tasks are **keyed**. A new task with the same key cancels a queued or running one, so only the
  latest search, match count, or reload of a file runs.
- Cancellation is cooperative through a `CancelToken`, which long loops (walks, scans) check.
- A panic in a task is caught and counted, and the worker keeps running.
- On exit, the scheduler is shut down: tasks are cancelled and workers joined.

| Task                  | Priority | Key                  |
| --------------------- | -------- | -------------------- |
| `open-startup-files`  | High     | none                 |
| `workspace-search`    | High     | one search at a time |
| `count-matches`       | High     | one per document     |
| `reload-changed-file` | High     | one per path         |
| `workspace-index`     | Normal   | one index at a time  |

Find in files additionally uses up to four walker threads of its own inside its task, because it
is dominated by opening many small files (3.4× faster in the benchmark). The editor commands
themselves stay synchronous: they take under a millisecond and must apply in order.

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
  a cursor above or below, undo, redo, find next/previous, select all matches, replace, replace
  all (one undo step, with `$1`/`${name}` expansion for regex queries), and add next occurrence
  (Ctrl+D). Per-selection edits are merged into a single `ChangeSet`.
- **Find** compiles a `SearchQuery` (literal or regex; case-sensitive; whole word) with
  `regex-cursor`, which searches the rope's chunks directly without copying the text. `^`/`$`
  match per line, including before CRLF. Counts stop at 20,000 matches and are reported as
  truncated.
- **Undo history** stores each change as a forward/inverse pair in groups. Consecutive typing or
  deleting merges into one group. A group ends on cursor movement, a save, or whitespace typed
  after a word. Retention is bounded (`HistoryLimits`: 10,000 groups and 64 MiB of stored text by
  default), and the oldest groups are dropped first.
- **Dirty state** compares the current history state id with the saved one, so undoing back to
  the saved text makes the document clean again.
- **Lifecycle**: `Editor` owns documents by stable `DocumentId`s that are never reused. A file
  already open is found by path, so it isn't opened twice. Untitled documents are numbered
  `Untitled-N`.
- **Reload** replaces only the differing middle of the text (common prefix and suffix kept), so
  cursors before and after the change stay on their text. The reload is recorded in history, so
  it can be undone, and the document is marked saved at the new disk state.

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
  user is asked whether to overwrite. A file deleted externally is recreated on save. While a
  workspace is open, its files are also watched (see below).
- **Unsaved changes are never discarded silently.** Closing a dirty tab asks Save / Don't Save /
  Cancel. Closing the window with dirty documents asks once (Save All / Don't Save / Cancel); Save
  All prompts for a path for each untitled document, and any cancellation keeps the window open.
- **Save As** to a path that's open in another tab is refused, so two documents never share a
  path.

Hard links are not preserved (the rename replaces the link with a new file), and on Windows the
replacement file gets default attributes. A file changed externally within the filesystem's
timestamp granularity and with the same length isn't detected.

## Workspace

`ferrite-app/src/workspace.rs` owns the open folder (one at a time). Opening one starts:

- **Exclusions and ignore rules.** `files.exclude` globs (default `.git`, `.DS_Store`,
  `Thumbs.db`) are hidden everywhere. `.gitignore` (in git repositories), `.ignore`, and global
  git excludes come from ripgrep's `ignore` crate. Ignored entries are **shown dimmed** in the
  explorer but skipped by the file index and search.
- **Explorer listings** are fetched one directory at a time when a folder is expanded, never by
  walking the whole tree. Directories are listed first, sorted case-insensitively. Listing
  requests outside the workspace root are refused.
- **The Go to File index** is built on the scheduler (`workspace-index`) and is cancelled if
  the workspace changes. It holds relative paths only, up to 200,000 files (beyond that it's
  marked truncated). Queries are fuzzy-matched with `nucleo-matcher` and return the best 100,
  with match positions for highlighting.
- **File watching** (`notify` with a 250 ms debounce) keeps everything current incrementally. The
  index adds and removes only the changed paths, the explorer re-lists only the loaded directories
  that changed, and open documents react:
  - unchanged stamp (Ferrite's own save): ignored;
  - **clean** document: reloaded in place;
  - **dirty** document: left untouched, and the user is told that saving will ask before
    overwriting.
- **Find in files** runs on the scheduler, searches files in parallel, and streams results file
  by file through a Tauri `Channel`. Starting a new search cancels the previous one. Open documents are searched as edited
  (including unsaved changes). Binary files (NUL in the first 8 KiB), non-UTF-8 files, and files
  over `search.max_file_bytes` (4 MiB) are skipped. A search stops at 20,000 matches, or 1,000 per
  file. **Replace across files is not implemented yet** ([ADR 0003](adr/0003-workbench-services.md)).

## Settings and session state

- **`settings.toml`** lives in the platform config directory (e.g. `%APPDATA%\dev.ferrite.editor`)
  and is edited in Ferrite itself (**Preferences: Open Settings**, `Ctrl+,`). It's created from a
  commented template the first time it's opened. Saving it validates and applies it immediately:
  tab size and spaces apply to open documents, font and theme to the frontend, and exclusions
  rebuild the index. Invalid settings (unknown keys, out-of-range values, bad globs) are reported
  with the parser's message and the previous settings stay in effect. **Color Theme** commands
  edit the file with `toml_edit`, preserving comments and formatting.
- **`state.json`** in the local data directory holds the open folder, open files, the active file,
  and the frontend's layout (side bar visibility, width, active view, expanded folders). It's
  written atomically on every change; the frontend debounces its layout updates by 500 ms. On
  launch the folder and files are reopened, then any files named on the command line, the last of
  which becomes active. Unsaved changes are not persisted, so closing still asks about them.

## Commands and keybindings

All workbench actions are entries in a frontend **command registry** (`src/commands/`). Each
has an id, a title, an optional category, optional keybindings (`mod` means Ctrl, or Cmd on
macOS, with per-platform overrides), an `enabled` predicate, and an `editorFocus` flag for
editor-only keys. One capture-phase listener runs bound commands. The **command palette** lists
enabled commands with their shortcuts (`>`), finds files (no prefix), and goes to a line (`:`).
Editing keys that need the editor's textarea (arrows, Backspace, clipboard) stay in
`editor/keymap.ts`. Extension contributions (M7) will register into the same registry.

## The frontend–backend boundary

- **Rust owns authoritative state.** The frontend never holds a document's text; it renders views
  of the lines near the visible area.
- **Commands are the only entry point.** Tauri commands are defined in `src-tauri/src/`
  (`commands.rs`, `editor/commands.rs`, `lsp/commands.rs`, `workspace.rs`, `settings.rs`,
  `ui_state.rs`, `events.rs`). They are mirrored by typed wrappers in `src/ipc/`, the only frontend code that
  calls `invoke`.
- **Backend events** (workspace changed, files changed, document reloaded or changed on disk,
  settings changed, index status, match counts, syntax parsed, diagnostics changed, language
  server state) are pushed through one Tauri `Channel` that the frontend
  registers at startup, so the webview needs no event permissions.
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
editor_apply(id, ops[], viewport, known?, search?) → EditorView
known      = { version, startLine, endLine }          lines the client holds
EditorView = { version, lineCount, startLine, endLine,
               lines: { base, splices[], chunks[{ start, lines[] }] },
               selections[], selectionCount, primary: { position, visualColumn }, dirty,
               search?: { matches[] (visible only), total | null, truncated, current, error },
               highlights: [{ line, spans: [start, end, kind, ...] }]  changed lines only,
               diagnostics: [{ start, end, severity }]                  visible lines,
               language?: { language, server?: { name, state, message }, errors, warnings,
                            completionTriggers } }
```

- Positions on the wire are `{ line, column }` with UTF-16 columns, which are JavaScript string
  indices into the rendered line. Rust converts them against the document as it is when each
  operation applies.
- `EditorSession` (`src/editor/session.ts`) keeps at most one request in flight. Operations
  issued in the meantime are sent together as the next batch, so ordering is preserved and fast
  typing costs few round trips. Consecutive drag positions collapse to the latest one.
- **Views are incremental.** The viewport is the visible lines plus 20 lines of overscan above
  and below. Each request says which lines the client holds (`known`). The backend replays the
  buffer's line splices since that version (`base`) and sends only the lines the client is
  missing (`chunks`). The client replays the same splices on its cache (`src/editor/lineCache.ts`)
  and fills in the chunks. A keystroke sends one line, Enter shifts cached lines instead of
  resending them, and scrolling sends only newly exposed lines. A client too far behind (the log
  keeps 512 changes) gets a full update. An update the client can't apply makes it drop its cache
  and refetch, so a mismatch costs a round trip, never wrong text.
- Lines longer than 16 KiB arrive **shortened for display**, with their full length, and render
  with a "…" badge. Editing, saving, and search always use the whole line.
- Scrolling requests a new view only when the visible lines aren't already held.
- Before saving or closing, the workbench waits for the session to go idle, so the save includes
  every keystroke typed before it.
- While the find widget is open, its query rides along with every request, so highlights and the
  "3 of 17" count stay current as you type. For documents over 1 MiB, the total is counted on the
  scheduler from a snapshot. The view shows visible matches immediately and `total: null`
  ("Counting…") until a `searchCounted` event triggers a refresh. Find operations with an invalid regex are skipped,
  and the view reports the error instead.
- **Highlights are incremental too.** Spans use UTF-16 columns and token kinds numbered as in
  `TOKEN_KINDS`. Lines sent as chunks start with no spans; other held lines keep theirs. The
  backend remembers a hash of the spans it sent for each line the client holds, replays the
  line splices on those hashes, and sends spans only for lines whose hash changed. Typing on a
  line resends that line's spans; opening a block comment resends the lines it now covers.
- **Every view syncs the document with its language server first**, so diagnostics, hover, and
  completion requests always refer to text the server has.

| Command                                                       | Purpose                                                |
| ------------------------------------------------------------- | ------------------------------------------------------ |
| `app_info`, `frontend_log`, `frontend_ready`                  | Process metadata, logging, startup timing              |
| `document_list`                                               | Open documents (restores tabs after a webview reload)  |
| `document_new`                                                | New untitled document                                  |
| `document_open`, `document_open_dialog`                       | Open a path, or show the native open dialog            |
| `document_save`, `document_save_as`                           | Save, prompting for a path or an overwrite when needed |
| `document_close`                                              | Close, asking about unsaved changes                    |
| `editor_view`                                                 | View for a viewport (after scrolling)                  |
| `editor_apply`                                                | Apply a batch of operations, return the new view       |
| `editor_clipboard`                                            | Copy or cut the selection to the system clipboard      |
| `workspace_get`, `workspace_open[_dialog]`, `workspace_close` | The open folder                                        |
| `workspace_list_dir`                                          | One explorer directory listing                         |
| `workspace_find_files`                                        | Fuzzy Go to File over the index                        |
| `workspace_search`, `workspace_search_cancel`                 | Streaming find in files                                |
| `settings_get`, `settings_file`, `settings_set_theme`         | Frontend settings, the settings file path, theme       |
| `ui_state_get`, `ui_state_save`                               | Saved layout and active file                           |
| `events_subscribe`                                            | Register the backend event channel                     |
| `perf_stats`                                                  | Backend timings, scheduler statistics, memory          |
| `lsp_hover`, `lsp_definition`, `lsp_completion`               | Language server requests at a position                 |
| `bench_config`, `bench_generate`, `bench_report`              | Benchmark mode only (`--bench <name>`)                 |

### Permissions, dialogs, and clipboard

The main window's capability grants **no** Tauri core or plugin permissions. File dialogs,
message dialogs, and clipboard writes (`tauri-plugin-dialog`, `tauri-plugin-clipboard-manager`)
are called from Rust, so the webview can't open arbitrary dialogs or read the clipboard. Copied
text goes from the backend straight to the clipboard. Paste uses the webview's native `paste`
event, which needs no permission.

The production Content Security Policy allows only same-origin scripts, IPC, and inline styles.
The development CSP additionally allows Vite's HMR socket and inline scripts.

## Syntax highlighting

`ferrite-syntax` bundles the grammars; the app keeps one Tree-sitter tree per document of a known
language (`editor/syntax.rs`), detected from the file name and re-detected after Save As.

- **Edits update the tree immediately.** On each view, the byte edits since the tree's version
  (from the buffer's edit log, with row/column points) are applied with `Tree::edit`, so the old
  tree's spans stay aligned with the text right away.
- **Reparsing is incremental and budgeted.** With a tree, an incremental reparse runs inline
  under the editor lock, stopped by Tree-sitter's progress callback after 3 ms. If it doesn't
  finish (or there is no tree yet), the parse runs on the scheduler (keyed `parse:<id>`, high
  priority) over a snapshot, with the edited old tree, and can be cancelled. Its result is caught
  up with any edits made meanwhile, stored, and announced with a `syntaxUpdated` event that makes
  the editor refetch. Only one background parse per document runs at a time, so continuous typing
  in a large file costs at most one wasted 3 ms attempt per background parse.
- **Highlighting covers only the viewport.** The highlight query runs over the visible lines'
  byte range (each line limited to the 16 KiB display limit), reading the rope through a
  `TextProvider` without copying the document. Overlapping captures resolve by painting wide
  captures first and narrow ones over them, with earlier query patterns winning for the same
  node, which is the convention highlight queries are written for. Capture names map to a fixed
  set of token kinds (`TokenKind`), styled by `--color-syntax-*` tokens.
- Grammars and queries are compiled once, on first use of each language. Documents over 16 MiB
  aren't highlighted.

## Language servers

`ferrite-lsp` runs servers and speaks the protocol; the app decides which documents go to which
server (`lsp/`).

- **Servers start on demand.** The first view of a document whose language has a configured
  server opens it on the server for its root (the workspace folder if it contains the file, else
  the nearest ancestor with a root marker such as `Cargo.toml`, else its directory). The server
  is started on its own thread; until it is up, the status bar shows "starting", and when it is
  up every waiting document is opened. Known servers are rust-analyzer, typescript-language-server,
  and pyright; settings (`[lsp]`) can change or add servers or turn them off. A server that isn't
  installed is reported once in the status bar, not retried.
- **One connection, three threads.** A writer thread drains a queue to the server's stdin, so
  sending never blocks the caller (or the editor lock); a reader thread parses
  `Content-Length`-framed messages (at most 64 MiB) and routes responses to waiting requests and
  notifications to the app; a third forwards stderr to the log. The client answers the requests
  servers send (configuration, progress tokens, capability registration) with defaults.
- **Positions.** The client offers UTF-8 and UTF-16 position encodings, preferring UTF-8, which is
  Ferrite's native unit, so rust-analyzer needs no conversion. For UTF-16 servers, positions are
  converted with the rope's indices.
- **Document sync is incremental.** Each view sends the buffer's edits since the last sync as
  `didChange` ranges (for UTF-8, straight from the edit log's points; for UTF-16, by replaying
  the edits on a copy of the last synced snapshot, which is cheap with a rope). Servers that only
  accept full text get full text; documents over 8 MiB aren't sent. `didSave` and `didClose`
  follow the document's lifecycle.
- **Diagnostics are anchored to text.** A `publishDiagnostics` notification is interpreted
  against the snapshot of the version it names (the last 16 synced snapshots are kept), stored
  as byte ranges, and mapped through later edits when shown, so squiggles move with the text
  while the server catches up. Each view carries the diagnostics on its lines and the document's
  error and warning counts.
- **Requests never hold locks while waiting.** `lsp_hover`, `lsp_definition`, and
  `lsp_completion` sync the document, capture the position and a snapshot under the locks,
  then wait on a blocking thread (5 s timeout). Completion results are filtered and ranked in
  Rust (prefix, case-insensitive prefix, then letters in order; at most 200) and their edit
  positions are mapped to the current text. The frontend narrows them further as typing
  continues, asking again only for incomplete lists. Requests the server cancels or invalidates
  (`ContentModified`) are dropped quietly. Abandoned requests are cancelled with
  `$/cancelRequest`.
- **Failures are contained.** A server that exits fails its pending requests, drops its
  documents' diagnostics, and is restarted on demand up to three times, then marked failed.
  Opening or closing a folder, or changing `[lsp]` settings, restarts all servers. On exit, the
  app asks every server to shut down and kills those that don't within 2 seconds.

## Frontend structure

- **`ipc/`**: typed command wrappers. `call()` normalizes failures to `CommandError`.
- **`state/`**: small observable stores (`createStore` + `useSyncExternalStore`): `documents`
  (tab summaries, order, the active id, and actions), `workspace` (folder and loaded listings),
  `search` (find-in-files results), `layout` (persisted), `appearance` (theme and font),
  `editorStatus`, and `notifications`. Each component subscribes only to what it reads, so a
  keystroke re-renders the editor view and status bar, not the workbench.
- **`commands/`**: the command registry, keybinding matching, fuzzy matching, and the built-in
  commands. `editor/activeEditor.ts` exposes the active editor to commands (find, go to line,
  next match) without coupling them to React.
- **`editor/EditorView`**: a scroll container sized to `lineCount × lineHeight`. Only the
  viewport's lines are in the DOM, positioned with a single `translateY`. Each line is a run of
  text nodes, plain or in `tok-*` spans for highlighting. Carets, selection rectangles, find
  matches, and diagnostic squiggles are measured from the DOM with `Range` after each render
  (`editor/measure.ts` maps UTF-16 columns across a line's text nodes), and clicks map back to
  columns with `caretPositionFromPoint`, so proportional glyphs, tabs, and wide characters line
  up without a custom text-layout engine.
- **Language features**: resting the pointer on text shows a hover card (diagnostics at that
  point plus the server's Markdown, rendered as React elements, never HTML); F12 or Ctrl/Cmd+click
  goes to the definition; completion opens on the server's trigger characters, on typing a word,
  or with Ctrl+Space, and is navigated with the arrow keys and accepted with Enter or Tab. The
  status bar shows problem counts, the language, and the server's state.
- **Input**: a hidden textarea at the primary caret receives keys, text, IME composition (shown
  inline while composing), and paste. `editor/keymap.ts` maps keys to operations per platform
  (Ctrl on Windows/Linux, Cmd/Option on macOS). Commands' keybindings are handled in the capture
  phase before the editor sees the key.
- **Tabs** can be reordered by dragging, cycled with Ctrl+Tab and Ctrl+PageUp/PageDown, and
  remember their scroll position while the app runs. Clicking a tab returns focus to its editor.
- **Theming**: `styles/tokens.css` defines semantic tokens for dark and light themes (including
  find matches, widgets, inputs, lists, and buttons), selected by `data-theme` on `<html>`.
  Components use tokens only. The `workbench.theme` setting picks dark, light, or the OS scheme.
  Editor font size, family, line height, and tab width are CSS variables set from settings.
- **Accessibility**: the tab strip and tab panels use ARIA tab roles, and unsaved tabs say so in
  their accessible name. The explorer is an ARIA `tree` with arrow-key navigation, and the palette
  is a combobox with `aria-activedescendant`. Find options are toggle buttons, errors are
  announced through `role="alert"`, and the sash is keyboard adjustable. Screen readers can't yet
  read the editor's text, because the input textarea is empty; that's planned for M8.

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

## Instrumentation

- **Backend:** named timing histograms (`ferrite_tasks::Metrics`; power-of-two buckets, O(1)
  memory) for `editor.apply`, `editor.view`, `document.load`, `document.save`,
  `workspace.index`, `workspace.search`, and `search.count`. The scheduler also keeps wait and run
  times per task type, with completed, cancelled, dropped, and panicked counts.
- **Frontend** (`src/state/perf.ts`): series for IPC round trips, render and measure time,
  input-to-paint latency, and lines received per update.
- **Overlay:** **Developer: Toggle Performance Overlay** shows both, plus backend and JS heap
  memory and open document sizes.
- **Benchmarks:** see [`performance.md`](performance.md) for the benchmark suites and the
  measured baseline.

## Startup measurement

`run()` records an `Instant` on entry. The backend logs `elapsed_ms` when setup finishes and when
the frontend first calls `frontend_ready`. `scripts/bench.mjs startup` measures this
repeatably; see [`performance.md`](performance.md) for the current numbers. Files from the
previous session and the command line now open on the scheduler, so they no longer delay the
window.

Single, warm-cache runs on the development machine (Windows 11, x86_64), for orientation only:

| Build / scenario                                        | `backend initialized` | `frontend ready` |
| ------------------------------------------------------- | --------------------- | ---------------- |
| M1 debug                                                | 490 ms                | 673 ms           |
| M1 release                                              | 332 ms                | 372 ms           |
| M2 debug, 11 MB / 200,000-line file on the command line | 333 ms                | 629 ms           |
| M3 debug, this repository restored as the workspace     | 422 ms                | 584 ms           |

Indexing this repository (149 non-ignored files) took 13 ms on the index thread, after the
window was already interactive.

The M1 release `ferrite.exe` process had a working set of about 30 MB. WebView2 runs in separate
processes, which this figure excludes.

## Known limitations

These are tracked for later milestones:

- Lines longer than 16 KiB are shown only up to that point (with a badge). The cursor can't be
  seen beyond it, though editing there works. Visual columns past 4 KiB count characters, not
  tab stops or grapheme clusters.
- Files load completely into memory (up to 256 MiB); there's no lazy or partial loading.
- Replace across files, include/exclude globs in the search panel, and search-result context
  lines aren't implemented.
- The explorer can't create, rename, move, or delete files yet. Large directories aren't
  virtualized.
- Only files inside the open folder are watched. Open files outside it don't reload on external
  changes, but saving still detects conflicts.
- Single folder only; multi-root workspaces aren't supported.
- Scroll positions are remembered per tab only while the app runs. Unsaved changes aren't
  persisted across restarts (closing asks about them).
- Language features are limited to diagnostics, hover, go to definition, and completion. Find
  references, rename, code actions, formatting, signature help, and semantic tokens aren't
  implemented, nor are completion's additional edits (such as auto-imports) or snippets
  (completion asks servers for plain text). There's no problems panel; diagnostics for files that
  aren't open aren't shown.
- Highlighting uses each grammar's bundled query; there's no injection support (e.g. code in
  Markdown or doc comments) and no language detection beyond file names.
- There's no graphical settings UI; settings are edited as TOML. Keybindings aren't configurable
  yet (M7).
- Multi-line Tab/Shift+Tab indentation, auto-closing brackets, and copying the whole line when
  nothing is selected aren't implemented. Alt+drag doesn't create column selections.
- Errors opening files at startup are only logged, not shown.
- On Linux, watching very large trees can exhaust the inotify watch limit; watching then fails
  with a logged warning and the workspace works without live updates.
