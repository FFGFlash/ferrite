# Ferrite

A fast, memory-efficient, extensible code editor built with Rust, Tauri, React, and a sandboxed
WebAssembly extension system.

> **Status:** Milestone 2 (text editing foundation) is complete. Ferrite opens, edits, and saves
> UTF-8 files with multiple cursors, undo/redo, and unsaved-changes protection. The file explorer,
> command palette, and search arrive in Milestone 3. See [`docs/DESIGN.md`](docs/DESIGN.md) for the
> full brief and milestone plan.

## Prerequisites

| Tool                                                | Version used                                 |
| --------------------------------------------------- | -------------------------------------------- |
| [Rust](https://rustup.rs) (stable, MSVC on Windows) | 1.99.0                                       |
| [Node.js](https://nodejs.org)                       | 26.x                                         |
| [pnpm](https://pnpm.io)                             | 10.27.0 (pinned via `packageManager`)        |
| Platform dependencies for Tauri 2                   | see the [Tauri prerequisites][tauri-prereqs] |

On Windows this means the Visual Studio C++ build tools and the WebView2 runtime (preinstalled on
Windows 11). Linux needs WebKitGTK 4.1 development packages.

[tauri-prereqs]: https://v2.tauri.app/start/prerequisites/

## Getting started

```sh
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` starts the Vite dev server on port 1420, builds the Rust backend, and opens the
desktop window. Frontend changes hot-reload; Rust changes trigger a rebuild and restart.

Files passed on the command line open as tabs, for example
`target/debug/ferrite path/to/file.rs` (the debug binary needs `pnpm dev` running).

## Editing

Ferrite currently opens UTF-8 text files of up to 256 MiB. Line endings and a UTF-8 BOM are
preserved exactly, and new lines use the file's existing line ending.

| Action                          | Windows / Linux               | macOS                |
| ------------------------------- | ----------------------------- | -------------------- |
| New file / Open / Close tab     | Ctrl+N / Ctrl+O / Ctrl+W      | ⌘N / ⌘O / ⌘W         |
| Save / Save As                  | Ctrl+S / Ctrl+Shift+S         | ⌘S / ⌘⇧S             |
| Undo / Redo                     | Ctrl+Z / Ctrl+Y, Ctrl+Shift+Z | ⌘Z / ⌘⇧Z             |
| Cut / Copy / Paste / Select all | Ctrl+X / C / V / A            | ⌘X / C / V / A       |
| Move by word                    | Ctrl+←/→                      | ⌥←/→                 |
| Line start (smart) / end        | Home / End                    | ⌘← / ⌘→              |
| Document start / end            | Ctrl+Home / Ctrl+End          | ⌘↑ / ⌘↓              |
| Delete word                     | Ctrl+Backspace / Ctrl+Delete  | ⌥Backspace / ⌥Delete |
| Add cursor above / below        | Ctrl+Alt+↑ / ↓                | ⌘⌥↑ / ↓              |
| Add cursor at click             | Alt+click                     | ⌥click               |
| Select word                     | Double-click                  | Double-click         |
| Back to one cursor              | Escape                        | Escape               |

Hold Shift with any movement key to extend the selection. Closing a tab or the window with
unsaved changes asks whether to save them. If another program changed a file since it was
opened, saving asks before overwriting it.

## Commands

| Command                                                 | What it does                                                    |
| ------------------------------------------------------- | --------------------------------------------------------------- |
| `pnpm tauri dev`                                        | Run the desktop app in development mode                         |
| `pnpm tauri build`                                      | Build a release binary and platform installers                  |
| `pnpm tauri build --no-bundle`                          | Build the release binary only (`target/release/ferrite[.exe]`)  |
| `pnpm dev`                                              | Run only the frontend in a browser (no backend; IPC is stubbed) |
| `pnpm test`                                             | Run frontend tests (Vitest + Testing Library)                   |
| `pnpm typecheck`                                        | Type-check the frontend                                         |
| `pnpm format`                                           | Format frontend (Prettier) and Rust (`cargo fmt`) sources       |
| `pnpm format:check`                                     | Check formatting without writing                                |
| `cargo test --workspace`                                | Run Rust tests                                                  |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint Rust sources                                               |

## Logging

The backend logs through [`tracing`](https://docs.rs/tracing) to stderr and to a daily rolling
file (seven files retained) in the platform log directory:

| Platform | Log directory                            |
| -------- | ---------------------------------------- |
| Windows  | `%LOCALAPPDATA%\dev.ferrite.editor\logs` |
| macOS    | `~/Library/Logs/dev.ferrite.editor`      |
| Linux    | `~/.local/share/dev.ferrite.editor/logs` |

Set `FERRITE_LOG` to change the filter, using
[`EnvFilter` syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).
The default is `info`. Frontend messages are logged under the `frontend` target:

```sh
FERRITE_LOG=debug,frontend=trace pnpm tauri dev
```

Startup timing is logged as `backend initialized` and `frontend ready`, each with `elapsed_ms`
measured from the start of `ferrite_app::run`.

## Repository layout

```text
Cargo.toml            Cargo workspace
package.json          Frontend package and scripts
crates/
  ferrite-text/       Rope-backed text buffer, change sets, positions, Unicode boundaries
  ferrite-workspace/  Loading and safely saving files
  ferrite-core/       Documents, selections, editing operations, undo, dirty state
src-tauri/            ferrite-app crate: Tauri shell, lifecycle, IPC commands, dialogs, logging
src/                  React + TypeScript workbench
  app/                Root component, workbench layout, view registry, shortcuts
  components/         Workbench chrome (activity bar, side bar, sash, status bar, notifications)
  editor/             Editor view, tabs, input keymap, backend session
  panels/             Side bar views
  state/              Small observable stores (documents, editor status, notifications)
  ipc/                Typed wrappers for Tauri commands (the only place that calls invoke)
  themes/             Theme selection
  styles/             Design tokens and base styles
docs/                 Architecture notes and ADRs
```

See [`docs/architecture.md`](docs/architecture.md) for how these pieces fit together and how the
layout will grow.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
