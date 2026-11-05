# ADR 0003: Workbench services

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

Milestone 3 turns Flareite into a usable programming editor: a file explorer, a command palette,
search and replace, theme tokens, persistent layout, and keyboard shortcuts. These features need
workspace-wide services (directory listing, a file index, search, file watching), a settings
system, and a way to name and bind actions. The brief requires background work that never
blocks typing, incremental updates, respect for ignore rules, bounded caches, and no eager
reading of workspace files.

## Decisions

### Reuse ripgrep's and Helix's building blocks

- **`ignore`** (ripgrep) for directory walks that respect `.gitignore`, `.ignore`, and global git
  excludes, and **`globset`** for user exclusions.
- **`regex-cursor`** (Helix) to search the rope's chunks directly. It's built on `regex-automata`
  0.4, the engine behind `regex`, so syntax and semantics match. In-editor find never copies a
  document into a string. Capture-group replacement uses `regex-automata`'s interpolation.
- **`nucleo-matcher`** (Helix) for fuzzy path matching in Go to File.
- **`notify`** through **`notify-debouncer-mini`** for file watching, debounced to 250 ms.

The less widely used crates (`regex-cursor`, `nucleo-matcher`) are small, pinned, and used
behind our own `Searcher` and `FileIndex` types, so either can be replaced without touching
callers.

### Lazy explorer, background index, streaming search

The explorer lists one directory at a time, only when expanded. The Go to File index walks the
workspace once on a background thread, stores relative paths only (bounded at 200,000), and is
then updated incrementally from watcher events. Find in files is a separate streaming walk that
reads files one at a time and never caches their contents. Each piece is cancellable, so opening
another folder or starting another search stops the old work.

Ignored files are **shown dimmed** in the explorer (you can still open build output or
`node_modules`) but skipped by the index and search, where they would drown out results.

### External changes: reload clean documents, never touch dirty ones

When a watched file changes, a clean document reloads in place. Only the changed middle section
is replaced, so cursors and scroll position stay meaningful, and the reload can be undone. A
dirty document is never modified; the user is notified, and the save-time conflict check from
M2 asks before overwriting. Flareite's own saves are recognized by their recorded stamp and
ignored.

### Replace across files is deferred

In-editor replace (including Replace All as one undo step) is implemented. Replace across files
is not. Doing it safely means editing many files at once: open documents go through the core so
the change can be undone, closed files must be written atomically with conflict checks, and the
user needs a preview and a way to cancel partway. That's better built on M4's background-task
scheduling than squeezed into M3.

### Settings as a TOML file edited in Flareite

Settings are a validated `settings.toml` in the config directory, opened as a normal document;
saving applies them. This needs no settings UI yet, is easy to diff and share, and reuses the
editor itself. Unknown keys are rejected rather than ignored, so typos are reported. Programmatic
changes (the Color Theme commands) use `toml_edit` to keep the user's comments and formatting. A
graphical settings editor can be layered on top later.

Session state (open folder, open files, layout) is kept separately in `state.json`. It's
machine-written and not meant for hand editing, and keeping it apart means a corrupt state file
can never invalidate the user's settings.

### Frontend command registry

Every workbench action is a registered command with an id, a title, optional keybindings, and an
`enabled` predicate. The palette, the welcome page, and the keybinding dispatcher all read from
the registry instead of from hard-coded handlers. Extension-contributed commands and keybindings
(M7) will register into the same structure. Low-level editing keys stay in the editor's keymap,
because they need the textarea's focus and IME state.

### One event channel from the backend

Backend notifications (files changed, document reloaded, settings changed, …) go through a
single Tauri `Channel` the frontend registers at startup, rather than through Tauri's global
event system. A channel is part of IPC, so the webview's capability still grants no permissions.

## Consequences

- The workspace stays responsive while indexing: the window is interactive before the index is
  built, and Go to File reports that indexing is still running.
- Watching a whole tree recursively can hit OS limits (inotify on Linux). Watching then fails
  with a logged warning, and the workspace works without live updates.
- Settings are only re-read when saved from Flareite; editing `settings.toml` in another program
  takes effect after a restart (the settings file isn't watched).
- Files outside the open folder aren't watched; their conflicts are still caught at save time.
